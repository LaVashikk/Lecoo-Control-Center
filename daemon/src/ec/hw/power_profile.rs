use super::EcDevice;
use anyhow::{Result, bail};
use ipc::{IpcResponse, PowerProfile};

pub fn apply_power_profile(ec: &EcDevice, profile: &PowerProfile) -> Result<IpcResponse> {
    ec.write_ram(ec.offsets.ram_power_profile, *profile as u8)?;
    Ok(IpcResponse::Success)
}

pub fn read_power_profile(ec: &EcDevice) -> Result<PowerProfile> {
    let profile = ec.read_ram(ec.offsets.ram_power_profile)?;
    Ok(match profile {
        1 => PowerProfile::Silent,
        2 => PowerProfile::Default,
        3 => PowerProfile::Performance,
        _ => bail!("Unknown power profile: {}", profile),
    })
}
