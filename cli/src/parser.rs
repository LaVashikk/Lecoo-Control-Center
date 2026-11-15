//! Custom CLI Parser using `lexopt`.
//!
//! Why aren't we using `clap`?
//! `clap` requires the entire CLI structure (commands, arguments, possible values)
//! to be statically defined at compile time. However, this application heavily relies
//! on dynamic hardware capabilities retrieved via IPC (e.g., supported charge presets,
//! power profiles). Injecting dynamic states into `clap`'s static tree requires
//! extensive hacks and defeats the purpose of the library.
//!
//! By using `lexopt`, we maintain a lightweight, stream-based parsing approach.
//! It allows us to manually parse arguments based on the daemon's runtime `Capabilities`
//! while keeping the binary size incredibly lean, compile times fast, and providing
//! a completely dynamic `--help` output that actually matches the user's hardware.

use lecoo_types::caps::Capabilities;
use lecoo_types::ec_types::{
    BreathConfig, ChargeIntent, ChargeRange, FanMode, KeyboardBacklightLevel, PowerLedMode, PowerProfile,
};
use lecoo_types::settings::OVERHEAT_DEFAULT_C;
use lexopt::prelude::*;
use rust_i18n::t;

use crate::defs::{
    FanTarget, ParseError, parse_fan_mode, parse_fan_target, parse_int_arg, parse_kbd_level, parse_led_mode,
    parse_power_profile, parse_u8_arg, require_arg,
};

#[derive(Debug, Clone)]
pub enum DaemonSubcommand {
    TelemetryEnable,
    TelemetryDisable,
    TelemetryId,
    SettingsReset,
    SettingsRead,
    SettingsApply,
    /// None turns the alarm off
    Overheat(Option<u8>),
    OverheatStatus,
    Version,
}

#[derive(Debug, Clone)]
pub enum CliCommand {
    Info,
    Temps,
    Fans,
    Monitoring { rate: Option<f32> },
    Power { profile: Option<PowerProfile> },
    Fan { target: FanTarget, mode: FanMode },
    Charge { intent: Option<ChargeIntent> },
    Battery,
    Kbd { level: Option<KeyboardBacklightLevel> },
    Led { mode: PowerLedMode },
    LedSleep { mode: PowerLedMode },
    LedFlash { effect: BreathConfig, duration_ms: u16 },
    Daemon(DaemonSubcommand),
    HwTest,
    Read { addr: u16, count: u16 },
    Write { addr: u16, val: u8 },
    Help { target: Option<String> },
    Version,
}

macro_rules! parse_bail {
    ($cmd:expr, $($arg:tt)*) => {
        return Err(ParseError {
            message: format!($($arg)*),
            command: $cmd.clone(),
        })
    };
}

pub fn parse_args(caps: Option<&Capabilities>) -> Result<CliCommand, ParseError> {
    let mut parser = lexopt::Parser::from_env();

    let mut command_name: Option<String> = None;
    let mut args: Vec<String> = Vec::new();
    let mut min_val: Option<u8> = None;
    let mut max_val: Option<u8> = None;
    let mut rate_val: Option<f32> = None;
    let mut for_val: Option<f32> = None;
    let mut wants_help = false;

    while let Some(arg) = parser
        .next()
        .map_err(|e| ParseError { message: e.to_string(), command: command_name.clone() })?
    {
        match arg {
            Short('h') | Long("help") => {
                wants_help = true;
            }
            Short('v') | Long("version") => return Ok(CliCommand::Version),
            Long("min") => {
                let val_str = parser
                    .value()
                    .map_err(|e| ParseError { message: e.to_string(), command: command_name.clone() })?;
                let v: u8 = val_str.parse().map_err(|_| ParseError {
                    message: "Invalid value for --min".into(),
                    command: command_name.clone(),
                })?;
                min_val = Some(v);
            }
            Long("max") => {
                let val_str = parser
                    .value()
                    .map_err(|e| ParseError { message: e.to_string(), command: command_name.clone() })?;
                let v: u8 = val_str.parse().map_err(|_| ParseError {
                    message: "Invalid value for --max".into(),
                    command: command_name.clone(),
                })?;
                max_val = Some(v);
            }
            Long("rate") => {
                let val_str = parser
                    .value()
                    .map_err(|e| ParseError { message: e.to_string(), command: command_name.clone() })?;
                let v: f32 = val_str.parse().map_err(|_| ParseError {
                    message: "Invalid value for --rate".into(),
                    command: command_name.clone(),
                })?;
                rate_val = Some(v);
            }
            Long("for") => {
                let val_str = parser
                    .value()
                    .map_err(|e| ParseError { message: e.to_string(), command: command_name.clone() })?;
                let v: f32 = val_str.parse().map_err(|_| ParseError {
                    message: "Invalid value for --for".into(),
                    command: command_name.clone(),
                })?;
                for_val = Some(v);
            }
            Value(val) => {
                let s = val.into_string().map_err(|_| ParseError {
                    message: "Invalid string argument".into(),
                    command: command_name.clone(),
                })?;
                if command_name.is_none() {
                    if s.eq_ignore_ascii_case("help") {
                        wants_help = true;
                    } else {
                        command_name = Some(s);
                    }
                } else {
                    args.push(s);
                }
            }
            _ => {
                return Err(ParseError {
                    message: arg.unexpected().to_string(),
                    command: command_name.clone(),
                });
            }
        }
    }

    if wants_help {
        return Ok(CliCommand::Help { target: command_name.or_else(|| args.first().cloned()) });
    }

    let Some(cmd) = command_name else {
        return Ok(CliCommand::Help { target: None });
    };

    let cmd_ctx = Some(cmd.clone());

    match cmd.to_lowercase().as_str() {
        "info" => Ok(CliCommand::Info),
        "temps" => Ok(CliCommand::Temps),
        "fans" => Ok(CliCommand::Fans),
        "monitoring" => {
            let rate = rate_val.or_else(|| args.first().and_then(|s| s.parse().ok()));
            Ok(CliCommand::Monitoring { rate })
        }

        "power" => {
            if let Some(p_str) = args.first() {
                let profile = parse_power_profile(p_str, "[PROFILE]", &cmd_ctx)?;
                Ok(CliCommand::Power { profile: Some(profile) })
            } else {
                Ok(CliCommand::Power { profile: None })
            }
        }

        "fan" => {
            let target_str = require_arg(&args, 0, "<TARGET>\n  <MODE>", &cmd_ctx)?;
            let mode_str = require_arg(&args, 1, "<MODE>", &cmd_ctx)?;

            let target = parse_fan_target(target_str, "<TARGET>", &cmd_ctx)?;

            let mode = if mode_str.eq_ignore_ascii_case("custom") {
                let pwm = parse_u8_arg(&args, 2, "[PWM_VAL]", &cmd_ctx)?;
                FanMode::Custom(pwm)
            } else {
                parse_fan_mode(mode_str, "<MODE>", &cmd_ctx)?
            };

            Ok(CliCommand::Fan { target, mode })
        }

        "charge" => {
            if let (Some(min), Some(max)) = (min_val, max_val) {
                return Ok(CliCommand::Charge {
                    intent: Some(ChargeIntent::Preserve(Some(ChargeRange { min, max }))),
                });
            }

            if let Some(preset_str) = args.first() {
                let preset_name = preset_str.to_lowercase();

                if let Some(c) = caps {
                    if let Some((_, intent)) =
                        c.charge.presets.iter().find(|(name, _)| name.eq_ignore_ascii_case(&preset_name))
                    {
                        return Ok(CliCommand::Charge { intent: Some(*intent) });
                    }
                }

                let intent = match preset_name.as_str() {
                    "full" => ChargeIntent::Full,
                    "high" => ChargeIntent::Preserve(Some(ChargeRange { min: 90, max: 95 })),
                    "balanced" => ChargeIntent::Preserve(Some(ChargeRange { min: 70, max: 80 })),
                    "lifespan" => ChargeIntent::Preserve(Some(ChargeRange { min: 55, max: 60 })),
                    "desk" => ChargeIntent::Preserve(Some(ChargeRange { min: 40, max: 50 })),
                    "hold" | "freeze" => ChargeIntent::Freeze,
                    _ => parse_bail!(
                        cmd_ctx,
                        "{}",
                        t!("err_invalid_value", val = preset_str, arg = "[PRESET]")
                    ),
                };
                Ok(CliCommand::Charge { intent: Some(intent) })
            } else {
                Ok(CliCommand::Charge { intent: None })
            }
        }

        "battery" => Ok(CliCommand::Battery),

        "kbd" => {
            if let Some(mode_str) = args.first() {
                let level = if mode_str.eq_ignore_ascii_case("custom") {
                    let pwm = parse_u8_arg(&args, 1, "[PWM_VAL]", &cmd_ctx)?;
                    KeyboardBacklightLevel::Custom(pwm)
                } else {
                    parse_kbd_level(mode_str, "[MODE]", &cmd_ctx)?
                };
                Ok(CliCommand::Kbd { level: Some(level) })
            } else {
                Ok(CliCommand::Kbd { level: None })
            }
        }

        "led" => {
            // no animation check here: on boards without the breath engine the daemon flashes at full brightness
            if args.first().is_some_and(|a| a.eq_ignore_ascii_case("flash")) {
                let effect = match args.get(1) {
                    Some(name) => parse_preset(name, &cmd_ctx)?,
                    None => BreathConfig::ALERT,
                };
                // `as` saturates, so a silly --for just clamps to 0..65 s
                let duration_ms = (for_val.unwrap_or(1.5) * 1000.0) as u16;
                return Ok(CliCommand::LedFlash { effect, duration_ms });
            }

            let sleep = args.first().is_some_and(|a| a.eq_ignore_ascii_case("sleep"));
            let mode = parse_led_args(&args[sleep as usize..], &cmd_ctx)?;

            // the daemon won't stop us, it would just store a mode it can never apply
            if matches!(mode, PowerLedMode::Animation(_)) && caps.is_some_and(|c| !c.led.animation) {
                parse_bail!(cmd_ctx, "{}", t!("err_led_no_animation"));
            }
            Ok(if sleep { CliCommand::LedSleep { mode } } else { CliCommand::Led { mode } })
        }

        "daemon" => {
            let sub = args.first().ok_or_else(|| ParseError {
                message: t!("err_missing_args", args = "  <SUBCOMMAND>").into(),
                command: cmd_ctx.clone(),
            })?;
            let daemon_sub = match sub.to_lowercase().as_str() {
                "version" => DaemonSubcommand::Version,
                "telemetry" => {
                    let action = args.get(1).ok_or_else(|| ParseError {
                        message: t!("err_missing_args", args = "  <enable|disable|id>").into(),
                        command: cmd_ctx.clone(),
                    })?;
                    match action.to_lowercase().as_str() {
                        "enable" => DaemonSubcommand::TelemetryEnable,
                        "disable" => DaemonSubcommand::TelemetryDisable,
                        "id" => DaemonSubcommand::TelemetryId,
                        _ => parse_bail!(
                            cmd_ctx,
                            "{}",
                            t!("err_invalid_value", val = action, arg = "<enable|disable|id>")
                        ),
                    }
                }
                "overheat" => match args.get(1).map(|a| a.to_lowercase()).as_deref() {
                    None => DaemonSubcommand::OverheatStatus,
                    Some("on") => DaemonSubcommand::Overheat(Some(OVERHEAT_DEFAULT_C)),
                    Some("off") => DaemonSubcommand::Overheat(None),
                    Some(_) => {
                        DaemonSubcommand::Overheat(Some(parse_int_arg(&args, 1, "[on|off|°C]", &cmd_ctx)?))
                    }
                },
                "settings" => {
                    let action = args.get(1).ok_or_else(|| ParseError {
                        message: t!("err_missing_args", args = "  <reset|read|apply>").into(),
                        command: cmd_ctx.clone(),
                    })?;
                    match action.to_lowercase().as_str() {
                        "reset" => DaemonSubcommand::SettingsReset,
                        "read" => DaemonSubcommand::SettingsRead,
                        "apply" => DaemonSubcommand::SettingsApply,
                        _ => parse_bail!(
                            cmd_ctx,
                            "{}",
                            t!("err_invalid_value", val = action, arg = "<reset|read|apply>")
                        ),
                    }
                }
                _ => parse_bail!(cmd_ctx, "{}", t!("err_unrecognized_subcmd", cmd = sub)),
            };
            Ok(CliCommand::Daemon(daemon_sub))
        }

        "hwtest" => Ok(CliCommand::HwTest),

        // Secret dev commands: not in help and not in `known_commands`,
        // so a typo never suggests them either
        "read" => {
            let addr = parse_int_arg(&args, 0, "<ADDR>", &cmd_ctx)?;
            let count = if args.len() > 1 { parse_int_arg(&args, 1, "[COUNT]", &cmd_ctx)? } else { 1 };
            Ok(CliCommand::Read { addr, count })
        }
        "write" => {
            let addr = parse_int_arg(&args, 0, "<ADDR>", &cmd_ctx)?;
            let val = parse_int_arg(&args, 1, "<VAL>", &cmd_ctx)?;
            Ok(CliCommand::Write { addr, val })
        }

        _ => {
            let known_commands = [
                "info",
                "temps",
                "fans",
                "monitoring",
                "fan",
                "charge",
                "battery",
                "power",
                "kbd",
                "led",
                "daemon",
                "hwtest",
                "help",
            ];
            let mut best_match = None;
            let mut best_dist = usize::MAX;
            for k in known_commands {
                let dist = strsim::levenshtein(&cmd, k);
                if dist < best_dist && dist <= 2 {
                    best_dist = dist;
                    best_match = Some(k);
                }
            }

            if let Some(suggestion) = best_match {
                parse_bail!(
                    None,
                    "{}\n\n{}",
                    t!("err_unrecognized_cmd", cmd = cmd),
                    t!("err_did_you_mean", suggestion = suggestion)
                );
            } else {
                parse_bail!(None, "{}", t!("err_unrecognized_cmd", cmd = cmd));
            }
        }
    }
}

/// `<auto | custom PWM_VAL | breath PRESET>`, shared by `led` and `led sleep`
fn parse_led_args(args: &[String], cmd_ctx: &Option<String>) -> Result<PowerLedMode, ParseError> {
    let action = require_arg(args, 0, "<auto|custom|breath>", cmd_ctx)?;
    match action.to_lowercase().as_str() {
        "custom" => Ok(PowerLedMode::Custom(parse_u8_arg(args, 1, "[PWM_VAL]", cmd_ctx)?)),
        "breath" => {
            Ok(PowerLedMode::Animation(parse_preset(require_arg(args, 1, "<PRESET>", cmd_ctx)?, cmd_ctx)?))
        }
        _ => parse_led_mode(action, "<auto|custom|breath>", cmd_ctx),
    }
}

fn parse_preset(name: &str, cmd_ctx: &Option<String>) -> Result<BreathConfig, ParseError> {
    BreathConfig::by_name(name).ok_or_else(|| ParseError {
        message: t!("err_invalid_value", val = name, arg = "<PRESET>").into(),
        command: cmd_ctx.clone(),
    })
}
