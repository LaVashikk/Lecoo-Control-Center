use ipc::KeyboardBacklightLevel;
use anyhow::Result;
use super::EcDevice;

const KEYBOARD_BACKLIGHT_REG: u16 = 0x0F05;

pub fn read_keyboard_backlight(ec: &EcDevice) -> Result<u8> {
    ec.read_reg(KEYBOARD_BACKLIGHT_REG)
}

pub fn apply_keyboard_backlight(ec: &EcDevice, level: &KeyboardBacklightLevel) -> Result<()> {
    let mut addr = KEYBOARD_BACKLIGHT_REG;
    if ec.hram_offset == 0xC400 {
        // WORKAROUND for EC base offset 0xC400
        addr += 0xC000
    }

    ec.write_reg(addr, *level as u8)?;
    Ok(())
}
