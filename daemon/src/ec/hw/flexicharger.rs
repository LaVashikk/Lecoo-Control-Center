use super::EcDevice;
use crate::ec::{Addr, ChargeOps};
use anyhow::{Context, Result, bail};
use lecoo_types::ec_types::{ChargeIntent, ChargeRange};

pub enum Availability {
    Ready,
    /// Text is meant for a GUI tooltip, not a log line.
    Blocked(String),
    Unsupported(String),
}

// ---------- dispatch ----------

pub fn charge_availability(ec: &EcDevice, intent: &ChargeIntent) -> Result<Availability> {
    if !ec.profile.supports_intent(intent) {
        return Ok(Availability::Unsupported(unsupported_reason(ec, intent)));
    }

    match ec.profile.charge {
        ChargeOps::None => {
            Ok(Availability::Unsupported(format!("Board {} has no charge control", ec.profile.id)))
        }
        ChargeOps::FlexiCharger { .. } => Ok(Availability::Ready),
        ChargeOps::PolicyByte { arm_below, freeze_soc, .. } => {
            policy_byte::availability(ec, intent, arm_below, freeze_soc)
        }
    }
}

pub fn apply_charge(ec: &EcDevice, intent: &ChargeIntent) -> Result<()> {
    if !ec.profile.supports_intent(intent) {
        bail!("{}", unsupported_reason(ec, intent));
    }
    match ec.profile.charge {
        ChargeOps::None => {
            bail!("Board {} has no charge control", ec.profile.id)
        }
        ChargeOps::FlexiCharger { min, max, full_max, .. } => flexi::apply(ec, intent, min, max, full_max),
        ChargeOps::PolicyByte { reg, .. } => policy_byte::apply(ec, intent, reg),
    }
}

/// What the hardware is doing right now. Source of truth for `charge status`.
pub fn effective_charge(ec: &EcDevice) -> Result<(ChargeIntent, Option<(u8, u8)>)> {
    match ec.profile.charge {
        ChargeOps::None => Ok((ChargeIntent::Full, None)),
        ChargeOps::FlexiCharger { min, max, full_max, .. } => flexi::effective(ec, min, max, full_max),
        ChargeOps::PolicyByte { reg, fw_thresholds, .. } => policy_byte::effective(ec, reg, fw_thresholds), // TODO: from memory prob
    }
}

/// `effective_charge` named the way the user asked for it, so it can be compared with `desired`.
/// PolicyByte fw sets the latch bit itself once `lifespan` tops out, and then 0x07 means both "hold" and "lifespan, paused"
pub fn effective_for(ec: &EcDevice, desired: &ChargeIntent) -> Result<(ChargeIntent, Option<(u8, u8)>)> {
    let raw = effective_charge(ec)?;
    Ok(match (ec.profile.charge, raw.0, desired) {
        // lifespan at its top, the fw latched it. For the fw a hold IS this same state anyway
        (ChargeOps::PolicyByte { fw_thresholds, .. }, ChargeIntent::Freeze, ChargeIntent::Preserve(None)) => {
            (ChargeIntent::Preserve(None), Some(fw_thresholds))
        }
        // NOT mirrored: hold whose latch fw dropped below 61% really stopped holding, reconcile re-applies it in 62..84
        _ => raw,
    })
}

fn unsupported_reason(ec: &EcDevice, intent: &ChargeIntent) -> String {
    match (ec.profile.charge, intent) {
        (ChargeOps::PolicyByte { fw_thresholds, .. }, ChargeIntent::Preserve(Some(_))) => {
            format!(
                "{} cannot take a custom range: firmware owns the thresholds (~{}-{}%). Use the `lifespan` preset.",
                ec.profile.id, fw_thresholds.0, fw_thresholds.1
            )
        }
        (ChargeOps::FlexiCharger { bounds, .. }, ChargeIntent::Preserve(Some(r))) => {
            format!("Range {}-{} is outside {}..={}", r.min, r.max, bounds.0, bounds.1)
        }
        (ChargeOps::FlexiCharger { .. }, ChargeIntent::Freeze) => {
            format!("Freeze is not confirmed on {} yet", ec.profile.id)
        }
        _ => {
            format!("{} does not support this charge mode", ec.profile.id)
        }
    }
}

// ---------- shared helpers ----------

pub fn read_battery_rsoc(ec: &EcDevice) -> Result<u8> {
    let bat = ec.profile.battery.context("Board has no battery spec")?;
    ec.read(bat.rsoc)
}

/// Kept for IpcResponse::ChargeLimit and the battery LED logic.
pub fn read_charge_limit(ec: &EcDevice) -> Result<(u8, u8)> {
    match ec.profile.charge {
        ChargeOps::FlexiCharger { min, max, .. } => ec.with_batch(|b| Ok((b.read(min)?, b.read(max)?))),

        ChargeOps::PolicyByte { reg, fw_thresholds, .. } => {
            let (intent, _) = policy_byte::effective(ec, reg, fw_thresholds)?;
            Ok(match intent {
                ChargeIntent::Preserve(None) => fw_thresholds,
                ChargeIntent::Freeze => (read_battery_rsoc(ec)?, read_battery_rsoc(ec)?),
                _ => (0, 100),
            })
        }

        ChargeOps::None => Ok((0, 100)),
    }
}

/// SoC at which charging is expected to stop. Never None, so callers such as
/// the battery LED handler keep working on every board.
pub fn charge_stop_level(ec: &EcDevice) -> u8 {
    match effective_charge(ec) {
        Ok((ChargeIntent::Preserve(Some(r)), _)) => r.max,
        Ok((ChargeIntent::Preserve(None), Some((_, max)))) => max,
        // holding, the battery is not going anywhere
        Ok((ChargeIntent::Freeze, _)) => 0,
        _ => 100,
    }
}

fn is_charging(ec: &EcDevice) -> Option<bool> {
    let addr = ec.profile.battery?.charge_current?;
    // whole word: a single byte here is the high one, 0 for any trickle under 256 mA
    ec.read_u16_be(addr).ok().map(|v| v != 0)
}

/// Idempotent. Call at startup, on AC connect and on wake.
pub fn reconcile(ec: &EcDevice, desired: &ChargeIntent) -> Result<Option<String>> {
    let (effective, _) = effective_for(ec, desired)?;
    if effective == *desired {
        return Ok(None);
    }

    match charge_availability(ec, desired)? {
        Availability::Ready => {
            apply_charge(ec, desired)?;
            Ok(None)
        }

        Availability::Blocked(reason) => {
            // debug: the metrics tick retries this every round
            log::debug!("Charge policy pending: {reason}");
            Ok(Some(reason))
        }

        Availability::Unsupported(reason) => bail!(reason),
    }
}

// ---------- FlexiCharger ----------
// Controlled by the FlexiCharger daemon (software)

mod flexi {
    use super::*;

    pub fn apply(ec: &EcDevice, intent: &ChargeIntent, min: Addr, max: Addr, full_max: u8) -> Result<()> {
        let (lo, hi) = match intent {
            ChargeIntent::Full => (0, full_max),
            ChargeIntent::Preserve(Some(r)) => (r.min, r.max),
            ChargeIntent::Preserve(None) => bail!("Firmware-managed preserve is not available on board"),
            ChargeIntent::Freeze => {
                let soc = super::read_battery_rsoc(ec)?;
                (soc, soc)
            }
        };

        // set charge limits
        ec.with_batch(|b| {
            b.write(min, lo)?;
            b.write(max, hi)
        })?;

        Ok(())
    }

    pub fn effective(
        ec: &EcDevice,
        min: Addr,
        max: Addr,
        full_max: u8,
    ) -> Result<(ChargeIntent, Option<(u8, u8)>)> {
        let (lo, hi) = ec.with_batch(|b| Ok((b.read(min)?, b.read(max)?)))?;
        let intent = if hi == full_max || (full_max == 100 && hi >= 100) || (lo == 0 && hi == 0) {
            ChargeIntent::Full
        } else if lo >= hi {
            ChargeIntent::Freeze
        } else {
            ChargeIntent::Preserve(Some(ChargeRange { min: lo, max: hi }))
        };
        Ok((intent, Some((lo, hi))))
    }
}

// ---------- policy byte for new revisions ----------

mod policy_byte {
    use super::*;

    const BIT_ENABLE: u8 = 0x01;
    const BIT_POLICY: u8 = 0x02;
    const BIT_LATCH: u8 = 0x04;
    const OWNED_MASK: u8 = BIT_ENABLE | BIT_POLICY | BIT_LATCH;

    const V_FULL: u8 = 0x00;
    const V_PRESERVE: u8 = BIT_ENABLE | BIT_POLICY;
    const V_FREEZE: u8 = BIT_ENABLE | BIT_POLICY | BIT_LATCH;

    pub fn availability(
        ec: &EcDevice,
        intent: &ChargeIntent,
        arm_below: u8,
        freeze_soc: (u8, u8),
    ) -> Result<Availability> {
        let soc = super::read_battery_rsoc(ec)?;
        Ok(match intent {
            ChargeIntent::Full => Availability::Ready,

            ChargeIntent::Preserve(None) => {
                if soc > arm_below {
                    Availability::Blocked(format!(
                        "Will arm once the battery drops to {arm_below}% while on AC (now {soc}%)"
                    ))
                } else {
                    match super::is_charging(ec) {
                        Some(true) => Availability::Ready,
                        Some(false) => {
                            Availability::Blocked("Requires active charging: connect the AC adapter".into())
                        }
                        None => Availability::Blocked(
                            "Cannot confirm active charging: profile has no charge-current address".into(),
                        ),
                    }
                }
            }

            ChargeIntent::Freeze => {
                if (freeze_soc.0..=freeze_soc.1).contains(&soc) {
                    Availability::Ready
                } else {
                    Availability::Blocked(format!(
                        "Hold works between {}% and {}% (now {soc}%)",
                        freeze_soc.0, freeze_soc.1
                    ))
                }
            }

            ChargeIntent::Preserve(Some(_)) => {
                Availability::Unsupported(super::unsupported_reason(ec, intent))
            }
        })
    }

    /// No saved original byte: `resume` is just clearing our three bits, which
    /// keeps the operation idempotent and safe across reboots.
    pub fn apply(ec: &EcDevice, intent: &ChargeIntent, reg: Addr) -> Result<()> {
        let want = match intent {
            ChargeIntent::Full => V_FULL,
            ChargeIntent::Preserve(None) => V_PRESERVE,
            ChargeIntent::Freeze => V_FREEZE,
            ChargeIntent::Preserve(Some(_)) => bail!("{}", super::unsupported_reason(ec, intent)),
        };

        let after = ec.update_bits(reg, OWNED_MASK, want)?;
        if after & OWNED_MASK != want {
            bail!("Policy byte read-back mismatch: wanted {:#04X}, got {:#04X}", want, after & OWNED_MASK);
        }
        Ok(())
    }

    pub fn effective(ec: &EcDevice, reg: Addr, fw: (u8, u8)) -> Result<(ChargeIntent, Option<(u8, u8)>)> {
        Ok(match ec.read(reg)? & OWNED_MASK {
            V_FREEZE => (ChargeIntent::Freeze, None),
            V_PRESERVE => (ChargeIntent::Preserve(None), Some(fw)),
            _ => (ChargeIntent::Full, None),
        })
    }
}
