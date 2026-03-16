use anyhow::{Result, bail};
use ipc::{FanIndex, FanMode};
use super::EcDevice;

pub fn apply_fan_mode(ec: &EcDevice, fan: &FanIndex, mode: &FanMode) -> Result<()> {
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

    Ok(())
}
