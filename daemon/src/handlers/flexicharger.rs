use ipc::{ChargeLimit, IpcResponse};
use anyhow::{Ok, Result};
use crate::EcDevice;

const RAM_BAT_LIMIT_MIN: u16 = 0xBC;
const RAM_BAT_LIMIT_MAX: u16 = 0xBB;

fn read_charge_limit(ec: &EcDevice) -> Result<(u8, u8)> {
    let min = ec.read_ram(RAM_BAT_LIMIT_MIN)? as u8;
    let max = ec.read_ram(RAM_BAT_LIMIT_MAX)? as u8;
    Ok((min, max))
}

pub fn get_charge_limit(ec: &EcDevice) -> Result<IpcResponse> {
    let (min, max) = read_charge_limit(ec)?;
    Ok(IpcResponse::ChargeLimit(min, max))
}

pub fn set_charge_limit(ec: &EcDevice, limit: &ChargeLimit) -> Result<IpcResponse> {
    let (min, max) = match limit {
        ChargeLimit::FullCapacity => (0, 0),
        ChargeLimit::HighCapacity => (90, 95),
        ChargeLimit::Balanced => (70, 80),
        ChargeLimit::MaximumLifespan => (55, 60),
        ChargeLimit::DeskMode => (40, 50),
        // ChargeLimit::Custom(val) => (val.saturating_sub(5), val)
    };

    ec.write_ram(RAM_BAT_LIMIT_MIN, min)?;
    ec.write_ram(RAM_BAT_LIMIT_MAX, max)?;

    Ok(IpcResponse::Success)
}
