use ipc::KeyboardBacklightLevel;
use anyhow::Result;
use super::EcDevice;

pub fn read_keyboard_backlight(ec: &EcDevice) -> Result<KeyboardBacklightLevel> {
    if ec.offsets.kbd_backlight_pwm { // todo: improve this
        return match ec.read_reg(ec.offsets.reg_kbd_backlight)? {
            0x00 => Ok(KeyboardBacklightLevel::Off),
            0x4C => Ok(KeyboardBacklightLevel::Low),
            0x99 => Ok(KeyboardBacklightLevel::Medium),
            0xFF => Ok(KeyboardBacklightLevel::High),
            v => Ok(KeyboardBacklightLevel::Custom(v)),
        };
    }

    match ec.read_reg(ec.offsets.reg_kbd_backlight)? {
        0x00 => Ok(KeyboardBacklightLevel::Off),
        0x01 => Ok(KeyboardBacklightLevel::Low),
        0x02 => Ok(KeyboardBacklightLevel::Medium),
        0x03 => Ok(KeyboardBacklightLevel::High),
        0xFF => {
            let custom_value = ec.read_reg(ec.offsets.reg_kbd_custom_val)?;
            Ok(KeyboardBacklightLevel::Custom(custom_value))
        }

        v => Err(anyhow::anyhow!("Invalid keyboard backlight level: {:#04x}", v)),
    }
}

pub fn apply_keyboard_backlight(ec: &EcDevice, level: &KeyboardBacklightLevel) -> Result<()> {
    let addr = ec.offsets.reg_kbd_backlight;

    if ec.offsets.kbd_backlight_pwm { // todo: improve this
        let value = match level {
            KeyboardBacklightLevel::Off => 0x00,
            KeyboardBacklightLevel::Low => 0x4C,
            KeyboardBacklightLevel::Medium => 0x99,
            KeyboardBacklightLevel::High => 0xFF,
            KeyboardBacklightLevel::Custom(v) => *v,
        };
        ec.write_reg(addr, value)?;
        return Ok(());
    }

    match level {
        KeyboardBacklightLevel::Off => ec.write_reg(addr, 0x00)?,
        KeyboardBacklightLevel::Low => ec.write_reg(addr, 0x01)?,
        KeyboardBacklightLevel::Medium => ec.write_reg(addr, 0x02)?,
        KeyboardBacklightLevel::High => ec.write_reg(addr, 0x03)?,
        KeyboardBacklightLevel::Custom(v) => {
            ec.with_batch(|b| {
                // disable kdb timer
                b.write_ram(b.offsets.ram_kbd_bypass_timeout, 0xFF)?;
                // and enable PWM
                b.write_reg(b.offsets.reg_gpio_a4_mux, 0x00)?;

                // set custom value
                b.write_reg(addr, 0xFF)?;
                b.write_reg(b.offsets.reg_kbd_custom_val, *v)
            })?;
        }
    }

    Ok(())
}
