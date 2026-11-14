use std::hash::{DefaultHasher, Hash, Hasher};

use crate::ec_types::*;
use serde::{Deserialize, Serialize};

/// Current configuration settings of the system
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CurrentSettings {
    pub telemetry_enabled: bool,
    #[serde(with = "crate::hex_u64")]
    pub telemetry_client_id: u64,
    pub keyboard_backlight: KeyboardBacklightLevel,
    pub led_mode: PowerLedMode,
    pub led_on_sleep: PowerLedMode,
    pub power_profile: PowerProfile,
    pub charge: ChargeIntent,
    pub fan_mode_cpu: FanMode,
    pub fan_mode_gpu: FanMode,
}

impl Default for CurrentSettings {
    fn default() -> Self {
        let mut hasher = DefaultHasher::new();
        std::time::SystemTime::now().hash(&mut hasher);
        let client_id = hasher.finish();

        Self {
            telemetry_enabled: true,
            telemetry_client_id: client_id,
            keyboard_backlight: KeyboardBacklightLevel::Medium,
            led_mode: PowerLedMode::Auto,
            led_on_sleep: PowerLedMode::Animation(BreathConfig::SLEEP),
            power_profile: PowerProfile::Default,
            charge: ChargeIntent::Full,
            fan_mode_cpu: FanMode::Auto,
            fan_mode_gpu: FanMode::Auto,
        }
    }
}
