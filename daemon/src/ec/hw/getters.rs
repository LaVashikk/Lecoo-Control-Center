use super::EcDevice;
use anyhow::Result;
use ipc::PowerProfile;

pub fn read_system_info(ec: &EcDevice) -> Result<(u8, u8, u8)> {
    ec.with_batch(|b| {
        let chip_id1 = b.read_reg(b.offsets.reg_chip_id1)?;
        let chip_id2 = b.read_reg(b.offsets.reg_chip_id2)?;
        let chip_ver = b.read_reg(b.offsets.reg_chip_ver)?;
        Ok((chip_id1, chip_id2, chip_ver))
    })
}

pub fn read_fans_rpm(ec: &EcDevice) -> Result<(u16, u16)> {
    ec.with_batch(|b| {
        let cpu_msb = b.read_ram(b.offsets.ram_fan_cpu_msb)? as u16;
        let cpu_lsb = b.read_ram(b.offsets.ram_fan_cpu_lsb)? as u16;
        let cpu_rpm = (cpu_msb << 8) | cpu_lsb;

        let gpu_msb = b.read_ram(b.offsets.ram_fan_gpu_msb)? as u16;
        let gpu_lsb = b.read_ram(b.offsets.ram_fan_gpu_lsb)? as u16;
        let gpu_rpm = (gpu_msb << 8) | gpu_lsb;

        Ok((cpu_rpm, gpu_rpm))
    })
}

pub fn read_temperatures(ec: &EcDevice) -> Result<(u8, u8)> {
    ec.with_batch(|b| {
        let cpu_temp = b.read_ram(b.offsets.ram_temp_cpu)?;
        let sys_temp = b.read_ram(b.offsets.ram_temp_sys)?;
        Ok((cpu_temp, sys_temp))
    })
}

/// Atomically read power profile + temps + fan RPMs in a single lock cycle.
/// Used by the telemetry worker to minimize contention with IPC clients.
pub fn read_status_snapshot(ec: &EcDevice) -> Result<(PowerProfile, u8, u8, u16, u16)> {
    ec.with_batch(|b| {
        let profile_raw = b.read_ram(b.offsets.ram_power_profile)?;
        let cpu_temp = b.read_ram(b.offsets.ram_temp_cpu)?;
        let sys_temp = b.read_ram(b.offsets.ram_temp_sys)?;

        let cpu_msb = b.read_ram(b.offsets.ram_fan_cpu_msb)? as u16;
        let cpu_lsb = b.read_ram(b.offsets.ram_fan_cpu_lsb)? as u16;
        let gpu_msb = b.read_ram(b.offsets.ram_fan_gpu_msb)? as u16;
        let gpu_lsb = b.read_ram(b.offsets.ram_fan_gpu_lsb)? as u16;

        let profile = match profile_raw {
            1 => PowerProfile::Silent,
            2 => PowerProfile::Default,
            3 => PowerProfile::Performance,
            _ => PowerProfile::Default,
        };

        Ok((
            profile,
            cpu_temp,
            sys_temp,
            (cpu_msb << 8) | cpu_lsb,
            (gpu_msb << 8) | gpu_lsb,
        ))
    })
}
