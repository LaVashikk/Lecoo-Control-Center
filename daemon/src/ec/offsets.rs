/// Stores all hardware-specific register addresses.
/// This allows the daemon to adapt to different motherboard revisions (e.g., Emdoor N155A vs N155D)
/// without hardcoding values directly in the hardware control modules.
#[derive(Debug, Clone, Copy)]
pub struct EcOffsets {
    // System Info
    pub reg_chip_id1: u16,
    pub reg_chip_id2: u16,
    pub reg_chip_ver: u16,

    // ==========================================
    // HRAM Offsets
    // ==========================================

    // Temperatures
    pub ram_temp_cpu: u16,
    pub ram_temp_sys: u16,

    // Power & Performance
    pub ram_power_profile: u16,

    // Fan RPM Monitoring
    pub ram_fan_cpu_msb: u16,
    pub ram_fan_cpu_lsb: u16,
    pub ram_fan_gpu_msb: u16,
    pub ram_fan_gpu_lsb: u16,

    // Fan Control (Thermal Policy Overrides)
    pub ram_thermal_policy_cpu: u16,
    pub ram_thermal_policy_gpu: u16,

    // Battery & FlexiCharger
    pub ram_bat_rsoc: u16,
    pub ram_bat_limit_min: u16,
    pub ram_bat_limit_max: u16,

    /// LED state override (0x00 = Auto, 0x01 = Custom/Bypass)
    pub ram_led_bypass: u16,

    // Timer that resets manual control back to EC (0x04 to disable/extend)
    pub ram_kbd_bypass_timeout: u16,

    // ==========================================
    // Absolute Registers
    // ==========================================

    // Keyboard Backlight
    pub reg_kbd_backlight: u16,
    pub reg_kbd_custom_val: u16,
    pub kbd_backlight_pwm: bool,

    // == LED Controller (Port A0 / PWM Engine) ==

    /// Port A0 Control (Switches pin to manual PWM mode)
    pub reg_gpdra: u16,
    /// Switches pin A0 to manual PWM mode
    pub reg_gpio_a0_mux: u16,
    /// Port pin A4 to manual PWM mode
    pub reg_gpio_a4_mux: u16,
    /// PWM clock prescaler (0x00 = max frequency)
    pub reg_pwm_prescaler: u16,
    /// PWM resolution / cycle time (0xFF = 256 steps)
    pub reg_pwm_cycle: u16,
    /// Main brightness level (Duty Cycle)
    pub reg_pwm_duty: u16,
    /// PWM clock control
    pub reg_pwm_clock_ctrl: u16,

    /// LED Hardware Breathing Animation
    pub reg_led_breath_en: u16,
    /// LED Hardware Breathing Animation Step
    pub reg_led_breath_step: u16,
    /// LED Hardware Breathing Animation Delay
    pub reg_led_breath_delay: u16,

    // ==========================================
    // Bitmasks
    // ==========================================

    // Battery Indicator LEDs
    pub mask_orange_led: u8,
    pub mask_white_led: u8,
}

impl EcOffsets {
    /// Default configuration for standard Emdoor IT5570/IT5571 boards (e.g., Lecoo Pro 14 N155A)
    pub const DEFAULT_N155A: Self = Self {
        reg_chip_id1: 0x2000,
        reg_chip_id2: 0x2001,
        reg_chip_ver: 0x2002,

        // Temperatures
        ram_temp_cpu: 0x70,
        ram_temp_sys: 0x62,
        // Fans
        ram_fan_cpu_msb: 0x76,
        ram_fan_cpu_lsb: 0x77,
        ram_fan_gpu_msb: 0x79,
        ram_fan_gpu_lsb: 0x7A,
        ram_thermal_policy_cpu: 0x4F,
        ram_thermal_policy_gpu: 0x4E,
        // Power
        ram_power_profile: 0xB1,

        // Battery
        ram_bat_rsoc: 0x93,
        ram_bat_limit_min: 0xBC,
        ram_bat_limit_max: 0xBB,

        // Bypasses
        ram_led_bypass: 0x55,
        ram_kbd_bypass_timeout: 0xA6,

        // Absolute Regs (Keyboard)
        reg_kbd_backlight: 0x0F05,
        reg_kbd_custom_val: 0x1806,
        kbd_backlight_pwm: false,

        // Absolute Regs (LED)
        reg_gpdra: 0x1601,
        reg_gpio_a0_mux: 0x1610,
        reg_gpio_a4_mux: 0x1614,
        reg_pwm_prescaler: 0x1800,
        reg_pwm_cycle: 0x1801,
        reg_pwm_duty: 0x1802,
        reg_pwm_clock_ctrl: 0x1823, // ZTIER
        reg_led_breath_en: 0x1850,
        reg_led_breath_step: 0x1851,
        reg_led_breath_delay: 0x1852,

        // Masks
        mask_orange_led: 0x02,
        mask_white_led: 0x04,
    };

    pub const DEFAULT_N155D: Self = Self {
        ram_led_bypass: 0x4F, // or 0x50, 0x51, 0x54
        // The rest of the offsets remain the same as the default configuration
        ..Self::DEFAULT_N155A
    };

    pub const DEFAULT_N161A: Self = Self {
        // N161A keyboard backlight uses a direct PWM duty register:
        // 0x00 = Off, 0x4C = Low, 0x99 = Medium, 0xFF = High.
        reg_kbd_backlight: 0x1803,
        reg_kbd_custom_val: 0x1803,
        kbd_backlight_pwm: true,
        ..Self::DEFAULT_N155A
    };
}
