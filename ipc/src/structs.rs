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
    /// A gentle, smooth breathing effect perfect for normal, idle operation.
    /// Gradually fades in and out with comfortable pauses.
    pub fn smooth() -> Self {
        Self {
            max_brightness: BreathBrightness::Max75Percent,
            step_up: BreathStep::Medium,
            step_down: BreathStep::Medium,
            delay_at_max: BreathDelay::Ms250,
            delay_at_min: BreathDelay::Ms250,
        }
    }

    /// A slow, dim pulse suitable for sleep, standby, or night mode.
    /// Uses low brightness and a long pause at the minimum brightness state.
    pub fn sleep() -> Self {
        Self {
            max_brightness: BreathBrightness::Max25Percent,
            step_up: BreathStep::Slow,
            step_down: BreathStep::Slow,
            delay_at_max: BreathDelay::Sec0_5,
            delay_at_min: BreathDelay::Sec1,
        }
    }

    /// A fast, high-intensity pulse for alerts, errors, or important notifications.
    /// Almost continuous rapid flashing.
    pub fn alert() -> Self {
        Self {
            max_brightness: BreathBrightness::Max75Percent,
            step_up: BreathStep::Instant,
            step_down: BreathStep::Instant,
            delay_at_max: BreathDelay::Ms125 ,
            delay_at_min: BreathDelay::Ms125,
        }
    }

    /// Deep, relaxing breaths with long holds, similar to meditation routines.
    /// Slowly brightens, holds, slowly dims, and holds again.
    pub fn zen() -> Self {
        Self {
            max_brightness: BreathBrightness::Max50Percent,
            step_up: BreathStep::Slow,
            step_down: BreathStep::Slow,
            delay_at_max: BreathDelay::Sec1,
            delay_at_min: BreathDelay::Sec1,
        }
    }

    /// Sharp, instant burst of light followed by a slow, lingering fade out.
    /// Mimics a heartbeat or a sonar ping.
    pub fn ping() -> Self {
        Self {
            max_brightness: BreathBrightness::Max100Percent,
            step_up: BreathStep::Instant,
            step_down: BreathStep::Slow,
            delay_at_max: BreathDelay::Ms15,
            delay_at_min: BreathDelay::Sec0_5,
        }
    }

    /// A steady, high-energy throb. Good for a device that is actively processing
    /// or compiling something.
    pub fn energetic() -> Self {
        Self {
            max_brightness: BreathBrightness::Max100Percent,
            step_up: BreathStep::Fast,
            step_down: BreathStep::Medium,
            delay_at_max: BreathDelay::Ms125,
            delay_at_min: BreathDelay::Ms125,
        }
    }

    /// A subtle warning state. Bright enough to be noticed, but with
    /// a sharp fade-in and instant fade-out to create a "glitchy" or urgent feel.
    pub fn warning() -> Self {
        Self {
            max_brightness: BreathBrightness::Max100Percent,
            step_up: BreathStep::Fast,
            step_down: BreathStep::Fast,
            delay_at_max: BreathDelay::Ms15,
            delay_at_min: BreathDelay::Ms15,
        }
    }

    /// Slowly builds up tension by gradually increasing brightness to 75%,
    /// holds it for a second, and then instantly snaps to black.
    pub fn vacuum() -> Self {
        Self {
            max_brightness: BreathBrightness::Max75Percent,
            step_up: BreathStep::Slow,
            step_down: BreathStep::Instant, // Sharp drop
            delay_at_max: BreathDelay::Sec1,
            delay_at_min: BreathDelay::Ms250,
        }
    }

    /// Very fast, jagged breathing at high brightness with almost no pauses.
    pub fn panic() -> Self {
        Self {
            max_brightness: BreathBrightness::Max100Percent,
            step_up: BreathStep::Instant,
            step_down: BreathStep::Instant,
            delay_at_max: BreathDelay::Ms15,  // Minimum hardware delay
            delay_at_min: BreathDelay::Ms15,  // Minimum hardware delay
        }
    }

    /// A fast, low-brightness ping in the dark that holds briefly,
    /// then fades away slowly into a long silence.
    pub fn sonar() -> Self {
        Self {
            max_brightness: BreathBrightness::Max25Percent, // Dim
            step_up: BreathStep::Fast,
            step_down: BreathStep::Slow,
            delay_at_max: BreathDelay::Sec0_5,
            delay_at_min: BreathDelay::Sec2,
        }
    }

    /// A medium brightness glow that snaps on fast, but takes a painfully
    /// long time to fade out, creating an unnatural, lingering effect.
    pub fn toxic() -> Self {
        Self {
            max_brightness: BreathBrightness::Max50Percent,
            step_up: BreathStep::Fast,
            step_down: BreathStep::Slow,
            delay_at_max: BreathDelay::Ms250,
            delay_at_min: BreathDelay::Sec1,
        }
    }
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode)]
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

impl Default for CurrentSettings {
    fn default() -> Self {
        Self {
            power_profile: PowerProfile::Default,
            keyboard_backlight: KeyboardBacklightLevel::Medium,
            fan_mode_cpu: FanMode::Auto,
            fan_mode_gpu: FanMode::Auto,
            charge_limit: ChargeLimit::FullCapacity,
            led_mode: PowerLedMode::Auto,
        }
    }
}
