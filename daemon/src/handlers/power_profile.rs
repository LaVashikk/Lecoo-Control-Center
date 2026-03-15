use ipc::{IpcResponse, PowerProfile};
use anyhow::{Result, bail};
use crate::EcDevice;

pub fn set_power_profile(ec: &EcDevice, profile: &PowerProfile) -> Result<IpcResponse> {
    ec.write_ram(0xB1, *profile as u8)?; // todo!
    Ok(IpcResponse::Success)
}

fn read_power_profile(ec: &EcDevice) -> Result<PowerProfile> {
    let profile = ec.read_ram(0xB1)?;
    Ok(match profile {
        1 => PowerProfile::Silent,
        2 => PowerProfile::Default,
        3 => PowerProfile::Performance,
        _ => bail!("Unknown power profile: {}", profile),
    })
}

pub fn get_power_profile(ec: &EcDevice) -> Result<IpcResponse> {
    let profile = read_power_profile(ec)?;
    Ok(IpcResponse::PowerLimit(profile))
}
