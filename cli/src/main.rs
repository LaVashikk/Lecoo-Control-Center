use std::io::{self, Write};

use anyhow::Result;
use ipc::{DaemonCommand, IpcClient, IpcRequest, IpcResponse};
use lecoo_types::caps::Capabilities;
use lecoo_types::ec_types::{ChargeIntent, FanIndex, FanMode};

mod battery;
mod defs;
mod help;
mod hwtest;
mod parser;
mod raw;

use defs::FanTarget;
use parser::{CliCommand, DaemonSubcommand};

rust_i18n::i18n!("locales", fallback = "en");
use rust_i18n::t;

fn fetch_capabilities(client: &mut IpcClient) -> Option<Capabilities> {
    match client.request(&IpcRequest::DaemonCommand(DaemonCommand::GetCapabilities)) {
        Ok(IpcResponse::Capabilities(caps)) => Some(*caps),
        _ => None,
    }
}

fn main() -> Result<()> {
    if let Some(sys_locale) = sys_locale::get_locale() {
        rust_i18n::set_locale(&sys_locale);
    }

    let mut client_res = IpcClient::connect();
    let caps = client_res.as_mut().ok().and_then(fetch_capabilities);

    let command = match parser::parse_args(caps.as_ref()) {
        Ok(cmd) => cmd,
        Err(e) => {
            eprintln!(
                "{}{}error:{} {}",
                help::err_bold(),
                help::err_bold_red(),
                help::err_reset(),
                e.message
            );
            eprintln!();
            eprintln!("{}", help::get_usage(e.command.as_deref(), true));
            eprintln!();
            let help_cmd = format!("{}--help{}", help::err_bold(), help::err_reset());
            eprintln!("{}", t!("help_more_info", help = help_cmd));
            std::process::exit(1);
        }
    };

    match &command {
        CliCommand::Help { target } => {
            help::print_help(target.as_deref(), caps.as_ref());
            return Ok(());
        }
        CliCommand::Version => {
            println!("{} v{}", help::bin_name(), env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        _ => {}
    }

    let mut client = match client_res {
        Ok(c) => c,
        Err(e) => anyhow::bail!("{} ({})", t!("err_daemon_connection"), e),
    };

    if matches!(command, CliCommand::HwTest) {
        return hwtest::run(&mut client);
    }
    if let CliCommand::Read { addr, count } = command {
        return raw::dump(&mut client, addr, count);
    }
    if matches!(command, CliCommand::Battery) {
        return battery::run(&mut client);
    }

    let request = match command {
        CliCommand::Info => IpcRequest::GetSystemState,
        CliCommand::Temps => IpcRequest::GetTemperatures,
        CliCommand::Fans => IpcRequest::GetFansRPM,

        CliCommand::Monitoring { rate } => {
            let update_rate = (rate.unwrap_or(1.0) * 1000.0) as u64;
            println!("{}", t!("msg_monitoring_start", rate = update_rate));
            loop {
                let IpcResponse::Temps { cpu_c: cpu, sys_c: system } =
                    client.request(&IpcRequest::GetTemperatures)?
                else {
                    unreachable!()
                };
                let IpcResponse::FanRpm { cpu: cpu_fan, gpu: gpu_fan } =
                    client.request(&IpcRequest::GetFansRPM)?
                else {
                    unreachable!()
                };

                print!(
                    "\r{}      ",
                    t!("msg_monitoring_loop", cpu = cpu, sys = system, cpu_f = cpu_fan, gpu_f = gpu_fan)
                );
                io::stdout().flush().unwrap();
                std::thread::sleep(std::time::Duration::from_millis(update_rate));
            }
        }

        CliCommand::Power { profile } => match profile {
            Some(p) => IpcRequest::SetPowerProfile(p),
            None => IpcRequest::GetPowerProfile,
        },

        CliCommand::Fan { target, mode } => {
            if matches!(mode, FanMode::Turbo) {
                println!("{}{}{}", help::err_bold_red(), t!("warn_fan_turbo"), help::err_reset());
            }
            match target {
                FanTarget::Cpu => IpcRequest::SetFanMode { fan: FanIndex::Cpu, mode },
                FanTarget::Gpu => IpcRequest::SetFanMode { fan: FanIndex::Gpu, mode },
                FanTarget::Both => {
                    let _: IpcResponse =
                        client.request(&IpcRequest::SetFanMode { fan: FanIndex::Cpu, mode })?;
                    IpcRequest::SetFanMode { fan: FanIndex::Gpu, mode }
                }
            }
        }

        CliCommand::Charge { intent } => match intent {
            Some(i) => IpcRequest::SetChargeIntent(i),
            None => IpcRequest::GetChargeStatus,
        },

        CliCommand::Kbd { level } => match level {
            Some(l) => IpcRequest::SetKeyboardBacklight(l),
            None => IpcRequest::GetKeyboardBacklight,
        },

        CliCommand::Led { mode } => IpcRequest::SetLedMode(mode),
        CliCommand::LedFlash { effect, duration_ms } => IpcRequest::FlashLed { effect, duration_ms },
        CliCommand::LedSleep { mode } => IpcRequest::DaemonCommand(DaemonCommand::SetLedOnSleep(mode)),

        CliCommand::Daemon(sub) => match sub {
            DaemonSubcommand::TelemetryEnable => {
                IpcRequest::DaemonCommand(DaemonCommand::ActivateTelemetry(true))
            }
            DaemonSubcommand::TelemetryDisable => {
                IpcRequest::DaemonCommand(DaemonCommand::ActivateTelemetry(false))
            }
            DaemonSubcommand::TelemetryId => IpcRequest::DaemonCommand(DaemonCommand::GetTelemetryId),
            DaemonSubcommand::SettingsReset => IpcRequest::DaemonCommand(DaemonCommand::RestoreDefaults),
            DaemonSubcommand::SettingsRead => IpcRequest::DaemonCommand(DaemonCommand::GetSettings),
            DaemonSubcommand::SettingsApply => IpcRequest::DaemonCommand(DaemonCommand::ApplySettings),
            DaemonSubcommand::Version => IpcRequest::GetSystemState,
        },

        CliCommand::Write { addr, val } => IpcRequest::DaemonCommand(DaemonCommand::Write(addr, val)),

        CliCommand::Help { .. }
        | CliCommand::Version
        | CliCommand::HwTest
        | CliCommand::Read { .. }
        | CliCommand::Battery => unreachable!(),
    };

    let res: IpcResponse = client.request(&request)?;

    match res {
        IpcResponse::Success => println!("{}", t!("msg_success")),

        IpcResponse::SystemInfo(info) => {
            println!(
                "{}",
                t!("resp_sys_info", chip = info.chip, rev = info.revision, offset = info.hram_offset : {:04X}, ver = info.daemon_version)
            );
        }

        IpcResponse::FanRpm { cpu, gpu } => {
            println!("{}", t!("resp_fans_rpm", cpu = cpu, gpu = gpu));
        }

        IpcResponse::Temps { cpu_c: cpu, sys_c: sys } => {
            println!("{}", t!("resp_temps", cpu = cpu, sys = sys));
        }

        IpcResponse::KeyboardBacklight(lvl) => {
            println!("{}", t!("resp_kbd_backlight", lvl = lvl));
        }

        IpcResponse::ChargeLimit { min, max, current: cur } => {
            println!("{}", t!("resp_charge_title"));
            if min == 0 && max == 0 {
                println!("{}", t!("resp_charge_full"));
            } else {
                println!("{}", t!("resp_charge_range", min = min, max = max));
            }
            println!("{}", t!("resp_charge_current", cur = cur));
        }

        IpcResponse::ChargeStatus(status) => {
            let format_intent = |intent: &ChargeIntent| -> String {
                match intent {
                    ChargeIntent::Full => t!("resp_charge_intent_full").to_string(),
                    ChargeIntent::Preserve(Some(range)) => {
                        t!("resp_charge_intent_preserve", min = range.min, max = range.max).to_string()
                    }
                    ChargeIntent::Preserve(None) => t!("resp_charge_intent_preserve_auto").to_string(),
                    ChargeIntent::Freeze => t!("resp_charge_intent_freeze").to_string(),
                }
            };

            println!("{}", t!("resp_charge_status_title"));
            println!("{}", t!("resp_charge_status_desired", intent = format_intent(&status.desired)));
            println!("{}", t!("resp_charge_status_effective", intent = format_intent(&status.effective)));
            println!("{}", t!("resp_charge_status_battery", soc = status.soc));
            if let Some(reason) = status.pending {
                println!("{}", t!("resp_charge_status_pending", reason = reason));
            }
        }

        IpcResponse::PowerLimit(prof) => {
            println!("{}", t!("resp_power_title"));
            println!("{}", t!("resp_power_current", prof = prof));
        }

        IpcResponse::Capabilities(c) => println!("{:#?}", c),

        // `read` and `battery` send their own requests and print the answers themselves
        IpcResponse::Byte(_) | IpcResponse::BatteryInfo(_) => unreachable!(),

        IpcResponse::TelemetryDisabledInfo => {
            println!("{}", t!("resp_telemetry_disabled"));
        }

        IpcResponse::Error(err) => exit_with_error(err),

        IpcResponse::Settings(s) => println!("{:#?}", s),

        IpcResponse::TelemetryId(id) => {
            println!("{}", t!("resp_telemetry_id", id = id : {:016X}));
        }
    }

    Ok(())
}

pub(crate) fn exit_with_error(err: ipc::IpcError) -> ! {
    if let Some(info) = err.unsupported {
        println!("{}", t!("msg_unsupported_title", board = info.board));
        println!("{}", t!("msg_unsupported_hint"));
    } else if err.code == ipc::ErrorCode::Precondition {
        println!("{}", t!("msg_precondition", reason = err.message));
    } else {
        eprintln!("{}{}{}", help::err_red(), t!("msg_error", msg = err.message), help::err_reset());
    }
    std::process::exit(1);
}
