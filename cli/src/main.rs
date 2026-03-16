use anyhow::Context;
use clap::{Parser, Subcommand, ValueEnum};
use ipc::{ChargeLimit, FanIndex, FanMode, IpcClient, IpcRequest, IpcResponse, KeyboardBacklightLevel, PowerLedMode, PowerProfile};

#[derive(Parser)]
#[command(name = "lecoo-ctrl")]
#[command(about = "Lecoo Control Center CLI", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Get info about the embedded controller
    Info,

    /// Get system temperatures
    Temps,

    /// Get current fan speeds (RPM)
    Fans,

    /// Control fan settings (e.g., `fan cpu auto`, `fan gpu custom 150`)
    Fan {
        #[arg(value_enum)]
        target: CliFanIndex,

        #[arg(value_enum)]
        mode: CliFanMode,

        val: Option<u8>,
    },

    /// Get or set battery charge limit
    Charge {
        /// Empty to read limit. To set, use: full, high, balanced, lifespan, desk
        #[arg(value_parser = parse_charge_arg)]
        limit: Option<ChargeLimit>,
    },

    /// Set system power profile
    Power {
        #[arg(value_enum)]
        profile: Option<CliPowerProfile>,
    },

    /// Set keyboard backlight level (0-3)
    Kbd {
        #[arg(value_parser = clap::value_parser!(u8).range(0..=3))]
        level: Option<u8>,
    },

    /// Control rear LED ring (e.g., `led auto`, `led custom 255`)
    Led {
        #[command(subcommand)]
        action: CliLedAction,
    },
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum CliPowerProfile {
    Silent,
    Default,
    Perf,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum CliFanIndex {
    Cpu,
    Gpu,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum)]
enum CliFanMode {
    Auto,
    Full,
    Custom,
}

#[derive(Subcommand, Clone)]
enum CliLedAction {
    /// Let EC control the LED automatically
    Auto,
    /// Set manual static brightness (0-255)
    Custom { val: u8 },
    // todo: add others
}

// --- Custom Parsers ---

fn parse_charge_arg(s: &str) -> Result<ChargeLimit, String> {
    match s.to_lowercase().as_str() {
        "full" => Ok(ChargeLimit::FullCapacity),
        "high" => Ok(ChargeLimit::HighCapacity),
        "balanced" => Ok(ChargeLimit::Balanced),
        "lifespan" => Ok(ChargeLimit::MaximumLifespan),
        "desk" => Ok(ChargeLimit::DeskMode),
        _ => Err("Use a preset: full, high, balanced, lifespan, desk".to_string()),
        // other => {
        //     if let Ok(val) = other.parse::<u8>() {
        //         if val <= 100 {
        //             Ok(ChargeLimit::Custom(val))
        //         } else {
        //             Err("Limit must be between 0 and 100".to_string())
        //         }
        //     } else {
        //         Err("Use a percentage (0-100) or preset: full, balanced, lifespan".to_string())
        //     }
        // }
    }
}

// fn parse_hex_or_dec(s: &str) -> Result<u16, std::num::ParseIntError> {
//     if let Some(hex) = s.strip_prefix("0x") {
//         u16::from_str_radix(hex, 16)
//     } else {
//         s.parse::<u16>()
//     }
// }

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let mut client = IpcClient::connect().context("Failed to connect to daemon! Is it running?")?;

    let request = match cli.command {
        Commands::Info => IpcRequest::GetSystemState,
        Commands::Temps => IpcRequest::GetTemperatures,
        Commands::Fans => IpcRequest::GetFansRPM,

        Commands::Power { profile } => match profile {
            None => IpcRequest::GetPowerProfile,
            Some(p) => {
                let power_profile = match p {
                    CliPowerProfile::Silent => PowerProfile::Silent,
                    CliPowerProfile::Default => PowerProfile::Default,
                    CliPowerProfile::Perf => PowerProfile::Performance,
                };
                IpcRequest::SetPowerProfile(power_profile)
            }
        }

        Commands::Fan { target, mode, val } => {
            let fan_idx = match target {
                CliFanIndex::Cpu => FanIndex::Cpu,
                CliFanIndex::Gpu => FanIndex::Gpu,
            };
            let fan_mode = match mode {
                CliFanMode::Auto => FanMode::Auto,
                CliFanMode::Full => FanMode::Full,
                CliFanMode::Custom => FanMode::Custom(val.unwrap_or(0)),
            };
            IpcRequest::SetFanMode { fan: fan_idx, mode: fan_mode }
        }

        Commands::Charge { limit } => {
            match limit {
                Some(l) => IpcRequest::SetChargeLimit(l),
                None => IpcRequest::GetChargeLimit,
            }
        }

        Commands::Kbd { level } => match level {
            Some(l) => {
                let lvl = match l {
                    0 => KeyboardBacklightLevel::Off,
                    1 => KeyboardBacklightLevel::Low,
                    2 => KeyboardBacklightLevel::Medium,
                    _ => KeyboardBacklightLevel::High,
                };
                IpcRequest::SetKeyboardBacklight(lvl)
            }
            None => IpcRequest::GetKeyboardBacklight,
        },

        Commands::Led { action } => {
            let led_m = match action {
                CliLedAction::Auto => PowerLedMode::Auto,
                CliLedAction::Custom { val } => PowerLedMode::Custom(val), // todo
            };
            IpcRequest::SetLedMode(led_m)
        }
    };

    // --------------

    let res: IpcResponse = client.request(&request)?;

    // Result handling
    match res {
        IpcResponse::Success => println!("Done."),
        IpcResponse::Message(msg) => println!("{}", msg),

        IpcResponse::FanRPM(cpu, gpu) => {
            println!("⚙️ Fan Speeds:");
            println!("   CPU: {} RPM", cpu);
            println!("   GPU: {} RPM", gpu);
        }

        IpcResponse::Temp(cpu, system) => {
            println!("🌡️ Temperatures:");
            println!("   CPU: {} °C", cpu);
            println!("   System: {} °C", system);
        }

        IpcResponse::KeyboardBacklight(level) => {
            println!("💡 Keyboard Backlight:");
            println!("   Level: {}", level);
        }

        IpcResponse::ChargeLimit(min, max) => {
            println!("🔋 Charge Limit (FlexiCharger):");
            if min == 0 && max == 0 {
                println!("   Mode: Full Capacity (Charges to 100%)");
            } else {
                println!("   Start charging at: {}%", min);
                println!("   Stop charging at:  {}%", max);
            }
        }

        IpcResponse::PowerLimit(profile) => {
            println!("⚡ Power Profile:");
            println!("   Current: {}", profile);
        }

        IpcResponse::Error(msg) => {
            eprintln!("❌ Error: {}", msg);
            std::process::exit(1);
        }
    }

    Ok(())
}
