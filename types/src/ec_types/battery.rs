use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct BatteryInfo {
    /// Firmware constant on N155A (5200), not the pack's real design capacity!!
    pub design_mah:    u16,
    pub design_mv:     u16,
    /// Learned by the gauge, drops as the cells wear
    pub full_mah:      u16,
    pub remaining_mah: u16,
    pub voltage_mv:    u16,
    pub cycles:        u16,
    /// Negative while discharging
    pub current_ma:    i32,
    /// Cells in series, from the board profile
    pub cells:         u8,
}

impl BatteryInfo {
    pub fn health_pct(&self) -> Option<f32> {
        (self.design_mah > 0).then(|| self.full_mah as f32 * 100. / self.design_mah as f32)
    }

    /// Gauge's own charge level, not the stepped percent the EC hands to the OS
    pub fn charge_pct(&self) -> Option<f32> {
        (self.full_mah > 0).then(|| self.remaining_mah as f32 * 100. / self.full_mah as f32)
    }

    pub fn full_wh(&self) -> f32 {
        (self.full_mah as f32 * self.design_mv as f32) / 1e6
    }

    pub fn remaining_wh(&self) -> f32 {
        (self.remaining_mah as f32 * self.design_mv as f32) / 1e6
    }

    pub fn power_w(&self) -> f32 {
        (self.current_ma as f32 * self.voltage_mv as f32) / 1e6
    }

    /// Average, the EC does not expose per-cell voltages
    pub fn cell_mv(&self) -> Option<u16> {
        (self.cells > 0).then(|| self.voltage_mv / self.cells as u16)
    }

    /// Minutes until empty while discharging, or until `stop_pct` of full while charging.
    /// Linear, so charging past ~90% (CV phase) reads optimistic
    pub fn eta_min(&self, stop_pct: u8) -> Option<u32> {
        let left_mah = match self.current_ma.signum() {
            -1 => self.remaining_mah as i32,
            1 => self.full_mah as i32 * stop_pct as i32 / 100 - self.remaining_mah as i32,
            _ => return None,
        };
        (left_mah > 0).then(|| (left_mah * 60 / self.current_ma.abs()) as u32)
    }
}
