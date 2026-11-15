use serde::{Deserialize, Serialize};

/// Represents the LED Ring behavior
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", content = "c")]
pub enum PowerLedMode {
    #[default]
    Auto,
    Custom(u8),
    Animation(BreathConfig),
}

/// Represents breathing animation brightness levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BreathBrightness {
    Max25Percent  = 0b00,
    Max50Percent  = 0b01,
    Max75Percent  = 0b10,
    Max100Percent = 0b11,
}

/// Represents breathing animation step sizes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BreathStep {
    Slow    = 0b00,
    Medium  = 0b01,
    Fast    = 0b10,
    Instant = 0b11,
}

/// Represents breathing animation delay durations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BreathDelay {
    Ms15   = 0x00, // ~15.6 ms
    Ms125  = 0x01, // ~125 ms
    Ms250  = 0x02, // ~250 ms
    Sec0_5 = 0x03, // 0.5 sec
    Sec1   = 0x04, // 1.0 sec
    Sec2   = 0x05, // 2.0 sec
    Sec4   = 0x06, // 4.0 sec
}

/// Represents breathing animation configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BreathConfig {
    pub max_brightness: BreathBrightness,
    pub step_up:        BreathStep,
    pub step_down:      BreathStep,
    pub delay_at_max:   BreathDelay,
    pub delay_at_min:   BreathDelay,
}

impl BreathConfig {
    /// A gentle, smooth breathing effect perfect for normal, idle operation.
    /// Gradually fades in and out with comfortable pauses.
    pub const SMOOTH: Self = Self {
        max_brightness: BreathBrightness::Max75Percent,
        step_up:        BreathStep::Medium,
        step_down:      BreathStep::Medium,
        delay_at_max:   BreathDelay::Ms250,
        delay_at_min:   BreathDelay::Ms250,
    };

    /// A slow, dim pulse suitable for sleep, standby, or night mode.
    /// Uses low brightness and a long pause at the minimum brightness state.
    pub const SLEEP: Self = Self {
        max_brightness: BreathBrightness::Max25Percent,
        step_up:        BreathStep::Slow,
        step_down:      BreathStep::Slow,
        delay_at_max:   BreathDelay::Sec0_5,
        delay_at_min:   BreathDelay::Sec1,
    };

    /// A fast, high-intensity pulse for alerts, errors, or important notifications.
    /// Almost continuous rapid flashing.
    pub const ALERT: Self = Self {
        max_brightness: BreathBrightness::Max75Percent,
        step_up:        BreathStep::Instant,
        step_down:      BreathStep::Instant,
        delay_at_max:   BreathDelay::Ms125,
        delay_at_min:   BreathDelay::Ms125,
    };

    /// Deep, relaxing breaths with long holds, similar to meditation routines.
    /// Slowly brightens, holds, slowly dims, and holds again.
    pub const ZEN: Self = Self {
        max_brightness: BreathBrightness::Max50Percent,
        step_up:        BreathStep::Slow,
        step_down:      BreathStep::Slow,
        delay_at_max:   BreathDelay::Sec1,
        delay_at_min:   BreathDelay::Sec1,
    };

    /// Sharp, instant burst of light followed by a slow, lingering fade out.
    /// Mimics a heartbeat or a sonar ping.
    pub const PING: Self = Self {
        max_brightness: BreathBrightness::Max100Percent,
        step_up:        BreathStep::Instant,
        step_down:      BreathStep::Slow,
        delay_at_max:   BreathDelay::Ms15,
        delay_at_min:   BreathDelay::Sec0_5,
    };

    /// A steady, high-energy throb. Good for a device that is actively processing
    /// or compiling something.
    pub const ENERGETIC: Self = Self {
        max_brightness: BreathBrightness::Max100Percent,
        step_up:        BreathStep::Fast,
        step_down:      BreathStep::Medium,
        delay_at_max:   BreathDelay::Ms125,
        delay_at_min:   BreathDelay::Ms125,
    };

    /// A subtle warning state. Bright enough to be noticed, but with
    /// a sharp fade-in and instant fade-out to create a "glitchy" or urgent feel.
    pub const WARNING: Self = Self {
        max_brightness: BreathBrightness::Max100Percent,
        step_up:        BreathStep::Fast,
        step_down:      BreathStep::Fast,
        delay_at_max:   BreathDelay::Ms15,
        delay_at_min:   BreathDelay::Ms15,
    };

    /// Slowly builds up tension by gradually increasing brightness to 75%,
    /// holds it for a second, and then instantly snaps to black.
    pub const VACUUM: Self = Self {
        max_brightness: BreathBrightness::Max75Percent,
        step_up:        BreathStep::Slow,
        step_down:      BreathStep::Instant, // Sharp drop
        delay_at_max:   BreathDelay::Sec1,
        delay_at_min:   BreathDelay::Ms250,
    };

    /// Very fast, jagged breathing at high brightness with almost no pauses.
    pub const PANIC: Self = Self {
        max_brightness: BreathBrightness::Max100Percent,
        step_up:        BreathStep::Instant,
        step_down:      BreathStep::Instant,
        delay_at_max:   BreathDelay::Ms15, // Minimum hardware delay
        delay_at_min:   BreathDelay::Ms15, // Minimum hardware delay
    };

    /// A fast, low-brightness ping in the dark that holds briefly,
    /// then fades away slowly into a long silence.
    pub const SONAR: Self = Self {
        max_brightness: BreathBrightness::Max25Percent, // Dim
        step_up:        BreathStep::Fast,
        step_down:      BreathStep::Slow,
        delay_at_max:   BreathDelay::Sec0_5,
        delay_at_min:   BreathDelay::Sec2,
    };

    /// A medium brightness glow that snaps on fast, but takes a painfully
    /// long time to fade out, creating an unnatural, lingering effect.
    pub const TOXIC: Self = Self {
        max_brightness: BreathBrightness::Max50Percent,
        step_up:        BreathStep::Fast,
        step_down:      BreathStep::Slow,
        delay_at_max:   BreathDelay::Ms250,
        delay_at_min:   BreathDelay::Sec1,
    };

    /// Names used by CLI/GUI
    pub const PRESETS: &[(&str, Self)] = &[
        ("smooth", Self::SMOOTH),
        ("sleep", Self::SLEEP),
        ("alert", Self::ALERT),
        ("zen", Self::ZEN),
        ("ping", Self::PING),
        ("energetic", Self::ENERGETIC),
        ("warning", Self::WARNING),
        ("vacuum", Self::VACUUM),
        ("panic", Self::PANIC),
        ("sonar", Self::SONAR),
        ("toxic", Self::TOXIC),
    ];

    pub fn by_name(name: &str) -> Option<Self> {
        Self::PRESETS.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, c)| *c)
    }

    /// None for a hand-made config
    pub fn preset_name(&self) -> Option<&'static str> {
        Self::PRESETS.iter().find(|(_, c)| c == self).map(|(n, _)| *n)
    }
}
