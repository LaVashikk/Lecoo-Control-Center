use std::sync::atomic::{AtomicBool, Ordering};

use ipc::{HardwareAnimation, PowerLedMode};
use anyhow::Result;
use super::EcDevice;

/// EC state override (0x00 = Auto, 0x01 = Custom/Bypass)
const RAM_LED_BYPASS: u16 = 0x55;

/// Port A0 Control (Switches pin to manual PWM mode)
const REG_GPIO_A0_MUX: u16 = 0x1610;

/// PWM clock prescaler (0x00 = max frequency)
const REG_PWM_PRESCALER: u16 = 0x1800;
/// PWM resolution / cycle time (0xFF = 256 steps)
const REG_PWM_CYCLE: u16 = 0x1801;
/// Main brightness level (Duty Cycle)
const REG_PWM_DUTY: u16 = 0x1802;

/// Hardware breathing toggle/enable
const REG_LED_BREATH_EN: u16 = 0x1850;
/// LCR1: Max Brightness, Step Up & Step Down timings
const REG_LED_BREATH_STEP: u16 = 0x1851;
/// LCR2: Delays at Max and Min brightness levels
const REG_LED_BREATH_DELAY: u16 = 0x1852;

// ======================

static IS_LED_ALREADY_CUSTOM: AtomicBool = AtomicBool::new(false);

pub fn reset_led_anim_engine(ec: &EcDevice) -> Result<()> {
    ec.write_reg(REG_LED_BREATH_EN, 0x00)?;     // Disable hardware breathing dimmer
    ec.write_reg(REG_PWM_PRESCALER, 0x00)?;     // Reset prescaler (maximum PWM frequency)
    ec.write_reg(REG_PWM_CYCLE, 0xFF)?;         // Reset Cycle Time (standard 256 steps)
    Ok(())
}


pub fn apply_led_mode(ec: &EcDevice, mode: &PowerLedMode) -> Result<()> {
    match mode {
        PowerLedMode::Auto => {
            ec.write_ram(RAM_LED_BYPASS, 0x00)?;
            reset_led_anim_engine(ec)?;
            IS_LED_ALREADY_CUSTOM.store(false, Ordering::Relaxed);
        }

        // Set LED to custom brightness value
        PowerLedMode::Custom(brightness) => {
            if !IS_LED_ALREADY_CUSTOM.load(Ordering::Relaxed) {
                ec.write_ram(RAM_LED_BYPASS, 0x01)?;            // LED-controller bypass mode
                ec.write_reg(REG_GPIO_A0_MUX, 0x00)?;           // Pin multiplexer in manual mode
                IS_LED_ALREADY_CUSTOM.store(true, Ordering::Relaxed);
            }
            reset_led_anim_engine(ec)?;                 // TODO HERE! call only if it hasn't been called already
            ec.write_reg(REG_PWM_DUTY, *brightness)?;           // PWM Duty Cycle Register (brightness)
        }

        // Set LED to blinking animation
        PowerLedMode::Animation(HardwareAnimation::Blinking(config)) => {
            if !IS_LED_ALREADY_CUSTOM.load(Ordering::Relaxed) {
                ec.write_ram(RAM_LED_BYPASS, 0x01)?;
                ec.write_reg(REG_GPIO_A0_MUX, 0x00)?;
                IS_LED_ALREADY_CUSTOM.store(true, Ordering::Relaxed);
            }
            reset_led_anim_engine(ec)?;
            ec.write_reg(REG_LED_BREATH_EN, 0x00)?;

            // Setting up hard blinking using frequency dividers
            ec.write_reg(REG_PWM_PRESCALER, config.prescaler)?;
            ec.write_reg(REG_PWM_CYCLE, config.cycle_time)?;
            ec.write_reg(REG_PWM_DUTY, config.duty)?;
        }

        // Set LED to breathing animation
        PowerLedMode::Animation(HardwareAnimation::Breathing(config)) => {
            if !IS_LED_ALREADY_CUSTOM.load(Ordering::Relaxed) {
                ec.write_ram(RAM_LED_BYPASS, 0x01)?;
                ec.write_reg(REG_GPIO_A0_MUX, 0x00)?;
                IS_LED_ALREADY_CUSTOM.store(true, Ordering::Relaxed);
            }
            reset_led_anim_engine(ec)?;

            // Returning the base PWM frequency to normal for a smooth dimmer
            ec.write_reg(REG_PWM_PRESCALER, 0x00)?;
            ec.write_reg(REG_PWM_CYCLE, 0xFF)?;

            // Assemble byte for the PWM0LCR1 register
            // Bits [5:4] - Max Brightness | Bits [3:2] - Step Down | Bits [1:0] - Step Up
            let lcr1_val = ((config.max_brightness as u8) << 4)
                            | ((config.step_down as u8) << 2)
                            | (config.step_up as u8);
            ec.write_reg(REG_LED_BREATH_STEP, lcr1_val)?;

            // Assemble byte for the PWM0LCR2 register
            // Bits [6:4] - Delay at Max | Bits [2:0] - Delay at Min
            let lcr2_val = ((config.delay_at_max as u8) << 4)
                            | (config.delay_at_min as u8);
            ec.write_reg(REG_LED_BREATH_DELAY, lcr2_val)?;

            // Aaaand launching the hardware breathing engine!
            ec.write_reg(REG_LED_BREATH_EN, 0x01)?;
        }
    }

    Ok(())
}

// TODO: blinking: keyboard also start blinking!!! its a bug
