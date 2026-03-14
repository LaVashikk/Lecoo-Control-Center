use bincode::{Decode, Encode};

/// Represents the power profiles
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum PowerProfile {
    Silent = 0x01,
    Default = 0x02,
    Performance = 0x03,
}

impl std::fmt::Display for PowerProfile {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let name = match self {
            PowerProfile::Silent => "Silent",
            PowerProfile::Default => "Default",
            PowerProfile::Performance => "Performance",
        };
        write!(f, "{}", name)
    }
}

/// Represents the keyboard backlight brightness levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum KeyboardBacklightLevel {
    Off = 0x00,
    Low = 0x01,
    Medium = 0x02,
    High = 0x03,
}

/// Represents the fan control mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum FanMode {
    Auto,           // Controlled by EC thermal tables
    Full,           // 100% speed override (Turbo)
    Custom(u8),     // Custom PWM duty cycle
}

/// Identifies the specific fan
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum FanIndex {
    Cpu = 0x4B,     // todo: ram-address to const
    Gpu = 0x4D,     // todo: ram-address to const
}

/// Represents battery charge limit profiles (FlexiCharger)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum ChargeLimit {
    FullCapacity,       // 100%
    HighCapacity,       // 95%
    Balanced,           // 80%
    MaximumLifespan,    // 60%
    DeskMode,           // 40%
    // Custom(u8),  // TODO: too dangerous, I think?
}

/// Represents the LED Ring behavior
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum PowerLedMode {
    Auto,
    Custom(u8),
    Animation(HardwareAnimation),
}

/// Represents hardware animations for the LED ring
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum HardwareAnimation {
    Breathing(BreathConfig),
    Blinking(BlinkConfig),
}

/// Represents breathing animation brightness levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum BreathBrightness {
    Max25Percent = 0b00,
    Max50Percent = 0b01,
    Max75Percent = 0b10,
    Max100Percent = 0b11,
}

/// Represents breathing animation step sizes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum BreathStep {
    Slow = 0b00,
    Medium = 0b01,
    Fast = 0b10,
    Instant = 0b11,
}

/// Represents breathing animation delay durations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub enum BreathDelay {
    Ms15 = 0x00,        // ~15.6 ms
    Ms125 = 0x01,       // ~125 ms
    Ms250 = 0x02,       // ~250 ms
    Sec0_5 = 0x03,      // 0.5 sec
    Sec1 = 0x04,        // 1.0 sec
    Sec2 = 0x05,        // 2.0 sec
    Sec4 = 0x06,        // 4.0 sec
}

/// Represents breathing animation configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct BreathConfig {
    pub max_brightness: BreathBrightness,
    pub step_up: BreathStep,
    pub step_down: BreathStep,
    pub delay_at_max: BreathDelay,
    pub delay_at_min: BreathDelay,
}

impl BreathConfig {
    // todo: add some default anims
}

/// Represents blink configuration parameters
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
pub struct BlinkConfig {
    pub prescaler: u8,
    pub cycle_time: u8,
    pub duty: u8,
}

impl BlinkConfig {
    /// 1 blink per second (Slow Alert)
    pub fn one_hz_50_percent() -> Self {
        Self { prescaler: 127, cycle_time: 255, duty: 127 }
    }

    /// 4 flashes per second (Fast strobe)
    pub fn four_hz_strobe() -> Self {
        Self { prescaler: 31, cycle_time: 255, duty: 32 }
    }

    // todo: add some other anims here
}


/// Current configuration settings of the system
#[derive(Debug, Clone, PartialEq, Eq)] // todo
pub struct CurrentSettings {
    /// Current power profile setting
    pub power_profile: PowerProfile,
    /// Current keyboard backlight brightness level
    pub keyboard_backlight: KeyboardBacklightLevel,
    /// Current CPU fan mode setting
    pub fan_mode_cpu: FanMode,
    /// Current GPU fan mode setting
    pub fan_mode_gpu: FanMode,
    /// Current battery charge limit setting
    pub charge_limit: ChargeLimit,
    /// Current power LED mode setting
    pub led_mode: PowerLedMode,
}
