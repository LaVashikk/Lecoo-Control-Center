//! Declarative command argument definitions.
//!
//! Single source of truth for both argument parsing (`parser.rs`) and
//! help output (`help.rs`). Adding a new enum value to a command means
//! editing exactly one `define_values!` invocation - the macro generates
//! both the parser match arm and the formatted help line.

use rust_i18n::t;

// --- Shared types ---

/// CLI-only fan target (not in IPC - the daemon uses FanIndex)
#[derive(Debug, Clone)]
pub enum FanTarget {
    Cpu,
    Gpu,
    Both,
}

#[derive(Debug)]
pub struct ParseError {
    pub message: String,
    pub command: Option<String>,
}

// --- Helpers ---

/// Extracts a required positional argument, or returns a localized error.
pub fn require_arg<'a>(
    args: &'a [String],
    index: usize,
    label: &str,
    cmd_ctx: &Option<String>,
) -> Result<&'a str, ParseError> {
    args.get(index).map(String::as_str).ok_or_else(|| ParseError {
        message: t!("err_missing_args", args = format!("  {label}")).into(),
        command: cmd_ctx.clone(),
    })
}

/// Extracts a positional argument and parses it as u8.
pub fn parse_u8_arg(
    args: &[String],
    index: usize,
    label: &str,
    cmd_ctx: &Option<String>,
) -> Result<u8, ParseError> {
    let s = require_arg(args, index, label, cmd_ctx)?;
    s.parse().map_err(|_| ParseError {
        message: t!("err_invalid_value", val = s, arg = label).into(),
        command: cmd_ctx.clone(),
    })
}

/// Extracts a positional argument as `0x`-prefixed hex or plain decimal
pub fn parse_int_arg<T: TryFrom<u32>>(
    args: &[String],
    index: usize,
    label: &str,
    cmd_ctx: &Option<String>,
) -> Result<T, ParseError> {
    let s = require_arg(args, index, label, cmd_ctx)?;
    let n = match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(hex) => u32::from_str_radix(hex, 16).ok(),
        None => s.parse().ok(),
    };

    n.and_then(|n| T::try_from(n).ok()).ok_or_else(|| ParseError {
        message: t!("err_invalid_value", val = s, arg = label).into(),
        command: cmd_ctx.clone(),
    })
}

// --- Macro ---

/// Defines enum value mappings for both CLI parsing and help generation.
///
/// Generates two public functions from one definition:
/// - `$parse_fn(input, arg_label, cmd_ctx) -> Result<T, ParseError>`
/// - `$help_fn()` - prints formatted value lines (without the "Possible values" header)
///
/// `pad` controls left-alignment width for consistent column layout in `--help`.
macro_rules! define_values {
    (
        $parse_fn:ident / $help_fn:ident, pad: $pad:expr, $EnumType:ty {
            $( $primary:literal $( | $alias:literal )* => $variant:expr, $help_key:literal; )+
        }
    ) => {
        pub fn $parse_fn(
            input: &str, arg_label: &str, cmd_ctx: &Option<String>,
        ) -> Result<$EnumType, ParseError> {
            match input.to_lowercase().as_str() {
                $( $primary $( | $alias )* => Ok($variant), )+
                _ => Err(ParseError {
                    message: t!("err_invalid_value", val = input, arg = arg_label).into(),
                    command: cmd_ctx.clone(),
                })
            }
        }

        pub fn $help_fn() {
            $(
                println!("          - {}{:<pad$}{}{}",
                    crate::help::bold(), concat!($primary, ":"),
                    crate::help::reset(), t!($help_key), pad = $pad);
            )+
        }
    };
}

// --- Value definitions ---

use lecoo_types::ec_types::{FanMode, KeyboardBacklightLevel, PowerLedMode, PowerProfile};

define_values! {
    parse_fan_target / help_fan_target, pad: 6, FanTarget {
        "cpu"  => FanTarget::Cpu,  "arg_fan_target_cpu_help";
        "gpu"  => FanTarget::Gpu,  "arg_fan_target_gpu_help";
        "both" => FanTarget::Both, "arg_fan_target_both_help";
    }
}

define_values! {
    parse_fan_mode / help_fan_mode, pad: 8, FanMode {
        "auto"  => FanMode::Auto,  "arg_fan_mode_auto_help";
        "full"  => FanMode::Full,  "arg_fan_mode_full_help";
        "turbo" => FanMode::Turbo, "arg_fan_mode_turbo_help";
    }
}

define_values! {
    parse_power_profile / help_power_profile, pad: 9, PowerProfile {
        "silent"  => PowerProfile::Silent,                   "arg_power_profile_silent_help";
        "default" => PowerProfile::Default,                  "arg_power_profile_default_help";
        "perf" | "performance" => PowerProfile::Performance, "arg_power_profile_perf_help";
    }
}

define_values! {
    parse_kbd_level / help_kbd_level, pad: 8, KeyboardBacklightLevel {
        "off"    => KeyboardBacklightLevel::Off,    "arg_kbd_mode_off_help";
        "low"    => KeyboardBacklightLevel::Low,    "arg_kbd_mode_low_help";
        "medium" => KeyboardBacklightLevel::Medium, "arg_kbd_mode_medium_help";
        "high"   => KeyboardBacklightLevel::High,   "arg_kbd_mode_high_help";
    }
}

define_values! {
    parse_led_mode / help_led_mode, pad: 8, PowerLedMode {
        "auto" => PowerLedMode::Auto, "cmd_led_auto_about";
    }
}
