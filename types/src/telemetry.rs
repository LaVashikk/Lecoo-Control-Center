use crate::caps::Capabilities;
use crate::ec_types::*;
use serde::{Deserialize, Serialize};

/// DMI and OS facts about the machine.
///
/// Shared by `Startup` and `Unsupported` on purpose: adding support for a new
/// board starts from exactly these strings, so an unsupported-board report
/// without them is close to useless.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostInfo {
    pub vendor:      String,
    pub product:     String,
    pub motherboard: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bios:        Option<String>,
    pub cpu:         String,
    pub os:          String,
    pub arch:        String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", content = "c")]
pub enum TelemetryData {
    Startup {
        host:           HostInfo,
        /// Board profile the daemon resolved, e.g. "n155"
        profile:        String,
        /// Profile came from `--profile` instead of DMI detection
        forced_profile: bool,
        firmware:       String,
        hram_offset:    u16,
        /// Feature matrix this board and daemon version resolved to
        caps:           Box<Capabilities>,
        /// Text of a partial EC restore on boot, if it happened
        #[serde(skip_serializing_if = "Option::is_none")]
        restore_error:  Option<String>,
    },

    /// Periodic sample. Carries the applied configuration next to the sensors:
    /// the point is to see which features people actually run, not only temps.
    Status {
        profile:      PowerProfile,
        cpu_temp_c:   u8,
        sys_temp_c:   u8,
        cpu_fan_rpm:  u16,
        gpu_fan_rpm:  u16,
        fan_mode_cpu: FanMode,
        fan_mode_gpu: FanMode,
        kbd:          KeyboardBacklightLevel,
        led:          PowerLedMode,
        charge:       ChargeIntent,
        #[serde(skip_serializing_if = "Option::is_none")]
        soc:          Option<u8>,
        uptime_s:     u64,
    },

    Unsupported {
        host: HostInfo,
        /// Probed EC signature. None = the chip did not answer at all.
        #[serde(skip_serializing_if = "Option::is_none")]
        chip: Option<String>,
    },

    /// A warn or error record forwarded from the daemon.
    ErrorLog {
        level:   String,
        message: String,
        module:  String,
        line:    u32,
        count:   u32,
    },

    Panic {
        message: String,
        file:    String,
        line:    u32,
        #[serde(skip_serializing_if = "Option::is_none")]
        thread:  Option<String>,
    },

    /// Only catches a tag with no content: `#[serde(other)]` maps to a unit
    /// variant, so `{"t":"Future","c":{...}}` still fails to decode
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryPayload {
    #[serde(with = "crate::hex_u64")]
    pub id:      u64,
    /// Random per daemon process. Ties `Status` and `Panic` back to the
    /// `Startup` of the same boot, which is what makes "session ended without
    /// a shutdown" countable server-side.
    #[serde(with = "crate::hex_u64")]
    pub session: u64,
    pub data:    TelemetryData,
}
