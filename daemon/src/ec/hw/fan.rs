use super::EcDevice;
use anyhow::{Result, bail};
use ipc::{FanIndex, FanMode};

/// Apply a fan mode to a specific fan index.
/// When `fan` is `Both`, the same mode is applied to both CPU and GPU fans
/// atomically (single lock cycle).
pub fn apply_fan_mode(ec: &EcDevice, fan: &FanIndex, mode: &FanMode) -> Result<()> {
    let (policy, duty) = match mode {
        FanMode::Auto => (0x00, 0),
        FanMode::Full => (0x40, 150),
        FanMode::Custom(d) => {
            if *d > 220 {
                bail!("Requested fan duty cycle ({}) exceeds safe limit (220).", d);
            }
            (0x40, *d)
        }
    };

    match fan {
        FanIndex::Both => ec.with_batch(|b| {
            b.write_ram(b.offsets.ram_thermal_policy_cpu, policy)?;
            b.write_ram(b.offsets.ram_thermal_policy_gpu, policy)?;
            b.write_ram(FanIndex::Cpu as u16, duty)?;
            b.write_ram(FanIndex::Gpu as u16, duty)
        }),
        FanIndex::Cpu | FanIndex::Gpu => {
            let thermal_policy_override: u16 = match fan {
                FanIndex::Cpu => ec.offsets.ram_thermal_policy_cpu,
                FanIndex::Gpu => ec.offsets.ram_thermal_policy_gpu,
                FanIndex::Both => unreachable!(),
            };
            ec.with_batch(|b| {
                b.write_ram(thermal_policy_override, policy)?;
                b.write_ram(*fan as u16, duty)
            })
        }
    }
}
