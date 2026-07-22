use anyhow::{Context, Result, bail};
use ipc::NativeChargeHoldStatus;

use super::EcDevice;

const POLICY_ADDR: u16 = 0x0414;
const RSOC_ADDR: u16 = 0x050F;
const CURRENT_TARGET_ADDR: u16 = 0x05D4;
const CURRENT_READBACK_ADDR: u16 = 0x05E4;
const NATIVE_MODE_MASK: u8 = 0x13;
const PRELATCHED_MODE_MASK: u8 = 0x17;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SavedMode {
    Hold,
    Native,
}

#[derive(Debug, Clone, Copy)]
struct SavedState {
    original: u8,
    mode: SavedMode,
}

#[cfg(windows)]
const HOLD_STATE_PATH: &str = "C:\\ProgramData\\LecooControl\\n161a_charge_hold_state";
#[cfg(not(windows))]
const HOLD_STATE_PATH: &str = "/var/lib/lecoo-control/n161a_charge_hold_state";

fn require_n161a(ec: &EcDevice) -> Result<()> {
    if !ec.offsets.n161a_native_charge_hold {
        bail!("native charge hold is supported only on the LECOO N161A");
    }
    Ok(())
}

fn save_original_policy(value: u8, mode: SavedMode) -> Result<()> {
    let path = std::path::Path::new(HOLD_STATE_PATH);
    let parent = path
        .parent()
        .context("invalid native charge-hold state path")?;
    std::fs::create_dir_all(parent)?;
    let mode = match mode {
        SavedMode::Hold => "hold",
        SavedMode::Native => "native",
    };
    std::fs::write(
        path,
        format!("version=1\noriginal={value:02X}\nmode={mode}\n"),
    )?;
    Ok(())
}

fn load_saved_state() -> Result<SavedState> {
    let value = std::fs::read_to_string(HOLD_STATE_PATH)
        .context("native charge-hold restore state was not found")?;
    let trimmed = value.trim();

    // Backward compatibility with the first test build's single hex byte.
    if let Ok(original) = u8::from_str_radix(trimmed, 16) {
        return Ok(SavedState {
            original,
            mode: SavedMode::Hold,
        });
    }

    let mut original = None;
    let mut mode = None;
    for line in trimmed.lines() {
        if let Some(value) = line.strip_prefix("original=") {
            original = Some(
                u8::from_str_radix(value, 16)
                    .context("native charge state has an invalid original policy")?,
            );
        } else if let Some(value) = line.strip_prefix("mode=") {
            mode = Some(match value {
                "hold" => SavedMode::Hold,
                "native" => SavedMode::Native,
                _ => bail!("native charge state has an unknown mode"),
            });
        }
    }
    Ok(SavedState {
        original: original.context("native charge state has no original policy")?,
        mode: mode.context("native charge state has no mode")?,
    })
}

fn hold_target(original: u8, rsoc: u8) -> Result<u8> {
    if rsoc <= 61 {
        bail!(
            "N161A charge hold requires charge above 61% and below 85% (current: {rsoc}%); at 61% or below use `lecoo-ctrl charge native`"
        );
    }
    if rsoc >= 85 {
        bail!(
            "N161A charge hold requires charge above 61% and below 85% (current: {rsoc}%); wait until charge falls below 85%"
        );
    }
    let mask = if rsoc <= 75 {
        PRELATCHED_MODE_MASK
    } else {
        NATIVE_MODE_MASK
    };
    Ok(original | mask)
}

fn read_word_is_zero(b: &crate::ec::EcBatch<'_>, addr: u16) -> Result<bool> {
    Ok(b.read_reg(addr)? == 0 && b.read_reg(addr + 1)? == 0)
}

fn wait_for_hold_confirmation(ec: &EcDevice, expected_policy: u8) -> Result<()> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    loop {
        let confirmed = ec.with_batch(|b| {
            let policy = b.read_reg(POLICY_ADDR)?;
            let target_zero = read_word_is_zero(b, CURRENT_TARGET_ADDR)?;
            let readback_zero = read_word_is_zero(b, CURRENT_READBACK_ADDR)?;
            Ok(
                (policy == expected_policy || policy & NATIVE_MODE_MASK == NATIVE_MODE_MASK)
                    && target_zero
                    && readback_zero,
            )
        })?;
        if confirmed {
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            bail!(
                "N161A charge hold was written but not confirmed within 15 seconds; do not repeat it automatically, use `lecoo-ctrl charge resume`"
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
}

fn wait_for_native_arm_confirmation(ec: &EcDevice) -> Result<()> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    loop {
        let confirmed = ec.with_batch(|b| {
            let policy = b.read_reg(POLICY_ADDR)?;
            let target_active = !read_word_is_zero(b, CURRENT_TARGET_ADDR)?;
            let readback_active = !read_word_is_zero(b, CURRENT_READBACK_ADDR)?;
            Ok(policy & NATIVE_MODE_MASK == NATIVE_MODE_MASK && target_active && readback_active)
        })?;
        if confirmed {
            return Ok(());
        }
        if std::time::Instant::now() >= deadline {
            bail!(
                "N161A native protection was written but active charging was not confirmed within 15 seconds; do not repeat automatically, use `lecoo-ctrl charge resume`"
            );
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
}

pub fn read_native_charge_hold(ec: &EcDevice) -> Result<NativeChargeHoldStatus> {
    if !ec.offsets.n161a_native_charge_hold {
        return Ok(NativeChargeHoldStatus::Unsupported);
    }

    ec.with_batch(|b| {
        let rsoc = b.read_reg(RSOC_ADDR)?;
        let policy = b.read_reg(POLICY_ADDR)?;
        if policy & NATIVE_MODE_MASK == NATIVE_MODE_MASK {
            let mode = load_saved_state().ok().map(|state| state.mode);
            if mode == Some(SavedMode::Native) {
                Ok(NativeChargeHoldStatus::NativeProtection { rsoc })
            } else {
                Ok(NativeChargeHoldStatus::Holding { rsoc })
            }
        } else {
            Ok(NativeChargeHoldStatus::Normal { rsoc })
        }
    })
}

pub fn enable_native_charge_hold(ec: &EcDevice) -> Result<()> {
    require_n161a(ec)?;

    let (rsoc, original, target_zero, readback_zero) = ec.with_batch(|b| {
        let rsoc = b.read_reg(RSOC_ADDR)?;
        let original = b.read_reg(POLICY_ADDR)?;
        let target_zero = read_word_is_zero(b, CURRENT_TARGET_ADDR)?;
        let readback_zero = read_word_is_zero(b, CURRENT_READBACK_ADDR)?;
        Ok((rsoc, original, target_zero, readback_zero))
    })?;

    // Range validation must precede the idempotent-policy check. Below the
    // native release boundary the root bits can remain set after the actual
    // current-zero latch has already been cleared by firmware.
    let target = hold_target(original, rsoc)?;

    if original & NATIVE_MODE_MASK == NATIVE_MODE_MASK {
        // We may only treat this as our own idempotent request when the exact
        // restore byte is still available. Never adopt an unknown native mode.
        let saved = load_saved_state().context(
            "native charge hold is active, but no valid restore state exists; refusing to adopt it",
        )?;
        if saved.mode != SavedMode::Hold {
            bail!(
                "N161A native protection is already active; run `lecoo-ctrl charge resume` before switching to hold"
            );
        }
        if target_zero && readback_zero {
            log::info!(
                "N161A native charge hold is already active at {}%; no EC write was needed",
                rsoc
            );
            return Ok(());
        }
        if target_zero || readback_zero {
            bail!(
                "N161A charge-current target and readback disagree; wait and query status before retrying"
            );
        }

        // Firmware can keep the root mode after releasing bit2 below 60%.
        // A valid-range hold request may re-establish that one latch without
        // discarding the original restore byte.
        let rearmed = original | PRELATCHED_MODE_MASK;
        ec.write_reg(POLICY_ADDR, rearmed)?;
        wait_for_hold_confirmation(ec, rearmed)?;
        log::info!(
            "Re-armed N161A native charge hold at {}%: 0x{:02X} -> 0x{:02X}",
            rsoc,
            original,
            rearmed
        );
        return Ok(());
    }
    if original & 0x02 != 0 {
        bail!(
            "N161A policy bit1 is already set in an unrecognized state (policy 0x{original:02X})"
        );
    }
    if target_zero || readback_zero {
        bail!(
            "N161A charge hold requires active charging current; connect AC power and wait for charging to begin"
        );
    }

    // Persist the exact restore byte before the single EC write.
    save_original_policy(original, SavedMode::Hold)?;
    ec.write_reg(POLICY_ADDR, target)?;
    wait_for_hold_confirmation(ec, target)?;
    log::info!(
        "Enabled N161A native charge hold at {}%: 0x{:02X} -> 0x{:02X}",
        rsoc,
        original,
        target
    );
    Ok(())
}

pub fn enable_native_charge_protection(ec: &EcDevice) -> Result<()> {
    require_n161a(ec)?;

    let (rsoc, original, target_zero, readback_zero) = ec.with_batch(|b| {
        Ok((
            b.read_reg(RSOC_ADDR)?,
            b.read_reg(POLICY_ADDR)?,
            read_word_is_zero(b, CURRENT_TARGET_ADDR)?,
            read_word_is_zero(b, CURRENT_READBACK_ADDR)?,
        ))
    })?;

    if original & NATIVE_MODE_MASK == NATIVE_MODE_MASK {
        let saved = load_saved_state().context(
            "native charge policy is active, but no valid restore state exists; refusing to adopt it",
        )?;
        if saved.mode == SavedMode::Native {
            log::info!(
                "N161A native charge protection is already armed at {}%; no EC write was needed",
                rsoc
            );
            return Ok(());
        }
        bail!(
            "N161A charge hold is already active; run `lecoo-ctrl charge resume` before switching to native protection"
        );
    }
    if original & 0x02 != 0 {
        bail!(
            "N161A policy bit1 is already set in an unrecognized state (policy 0x{original:02X})"
        );
    }
    if rsoc > 60 {
        bail!(
            "experimental N161A native protection must initially be armed at or below 60% (current: {rsoc}%)"
        );
    }
    if target_zero || readback_zero {
        bail!(
            "N161A native protection requires active charging current; connect AC power and wait for charging to begin"
        );
    }

    let target = original | NATIVE_MODE_MASK;
    save_original_policy(original, SavedMode::Native)?;
    ec.write_reg(POLICY_ADDR, target)?;

    wait_for_native_arm_confirmation(ec)?;

    log::info!(
        "Armed N161A native charge protection at {}%: 0x{:02X} -> 0x{:02X}",
        rsoc,
        original,
        target
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_outside_confirmed_range() {
        assert!(hold_target(0x05, 61).is_err());
        assert!(hold_target(0x05, 85).is_err());
    }

    #[test]
    fn prelatches_lower_band() {
        assert_eq!(hold_target(0x01, 62).unwrap(), 0x17);
        assert_eq!(hold_target(0x01, 75).unwrap(), 0x17);
    }

    #[test]
    fn uses_native_entry_in_upper_band() {
        assert_eq!(hold_target(0x01, 76).unwrap(), 0x13);
        assert_eq!(hold_target(0x01, 84).unwrap(), 0x13);
    }
}

pub fn resume_native_charging(ec: &EcDevice) -> Result<()> {
    require_n161a(ec)?;
    let current = ec.read_reg(POLICY_ADDR)?;
    if current & NATIVE_MODE_MASK != NATIVE_MODE_MASK {
        // The EC releases this latch itself below the native lower boundary.
        // In that case resume is an idempotent cleanup operation, not a write.
        if std::path::Path::new(HOLD_STATE_PATH).exists() {
            std::fs::remove_file(HOLD_STATE_PATH)?;
        }
        log::info!(
            "N161A native charge hold is already inactive (policy 0x{:02X}); no EC write was needed",
            current
        );
        return Ok(());
    }

    let saved = load_saved_state()?;
    ec.write_reg(POLICY_ADDR, saved.original)?;
    std::fs::remove_file(HOLD_STATE_PATH)?;
    log::info!(
        "Restored N161A charge policy exactly: 0x{:02X} -> 0x{:02X}",
        current,
        saved.original
    );
    Ok(())
}
