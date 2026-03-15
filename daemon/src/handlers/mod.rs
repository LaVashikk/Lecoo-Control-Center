use ipc::{FanIndex, FanMode, IpcResponse, KeyboardBacklightLevel, PowerProfile};
use anyhow::{Result, bail};

use crate::EcDevice;

mod getters;
mod power_profile;
mod kbd_backlight;
mod led;
mod flexicharger;

pub use power_profile::{set_power_profile, get_power_profile};
pub use getters::{RAM_TEMP_CPU, get_fans_rpm, get_system_state, get_temperatures};
pub use kbd_backlight::{get_keyboard_backlight, set_keyboard_backlight};
pub use led::set_led_mode;
pub use flexicharger::{set_charge_limit, get_charge_limit};

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
