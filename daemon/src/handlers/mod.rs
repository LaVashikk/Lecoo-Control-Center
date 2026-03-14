use ipc::{FanIndex, FanMode, IpcResponse, KeyboardBacklightLevel, PowerProfile};
use anyhow::{Result, bail};

use crate::EcDevice;

mod getters;
mod led;
mod flexicharger;

pub use getters::{get_fans_rpm, get_system_state, get_temperatures};
pub use led::set_led_mode;
pub use flexicharger::{set_charge_limit, get_charge_limit};

pub fn set_power_profile(ec: &EcDevice, profile: &PowerProfile) -> Result<IpcResponse> {
    ec.write_ram(0xB1, *profile as u8)?; // todo!
    Ok(IpcResponse::Success)
}

pub fn set_fan_mode(ec: &EcDevice, fan: &FanIndex, mode: &FanMode) -> Result<IpcResponse> {
    let thermal_policy_override: u16 = match fan { // TODO: it's ram, change adress.
        FanIndex::Cpu => 0x4F,
        FanIndex::Gpu => 0x4E,
    };

    match mode {
        FanMode::Auto => {
            ec.write_ram(thermal_policy_override, 0x00)?;
            ec.write_ram(*fan as u16, 0)?;
        }
        FanMode::Full => {
            ec.write_ram(thermal_policy_override, 0x40)?;
            ec.write_ram(*fan as u16, 150)?;

        }
        FanMode::Custom(duty) => {
            if *duty > 220 {
                bail!("Duty cycle too high, it's dangerous!");
            }

            ec.write_ram(thermal_policy_override, 0x40)?;
            ec.write_ram(*fan as u16, *duty)?;
        }
    };

    Ok(IpcResponse::Success)
}

pub fn set_keyboard_backlight(ec: &EcDevice, level: &KeyboardBacklightLevel) -> Result<IpcResponse> {
    ec.write_reg(0xCF05, *level as u8)?;
    Ok(IpcResponse::Success)
}
