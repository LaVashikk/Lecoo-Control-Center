use ipc::{IpcResponse, KeyboardBacklightLevel};
use anyhow::Result;
use crate::EcDevice;

const KEYBOARD_BACKLIGHT_REG: u16 = 0xCF05;

fn read_keyboard_backlight(ec: &EcDevice) -> Result<u8> {
    ec.read_reg(KEYBOARD_BACKLIGHT_REG)
}

pub fn get_keyboard_backlight(ec: &EcDevice) -> Result<IpcResponse> {
    let level = read_keyboard_backlight(ec)?;
    Ok(IpcResponse::KeyboardBacklight(level))
}

pub fn set_keyboard_backlight(ec: &EcDevice, level: &KeyboardBacklightLevel) -> Result<IpcResponse> {
    ec.write_reg(KEYBOARD_BACKLIGHT_REG, *level as u8)?;
    Ok(IpcResponse::Success)
}
