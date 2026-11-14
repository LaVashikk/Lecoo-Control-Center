use crate::ec_types::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct ChargeCaps {
    pub supported: bool,
    /// Bounds for a user-defined range. None = board cannot do arbitrary ranges.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_range: Option<(u8, u8)>,
    /// Firmware-owned (start, stop). Some => Preserve(None) is available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preserve_fixed: Option<(u8, u8)>,
    /// SoC window where Freeze works. None = unsupported or unverified.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub freeze_soc: Option<(u8, u8)>,
    pub survives_daemon: bool,
    /// Already resolved by the daemon for this board, so CLI/GUI never
    /// map a name to an intent themselves and never silently substitute.
    pub presets: Vec<(String, ChargeIntent)>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct FanCaps {
    pub index: FanIndex,
    pub duty_max: u8,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct KbdCaps {
    pub on_off: bool,
    pub levels: bool,
    pub custom: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct LedCaps {
    pub on_off: bool,
    pub brightness: bool,
    pub animation: bool,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SensorRole {
    Cpu,
    Sys,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Capabilities {
    pub board: String,
    pub daemon_version: String,
    pub fans: Vec<FanCaps>,
    pub sensors: Vec<SensorRole>,
    pub power_profiles: Vec<PowerProfile>,
    pub kbd: KbdCaps,
    pub led: LedCaps,
    pub battery_leds: bool,
    pub charge: ChargeCaps,
    pub battery_info: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ChargeStatus {
    pub desired: ChargeIntent,
    pub effective: ChargeIntent,
    pub soc: u8,
    /// Some(reason) => cannot apply right now; will retry on AC/wake events.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thresholds: Option<(u8, u8)>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct UnsupportedInfo {
    pub board: String,
    pub chip: Option<String>,
}
