//! Periodic EC samples the daemon collects on its metrics tick, served over IPC

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sample {
    pub cpu_c:       u8,
    pub sys_c:       u8,
    pub cpu_fan_rpm: u16,
    pub gpu_fan_rpm: u16,
    /// Daemon uptime when the sample was taken
    pub uptime_s:    u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct Metrics {
    /// None until the first tick
    pub last: Option<Sample>,
    // todo: histograms from gui/slint
}
