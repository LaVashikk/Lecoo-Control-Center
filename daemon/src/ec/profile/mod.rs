use lecoo_types::{
    caps::SensorRole,
    ec_types::{ChargeIntent, FanIndex, PowerProfile},
};

mod boards;
mod caps;
#[cfg(test)]
mod tests;

pub use boards::{PROFILES, by_id, detect};

/// Address in EC space. The variant decides how it is resolved, which is the
/// only thing preventing an HRAM offset from being used as an absolute address.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Addr {
    Reg(u16),
    Ram(u16),
    /// Absolute, but inside the bank that moves together with the HRAM window.
    Banked(u16),
}

impl Addr {
    pub fn raw(self) -> u16 {
        match self {
            Addr::Reg(x) | Addr::Ram(x) | Addr::Banked(x) => x,
        }
    }
}

/// Discovered at startup. Varies per machine.
#[derive(Debug, Clone, Copy)]
pub struct EcRuntime {
    pub port:        u16,
    pub hram_offset: u16,
    pub chip_id1:    u8,
    pub chip_id2:    u8,
    pub chip_ver:    u8,
}

// ---------- topology ----------

#[derive(Debug, Clone, Copy)]
pub struct FanSpec {
    pub index:         FanIndex,
    pub rpm_msb:       Addr,
    pub rpm_lsb:       Addr,
    pub duty:          Addr,
    pub policy:        Addr,
    pub policy_auto:   u8,
    pub policy_manual: u8,
    pub duty_full:     u8,
    pub duty_max:      u8,
}

#[derive(Debug, Clone, Copy)]
pub struct SensorSpec {
    pub role: SensorRole,
    pub addr: Addr,
}

#[derive(Debug, Clone, Copy)]
pub struct PowerSpec {
    pub reg: Addr,
    pub map: &'static [(u8, PowerProfile)],
}

#[derive(Debug, Clone, Copy)]
pub struct BatterySpec {
    pub rsoc:           Addr,
    /// Needed to verify "charging is active" guards. None => guard is inconclusive.
    pub charge_current: Option<Addr>,
}

#[derive(Debug, Clone, Copy)]
pub struct BatteryGaugeSpec {
    pub cells:       u8,
    pub design_cap:  Addr,
    pub design_volt: Addr,
    pub full_cap:    Addr,
    pub status:      Addr,
    pub current:     Addr,
    pub rem_cap:     Addr,
    pub voltage:     Addr,
    pub cycles:      Addr,
}

// ---------- behaviour slots ----------

#[derive(Debug, Clone, Copy)]
pub struct PwmSpec {
    pub bypass:     Addr,
    pub mux:        Addr,
    pub prescaler:  Addr,
    pub cycle:      Addr,
    pub duty:       Addr,
    pub clock_ctrl: Addr,
}

#[derive(Debug, Clone, Copy)]
pub enum LedOps {
    None,
    Pwm {
        pwm: PwmSpec,
    },
    PwmBreath {
        pwm:          PwmSpec,
        breath_en:    Addr,
        breath_step:  Addr,
        breath_delay: Addr,
    },
}

/// Separate slot from LedOps: if these masks are not indicator LEDs on some
/// board, we would be poking foreign GPIO on every AC plug event.
#[derive(Debug, Clone, Copy)]
pub struct BatteryLedsSpec {
    pub port:        Addr,
    pub orange_mask: u8,
    pub white_mask:  u8,
    pub active_low:  bool,
}

#[derive(Debug, Clone, Copy)]
pub enum KbdOps {
    None,
    Levels {
        reg:            Addr,
        custom_val:     Addr,
        bypass_timeout: Addr,
        mux:            Addr,
    },
    PwmDuty {
        reg: Addr,
    },
}

#[derive(Debug, Clone, Copy)]
pub enum ChargeOps {
    None,
    /// N155-rev: flexible charging range controlled by deamon.
    FlexiCharger {
        min:             Addr,
        max:             Addr,
        bounds:          (u8, u8),
        /// Value written to `max` for unrestricted charging.
        full_max:        u8,
        /// Set once min==max freezing is confirmed on hardware.
        freeze_verified: bool,
    },
    /// N161-rev: firmware state machine driven by bits of a single byte.
    /// bit0 enable, bit1 policy, bit2 latch ("charged, cut the current").
    PolicyByte {
        reg:           Addr,
        fw_thresholds: (u8, u8),
        arm_below:     u8,
        freeze_soc:    (u8, u8),
    },
}

#[derive(Debug, Clone, Copy)]
pub struct PresetSpec {
    pub name:   &'static str,
    pub intent: ChargeIntent,
}

// ---------- static board facts ----------

#[derive(Debug, Clone, Copy)]
pub struct BoardProfile {
    pub id:              &'static str,
    pub dmi:             &'static [&'static str],
    pub hram_candidates: &'static [u16],

    pub fans:          &'static [FanSpec],
    pub sensors:       &'static [SensorSpec],
    pub power:         Option<PowerSpec>,
    pub battery:       Option<BatterySpec>,
    pub battery_gauge: Option<BatteryGaugeSpec>,
    pub battery_leds:  Option<BatteryLedsSpec>,

    pub kbd:            KbdOps,
    pub led:            LedOps,
    pub charge:         ChargeOps,
    /// Board-specific, so a name never silently maps to a different behaviour.
    pub charge_presets: &'static [PresetSpec],
}

impl BoardProfile {
    pub fn fan(&self, idx: FanIndex) -> Option<&'static FanSpec> {
        self.fans.iter().find(|f| f.index == idx)
    }

    pub fn sensor(&self, role: SensorRole) -> Option<&'static SensorSpec> {
        self.sensors.iter().find(|s| s.role == role)
    }

    /// Flat address list. Backs both `--dump-profile` and the table test.
    pub fn addr_map(&self) -> Vec<(String, Addr)> {
        let mut v: Vec<(String, Addr)> = Vec::new();

        for f in self.fans {
            let t = format!("{:?}", f.index);
            v.push((format!("fan.{t}.rpm_msb"), f.rpm_msb));
            v.push((format!("fan.{t}.rpm_lsb"), f.rpm_lsb));
            v.push((format!("fan.{t}.duty"), f.duty));
            v.push((format!("fan.{t}.policy"), f.policy));
        }
        for s in self.sensors {
            v.push((format!("sensor.{:?}", s.role), s.addr));
        }
        if let Some(p) = self.power {
            v.push(("power.reg".into(), p.reg));
        }
        match self.kbd {
            KbdOps::None => {}
            KbdOps::PwmDuty { reg } => v.push(("kbd.reg".into(), reg)),
            KbdOps::Levels { reg, custom_val, bypass_timeout, mux } => {
                v.push(("kbd.reg".into(), reg));
                v.push(("kbd.custom_val".into(), custom_val));
                v.push(("kbd.bypass_timeout".into(), bypass_timeout));
                v.push(("kbd.mux".into(), mux));
            }
        }
        let push_pwm = |v: &mut Vec<(String, Addr)>, s: PwmSpec| {
            v.push(("led.bypass".into(), s.bypass));
            v.push(("led.mux".into(), s.mux));
            v.push(("led.prescaler".into(), s.prescaler));
            v.push(("led.cycle".into(), s.cycle));
            v.push(("led.duty".into(), s.duty));
            v.push(("led.clock_ctrl".into(), s.clock_ctrl));
        };
        match self.led {
            LedOps::None => {}
            LedOps::Pwm { pwm } => push_pwm(&mut v, pwm),
            LedOps::PwmBreath { pwm, breath_en, breath_step, breath_delay } => {
                push_pwm(&mut v, pwm);
                v.push(("led.breath_en".into(), breath_en));
                v.push(("led.breath_step".into(), breath_step));
                v.push(("led.breath_delay".into(), breath_delay));
            }
        }
        if let Some(b) = self.battery {
            v.push(("bat.rsoc".into(), b.rsoc));
            if let Some(a) = b.charge_current {
                v.push(("bat.current".into(), a));
            }
        }
        if let Some(g) = self.battery_gauge {
            v.push(("bat_gauge.design_cap".into(), g.design_cap));
            v.push(("bat_gauge.design_volt".into(), g.design_volt));
            v.push(("bat_gauge.full_cap".into(), g.full_cap));
            v.push(("bat_gauge.status".into(), g.status));
            v.push(("bat_gauge.current".into(), g.current));
            v.push(("bat_gauge.rem_cap".into(), g.rem_cap));
            v.push(("bat_gauge.voltage".into(), g.voltage));
            v.push(("bat_gauge.cycles".into(), g.cycles));
        }
        if let Some(b) = self.battery_leds {
            v.push(("bat_leds.port".into(), b.port));
        }
        match self.charge {
            ChargeOps::None => {}
            ChargeOps::FlexiCharger { min, max, .. } => {
                v.push(("charge.min".into(), min));
                v.push(("charge.max".into(), max));
            }
            ChargeOps::PolicyByte { reg, .. } => v.push(("charge.policy".into(), reg)),
        }
        v
    }
}
