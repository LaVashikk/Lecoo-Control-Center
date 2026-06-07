use std::sync::atomic::{AtomicBool, Ordering};

use super::EcDevice;
use anyhow::Result;
use ipc::PowerLedMode;

static IS_LED_ALREADY_CUSTOM: AtomicBool = AtomicBool::new(false);

pub fn reset_led_anim_engine(ec: &EcDevice) -> Result<()> {
    ec.with_batch(|b| {
        b.write_reg(b.offsets.reg_led_breath_en, 0x00)?; // Disable hardware breathing dimmer
        b.write_reg(b.offsets.reg_pwm_prescaler, 0x00)?; // Reset prescaler (maximum PWM frequency)
        b.write_reg(b.offsets.reg_pwm_cycle, 0xFF) // Reset Cycle Time (standard 256 steps)
    })
}
// TODO: improve logic here... one day

pub fn apply_led_mode(ec: &EcDevice, mode: &PowerLedMode) -> Result<()> {
    let offsets = ec.offsets;
    match mode {
        PowerLedMode::Auto => {
            ec.write_ram(offsets.ram_led_bypass, 0x00)?;
            reset_led_anim_engine(ec)?;
            IS_LED_ALREADY_CUSTOM.store(false, Ordering::Relaxed);
        }

        // Set LED to custom brightness value
        PowerLedMode::Custom(brightness) => {
            if !IS_LED_ALREADY_CUSTOM.load(Ordering::Relaxed) {
                ec.write_ram(offsets.ram_led_bypass, 0x01)?; // LED-controller bypass mode
                ec.write_reg(offsets.reg_gpio_a0_mux, 0x00)?; // Pin multiplexer in manual mode
                IS_LED_ALREADY_CUSTOM.store(true, Ordering::Relaxed);
            }
            reset_led_anim_engine(ec)?; // TODO HERE! call only if it hasn't been called already
            ec.write_reg(offsets.reg_pwm_duty, *brightness)?; // PWM Duty Cycle Register (brightness)
        }

        // Set LED to breathing animation
        PowerLedMode::Animation(config) => {
            if !IS_LED_ALREADY_CUSTOM.load(Ordering::Relaxed) {
                ec.write_ram(offsets.ram_led_bypass, 0x01)?;
                ec.write_reg(offsets.reg_gpio_a0_mux, 0x00)?;
                IS_LED_ALREADY_CUSTOM.store(true, Ordering::Relaxed);
            }
            reset_led_anim_engine(ec)?;

            // Returning the base PWM frequency to normal for a smooth dimmer
            ec.write_reg(offsets.reg_pwm_prescaler, 0x00)?;
            ec.write_reg(offsets.reg_pwm_cycle, 0xFF)?;

            // Assemble byte for the PWM0LCR1 register
            // Bits [5:4] - Max Brightness | Bits [3:2] - Step Down | Bits [1:0] - Step Up
            let lcr1_val = ((config.max_brightness as u8) << 4)
                | ((config.step_down as u8) << 2)
                | (config.step_up as u8);
            ec.write_reg(offsets.reg_led_breath_step, lcr1_val)?;

            // Assemble byte for the PWM0LCR2 register
            // Bits [6:4] - Delay at Max | Bits [2:0] - Delay at Min
            let lcr2_val = ((config.delay_at_max as u8) << 4) | (config.delay_at_min as u8);
            ec.write_reg(offsets.reg_led_breath_delay, lcr2_val)?;

            // Aaaand launching the hardware breathing engine!
            ec.write_reg(offsets.reg_led_breath_en, 0x01)?;
        }
    }

    Ok(())
}

pub fn apply_battery_leds(ec: &EcDevice, orange_on: bool, white_on: bool) -> Result<()> {
    let offsets = ec.offsets;
    ec.with_batch(|b| {
        let mut port_a = b.read_reg(offsets.reg_gpdra)?;

        if orange_on {
            port_a &= !offsets.mask_orange_led; // On
        } else {
            port_a |= offsets.mask_orange_led; // Off
        }

        if white_on {
            port_a &= !offsets.mask_white_led; // On
        } else {
            port_a |= offsets.mask_white_led; // Off
        }

        b.write_reg(offsets.reg_gpdra, port_a)
    })
}
