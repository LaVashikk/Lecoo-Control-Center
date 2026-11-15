use std::hash::{DefaultHasher, Hash, Hasher};

use crate::ec_types::*;
use serde::{Deserialize, Serialize};

/// What `overheat on` means. Telemetry tops out at 103° C on N155A, so this leaves some margin
pub const OVERHEAT_DEFAULT_C: u8 = 95;

/// Current configuration settings of the system
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CurrentSettings {
    pub telemetry_enabled:   bool,
    #[serde(with = "crate::hex_u64")]
    pub telemetry_client_id: u64,
    pub keyboard_backlight:  KeyboardBacklightLevel,
    pub led_mode:            PowerLedMode,
    pub led_on_sleep:        PowerLedMode,
    /// CPU temperature that turns on the LED alarm. None = feature off
    pub overheat_alert_c:    Option<u8>,
    pub power_profile:       PowerProfile,
    pub charge:              ChargeIntent,
    pub fan_mode_cpu:        FanMode,
    pub fan_mode_gpu:        FanMode,
}

impl Default for CurrentSettings {
    fn default() -> Self {
        let mut hasher = DefaultHasher::new();
        std::time::SystemTime::now().hash(&mut hasher);
        let client_id = hasher.finish();

        Self {
            telemetry_enabled:   true,
            telemetry_client_id: client_id,
            keyboard_backlight:  KeyboardBacklightLevel::Medium,
            led_mode:            PowerLedMode::Auto,
            led_on_sleep:        PowerLedMode::Animation(BreathConfig::SLEEP),
            overheat_alert_c:    None,
            power_profile:       PowerProfile::Default,
            charge:              ChargeIntent::Full,
            fan_mode_cpu:        FanMode::Auto,
            fan_mode_gpu:        FanMode::Auto,
        }
    }
}
