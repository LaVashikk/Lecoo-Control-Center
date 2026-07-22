use std::io::{self, Write};

use anyhow::Context;
use clap::{Parser, Subcommand, ValueEnum};
use ipc::{
    ChargeLimit, DaemonCommand, DaemonResponse, FanIndex, FanMode, IpcClient, IpcRequest,
    IpcResponse, KeyboardBacklightLevel, PowerLedMode, PowerProfile,
};

// Initialize i18n
rust_i18n::i18n!("locales", fallback = "en");
use rust_i18n::t;

// Macro to simplify localization in clap attributes
macro_rules! loc {
    ($key:expr) => {
        t!($key).to_string()
    };
}

#[derive(Parser)]
#[command(name = "lecoo-ctrl")]
#[command(version, about = loc!("cmd_app_about"), long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    #[command(about = loc!("cmd_info_about"))]
    Info,

    #[command(about = loc!("cmd_temps_about"))]
    Temps,

    #[command(about = loc!("cmd_fans_about"))]
    Fans,

    #[command(about = loc!("cmd_monitoring_about"))]
    Monitoring {
        #[arg(help = loc!("arg_monitoring_rate_help"))]
        rate: Option<f32>,
    },

    #[command(about = loc!("cmd_fan_about"))]
    Fan {
        #[arg(value_enum, help = loc!("arg_fan_target_help"))]
        target: CliFanIndex,

        #[arg(value_enum, help = loc!("arg_fan_mode_help"))]
        mode: CliFanMode,

        #[arg(help = loc!("arg_fan_val_help"))]
        val: Option<u8>,
    },

    #[command(about = loc!("cmd_charge_about"))]
    Charge {
        #[arg(
            value_enum,
            help = loc!("arg_charge_limit_help"),
            long_help = loc!("arg_charge_limit_long_help")
        )]
        limit: Option<CliChargeLimit>,
    },

    #[command(about = loc!("cmd_power_about"))]
    Power {
        #[arg(value_enum, help = loc!("arg_power_profile_help"))]
        profile: Option<CliPowerProfile>,
    },

    #[command(about = loc!("cmd_kbd_about"))]
    Kbd {
        #[arg(value_enum, help = loc!("arg_kbd_mode_help"))]
        mode: Option<CliKbdMode>,

        #[arg(help = loc!("arg_kbd_val_help"))]
        val: Option<u8>,
    },

    #[command(about = loc!("cmd_led_about"))]
    Led {
        #[command(subcommand)]
        action: CliLedAction,
    },

    #[command(about = loc!("cmd_daemon_about"))]
    Daemon {
        #[command(subcommand)]
        action: CliDaemonAction,
    },
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum CliPowerProfile {
    #[value(help = loc!("arg_power_profile_silent_help"))]
    Silent,
    #[value(help = loc!("arg_power_profile_default_help"))]
    Default,
    #[value(help = loc!("arg_power_profile_perf_help"))]
    Perf,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum CliFanIndex {
    #[value(help = loc!("arg_fan_target_cpu_help"))]
    Cpu,
    #[value(help = loc!("arg_fan_target_gpu_help"))]
    Gpu,
    #[value(help = loc!("arg_fan_target_both_help"))]
    Both,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum CliFanMode {
    #[value(help = loc!("arg_fan_mode_auto_help"))]
    Auto,
    #[value(help = loc!("arg_fan_mode_full_help"))]
    Full,
    #[value(help = loc!("arg_fan_mode_custom_help"))]
    Custom,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum CliKbdMode {
    #[value(help = loc!("arg_kbd_mode_off_help"))]
    Off,
    #[value(help = loc!("arg_kbd_mode_low_help"))]
    Low,
    #[value(help = loc!("arg_kbd_mode_medium_help"))]
    Medium,
    #[value(help = loc!("arg_kbd_mode_high_help"))]
    High,
    #[value(help = loc!("arg_kbd_mode_custom_help"))]
    Custom,
}

#[derive(Subcommand, Clone)]
enum CliLedAction {
    #[command(about = loc!("cmd_led_auto_about"))]
    Auto,
    #[command(about = loc!("cmd_led_custom_about"))]
    Custom {
        #[arg(help = loc!("arg_led_val_help"))]
        val: u8
    },
}

#[derive(Clone, Subcommand)]
enum CliDaemonAction {
    #[command(about = loc!("cmd_daemon_telemetry_about"))]
    Telemetry {
        #[command(subcommand)]
        telemetry_action: CliTelemetryAction,
    },
    #[command(about = loc!("cmd_daemon_settings_about"))]
    Settings {
        #[command(subcommand)]
        settings_action: CliSettingsAction,
    },
    #[command(about = loc!("cmd_daemon_version_about"))]
    Version,
}

#[derive(Subcommand, Clone)]
enum CliTelemetryAction {
    #[command(about = loc!("cmd_telemetry_enable_about"))]
    Enable,
    #[command(about = loc!("cmd_telemetry_disable_about"))]
    Disable,
    #[command(about = loc!("cmd_telemetry_id_about"))]
    Id,
}

#[derive(Subcommand, Clone)]
enum CliSettingsAction {
    #[command(about = loc!("cmd_settings_reset_about"))]
    Reset,
    #[command(about = loc!("cmd_settings_read_about"))]
    Read,
    #[command(about = loc!("cmd_settings_apply_about"))]
    Apply,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum CliChargeLimit {
    #[value(help = loc!("arg_charge_limit_full_help"))]
    Full,
    #[value(help = loc!("arg_charge_limit_high_help"))]
    High,
    #[value(help = loc!("arg_charge_limit_balanced_help"))]
    Balanced,
    #[value(help = loc!("arg_charge_limit_lifespan_help"))]
    Lifespan,
    #[value(help = loc!("arg_charge_limit_desk_help"))]
    Desk,
    #[value(help = loc!("arg_charge_limit_hold_help"))]
    Hold,
    #[value(help = loc!("arg_charge_limit_native_help"))]
    Native,
    #[value(help = loc!("arg_charge_limit_resume_help"))]
    Resume,
}

fn main() -> anyhow::Result<()> {
    // Determine system locale and set it for the application
    if let Some(sys_locale) = sys_locale::get_locale() {
        rust_i18n::set_locale(&sys_locale);
    }

    let cli = Cli::parse();

    // Lazy connection with localized error context
    let mut client = IpcClient::connect()
        .with_context(|| loc!("err_daemon_connection"))?;

    let request = match cli.command {
        Commands::Info => IpcRequest::GetSystemState,
        Commands::Temps => IpcRequest::GetTemperatures,
        Commands::Fans => IpcRequest::GetFansRPM,

        Commands::Monitoring { rate } => {
            let update_rate = (rate.unwrap_or(1.0) * 1000.0) as u64;
            println!("{}", t!("msg_monitoring_start", rate = update_rate));
            loop {
                let IpcResponse::Temp(cpu, system) = client.request(&IpcRequest::GetTemperatures)? else { unreachable!() };
                let IpcResponse::FanRPM(cpu_fan, gpu_fan) = client.request(&IpcRequest::GetFansRPM)? else { unreachable!() };

                print!("\r{}      ", t!("msg_monitoring_loop",
                    cpu = cpu, sys = system, cpu_f = cpu_fan, gpu_f = gpu_fan
                ));
                io::stdout().flush().unwrap();
                std::thread::sleep(std::time::Duration::from_millis(update_rate));
            }
        }

        Commands::Power { profile } => match profile {
            None => IpcRequest::GetPowerProfile,
            Some(p) => {
                let power_p = match p {
                    CliPowerProfile::Silent => PowerProfile::Silent,
                    CliPowerProfile::Default => PowerProfile::Default,
                    CliPowerProfile::Perf => PowerProfile::Performance,
                };
                IpcRequest::SetPowerProfile(power_p)
            }
        }

        Commands::Fan { target, mode, val } => {
            let fan_m = match mode {
                CliFanMode::Auto => FanMode::Auto,
                CliFanMode::Full => FanMode::Full,
                CliFanMode::Custom => FanMode::Custom(val.unwrap_or(0)),
            };
            let fan_idx = match target {
                CliFanIndex::Cpu => FanIndex::Cpu,
                CliFanIndex::Gpu => FanIndex::Gpu,
                CliFanIndex::Both => {
                    let _: IpcResponse = client.request(&IpcRequest::SetFanMode { fan: FanIndex::Cpu, mode: fan_m } )?;
                    FanIndex::Gpu
                },
            };
            IpcRequest::SetFanMode { fan: fan_idx, mode: fan_m }
        }

        Commands::Charge { limit } => match limit {
            Some(cli_limit) => {
                match cli_limit {
                    CliChargeLimit::Hold => IpcRequest::EnableNativeChargeHold,
                    CliChargeLimit::Native => IpcRequest::EnableNativeChargeProtection,
                    CliChargeLimit::Resume => IpcRequest::ResumeNativeCharging,
                    other => {
                        let ipc_limit = match other {
                            CliChargeLimit::Full => ChargeLimit::FullCapacity,
                            CliChargeLimit::High => ChargeLimit::HighCapacity,
                            CliChargeLimit::Balanced => ChargeLimit::Balanced,
                            CliChargeLimit::Lifespan => ChargeLimit::MaximumLifespan,
                            CliChargeLimit::Desk => ChargeLimit::DeskMode,
                            CliChargeLimit::Hold | CliChargeLimit::Native | CliChargeLimit::Resume => unreachable!(),
                        };
                        IpcRequest::SetChargeLimit(ipc_limit)
                    }
                }
            }
            None => IpcRequest::GetNativeChargeHold,
        },

        Commands::Kbd { mode, val } => match mode {
            Some(m) => {
                let lvl = match m {
                    CliKbdMode::Off => KeyboardBacklightLevel::Off,
                    CliKbdMode::Low => KeyboardBacklightLevel::Low,
                    CliKbdMode::Medium => KeyboardBacklightLevel::Medium,
                    CliKbdMode::High => KeyboardBacklightLevel::High,
                    CliKbdMode::Custom => KeyboardBacklightLevel::Custom(val.unwrap_or(255)),
                };
                IpcRequest::SetKeyboardBacklight(lvl)
            }
            None => IpcRequest::GetKeyboardBacklight,
        },

        Commands::Led { action } => {
            let led_m = match action {
                CliLedAction::Auto => PowerLedMode::Auto,
                CliLedAction::Custom { val } => PowerLedMode::Custom(val),
            };
            IpcRequest::SetLedMode(led_m)
        }

        Commands::Daemon { action } => match action {
            CliDaemonAction::Telemetry { telemetry_action } => match telemetry_action {
                CliTelemetryAction::Enable => IpcRequest::DaemonCommand(DaemonCommand::ActivateTelemetry(true)),
                CliTelemetryAction::Disable => IpcRequest::DaemonCommand(DaemonCommand::ActivateTelemetry(false)),
                CliTelemetryAction::Id => IpcRequest::DaemonCommand(DaemonCommand::GetTelemetryId),
            },
            CliDaemonAction::Settings { settings_action } => match settings_action {
                CliSettingsAction::Reset => IpcRequest::DaemonCommand(DaemonCommand::RestoreDefaults),
                CliSettingsAction::Read => IpcRequest::DaemonCommand(DaemonCommand::GetSettings),
                CliSettingsAction::Apply => IpcRequest::DaemonCommand(DaemonCommand::ApplySettings),
            },
            CliDaemonAction::Version => {
                println!("{}.{}", client.daemon_version.0, client.daemon_version.1);
                std::process::exit(0);
            },
        },
    };

    let res: IpcResponse = client.request(&request)?;

    match res {
        IpcResponse::Success => println!("{}", t!("msg_success")),

        IpcResponse::SystemInfo(chip, rev, offset, ver) => {
            println!("{}", t!("resp_sys_info", chip = chip, rev = rev, offset = offset : {:04X}, ver = ver));
        }

        IpcResponse::FanRPM(cpu, gpu) => {
            println!("{}", t!("resp_fans_rpm", cpu = cpu, gpu = gpu));
        }

        IpcResponse::Temp(cpu, sys) => {
            println!("{}", t!("resp_temps", cpu = cpu, sys = sys));
        }

        IpcResponse::KeyboardBacklight(lvl) => {
            println!("{}", t!("resp_kbd_backlight", lvl = lvl));
        }

        IpcResponse::ChargeLimit(min, max, cur) => {
            println!("{}", t!("resp_charge_title"));
            if min == 0 && max == 0 {
                println!("{}", t!("resp_charge_full"));
            } else {
                println!("{}", t!("resp_charge_range", min = min, max = max));
            }
            println!("{}", t!("resp_charge_current", cur = cur));
        }

        IpcResponse::NativeChargeHold(status) => {
            println!("{}", t!("resp_native_charge_title"));
            match status {
                ipc::NativeChargeHoldStatus::Unsupported => {
                    println!("{}", t!("resp_native_charge_unsupported"));
                }
                ipc::NativeChargeHoldStatus::Normal { rsoc } => {
                    println!("{}", t!("resp_native_charge_normal"));
                    println!("{}", t!("resp_charge_current", cur = rsoc));
                }
                ipc::NativeChargeHoldStatus::Holding { rsoc } => {
                    println!("{}", t!("resp_native_charge_holding"));
                    println!("{}", t!("resp_charge_current", cur = rsoc));
                }
                ipc::NativeChargeHoldStatus::NativeProtection { rsoc } => {
                    println!("{}", t!("resp_native_charge_protection"));
                    println!("{}", t!("resp_charge_current", cur = rsoc));
                }
            }
        }

        IpcResponse::PowerLimit(prof) => {
            println!("{}", t!("resp_power_title"));
            println!("{}", t!("resp_power_current", prof = prof));
        }

        IpcResponse::TelemetryDisabledInfo => {
            println!("{}", t!("resp_telemetry_disabled"));
        }

        IpcResponse::Error(msg) => {
            eprintln!("{}", t!("msg_error", msg = msg));
            std::process::exit(1);
        }

        IpcResponse::DaemonResponse(dr) => match dr {
            DaemonResponse::Settings(s) => println!("{:#?}", s),
            DaemonResponse::TelemetryId(id) => {
                println!("{}", t!("resp_telemetry_id", id = id : {:016X}));
            },
        },
    }

    Ok(())
}
