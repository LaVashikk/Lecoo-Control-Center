use super::*;
use lecoo_types::caps::{Capabilities, ChargeCaps, FanCaps, KbdCaps, LedCaps};

impl BoardProfile {
    pub fn caps(&self, daemon_version: &str) -> Capabilities {
        #[rustfmt::skip]
        let kbd = match self.kbd {
            KbdOps::None           => KbdCaps::default(),
            KbdOps::PwmDuty { .. } => KbdCaps { on_off: true, levels: true, custom: true },
            KbdOps::Levels { .. }  => KbdCaps { on_off: true, levels: true, custom: true },
        };

        #[rustfmt::skip]
        let led = match self.led {
            LedOps::None             => LedCaps::default(),
            LedOps::Pwm { .. }       => LedCaps { on_off: true, brightness: true, animation: false },
            LedOps::PwmBreath { .. } => LedCaps { on_off: true, brightness: true, animation: true },
        };

        let charge = match self.charge {
            ChargeOps::None => ChargeCaps::default(),
            ChargeOps::FlexiCharger { bounds, freeze_verified, .. } => ChargeCaps {
                supported:       true,
                custom_range:    Some(bounds),
                preserve_fixed:  None,
                freeze_soc:      if freeze_verified { Some(bounds) } else { None },
                survives_daemon: false,
                presets:         self.preset_list(),
            },
            ChargeOps::PolicyByte { fw_thresholds, freeze_soc, .. } => ChargeCaps {
                supported:       true,
                custom_range:    None,
                preserve_fixed:  Some(fw_thresholds),
                freeze_soc:      Some(freeze_soc),
                survives_daemon: true,
                presets:         self.preset_list(),
            },
        };

        Capabilities {
            board: self.id.to_string(),
            daemon_version: daemon_version.to_string(),
            fans: self.fans.iter().map(|f| FanCaps { index: f.index, duty_max: f.duty_max }).collect(),
            sensors: self.sensors.iter().map(|s| s.role).collect(),
            power_profiles: self.power.map(|p| p.map.iter().map(|(_, pr)| *pr).collect()).unwrap_or_default(),
            kbd,
            led,
            battery_leds: self.battery_leds.is_some(),
            battery_info: self.battery_gauge.is_some(),
            charge,
        }
    }

    fn preset_list(&self) -> Vec<(String, ChargeIntent)> {
        self.charge_presets
            .iter()
            .filter(|p| self.supports_intent(&p.intent))
            .map(|p| (p.name.to_string(), p.intent))
            .collect()
    }

    /// Static support only. Runtime preconditions live in the charge backend.
    pub fn supports_intent(&self, intent: &ChargeIntent) -> bool {
        use ChargeIntent as I;
        match (self.charge, intent) {
            (ChargeOps::None, _) => false,
            (_, I::Full) => true,
            (ChargeOps::FlexiCharger { bounds, .. }, I::Preserve(Some(r))) => {
                r.min < r.max && r.min >= bounds.0 && r.max <= bounds.1
            }
            (ChargeOps::FlexiCharger { .. }, I::Preserve(None)) => false,
            (ChargeOps::FlexiCharger { freeze_verified, .. }, I::Freeze) => freeze_verified,
            (ChargeOps::PolicyByte { .. }, I::Preserve(Some(_))) => false,
            (ChargeOps::PolicyByte { .. }, I::Preserve(None)) => true,
            (ChargeOps::PolicyByte { .. }, I::Freeze) => true,
        }
    }
}
