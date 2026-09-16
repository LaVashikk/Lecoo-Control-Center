// #![windows_subsystem = "windows"]

use anyhow::{Context, Result};
use ipc::{DaemonWorker, IpcServer};
use lecoo_types::{caps, settings::CurrentSettings, telemetry::TelemetryData};
use std::{
    sync::{Mutex, OnceLock},
    thread,
};

use crate::handlers::DaemonState;

pub mod ec;
mod handlers;
mod services;
mod telemetry;

pub static EC: OnceLock<ec::EcDevice> = OnceLock::new();
pub static STATE: OnceLock<Mutex<CurrentSettings>> = OnceLock::new();
pub static UNSUPPORTED: OnceLock<caps::UnsupportedInfo> = OnceLock::new();

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Returns the (profile, forced)
fn resolve_profile(args: &[String], board: &str) -> Option<(&'static ec::BoardProfile, bool)> {
    if let Some(i) = args.iter().position(|a| a == "--profile") {
        let id = args.get(i + 1)?;
        return match ec::by_id(id) {
            Some(p) => {
                log::warn!("Forced board profile: {}", p.id);
                Some((p, true))
            }
            None => {
                log::error!(
                    "Unknown profile id: {id}. Known: {}",
                    ec::PROFILES.iter().map(|p| p.id).collect::<Vec<_>>().join(", ")
                );
                None
            }
        };
    }
    ec::detect(board).map(|p| (p, false))
}

fn process_ipc_connection(mut conn: DaemonWorker) {
    thread::spawn(move || {
        if let Err(e) = conn.accept_handshake() {
            log::error!("Handshake rejected: {}", e);
            return;
        }

        loop {
            match conn.recv() {
                Ok(req) => {
                    let res = handlers::do_work(&req);

                    if let Err(e) = conn.send(&res) {
                        log::error!("Error sending response: {}", e);
                        break;
                    }
                }
                Err(err) => {
                    if err.kind() != std::io::ErrorKind::ConnectionReset {
                        log::error!("IPC recv error: {}", err);
                    } else {
                        // Save state on connection reset
                        if let Ok(state) = STATE.get().unwrap().try_lock() {
                            let _ = state.save();
                        } else {
                            log::warn!("Could not acquire lock to save state on connection reset");
                        }
                    }
                    break;
                }
            }
        }
    });
}

/// Process system/service events
fn process_service(rx_in_core: std::sync::mpsc::Receiver<services::InternalEvent>) {
    use lecoo_types::ec_types::{BreathConfig, PowerLedMode};
    let ec = EC.get().unwrap();

    let read_and_save_state = |ec: &ec::EcDevice| {
        if let Ok(mut state) = handlers::get_state() {
            let _ = ec::read_keyboard_backlight(ec).map(|kbd| state.keyboard_backlight = kbd);
            let _ = ec::read_power_profile(ec).map(|profile| state.power_profile = profile);
            let _ = ec::effective_charge(ec).map(|(intent, _)| state.charge = intent);
            let _ = state.save();
        } else {
            log::error!("Incomplete state save on event");
        }
    };

    loop {
        let Ok(event) = rx_in_core.recv() else { break };

        match event {
            services::InternalEvent::SystemShuttingDown | services::InternalEvent::SystemHibernating => {
                let _ = ec::apply_led_mode(ec, &PowerLedMode::Auto);
                read_and_save_state(ec);
            }

            services::InternalEvent::SystemSleeping => {
                let _ = ec::apply_led_mode(ec, &PowerLedMode::Animation(BreathConfig::sleep()));
                read_and_save_state(ec);
            }

            services::InternalEvent::SystemWakingUp => {
                let _ = handlers::get_state().map(|state| state.restore_state(ec));
            }

            services::InternalEvent::ChargerConnected => {
                if let Ok(state) = handlers::get_state() {
                    let desired = state.charge;
                    drop(state);
                    if let Err(e) = ec::reconcile(ec, &desired) {
                        log::warn!("charge reconcile on AC connect: {e}");
                    }
                }
                if let Ok(current) = ec::read_battery_rsoc(ec) {
                    let stop = ec::charge_stop_level(ec);
                    let _ = ec::apply_battery_leds(ec, current < stop, current >= stop);
                }
            }

            services::InternalEvent::ChargerDisconnected => {
                let _ = ec::apply_battery_leds(ec, false, false);
            }

            #[cfg(windows)]
            services::InternalEvent::Inited => {}
        }
    }
}

fn serve_forever(mut server: IpcServer) -> ! {
    loop {
        match server.accept() {
            Ok(conn) => process_ipc_connection(conn),
            Err(e) => log::error!("Accept error: {}", e),
        }
    }
}

fn main() -> Result<()> {
    services::init_logger();
    let (tx_to_core, rx_in_core) = std::sync::mpsc::channel();
    let args: Vec<String> = std::env::args().collect();

    // Let's give this MicroSLOP piece of the ~~shit~~ OS time to initialize the service
    // todo: a bit outdated and doesn't help
    #[cfg(windows)]
    if args.iter().any(|arg| arg == "--service") {
        let _service_worker = services::start(tx_to_core);
        let _ = rx_in_core.recv();
        thread::sleep(std::time::Duration::from_secs(2));
    } else {
        const MSG: &str = "The daemon has been launched in manual mode! Please, run it as a service. Otherwise, it will not work properly";
        eprintln!("{}", MSG);
        log::warn!("{}", MSG);
    }

    // Linux just start the service
    #[cfg(not(windows))]
    let _service_worker = services::start(tx_to_core);

    let daemon_state = CurrentSettings::load_or_default();
    telemetry::init(daemon_state.telemetry_enabled, daemon_state.telemetry_client_id);

    let server = IpcServer::bind();

    let insecure_mode = args.iter().any(|arg| arg == "--insecure");
    let board = services::get_board_name();

    let device = match resolve_profile(&args, &board) {
        Some((profile, forced)) => {
            log::info!("Detected motherboard {}.", profile.id);
            match ec::EcDevice::new_with_profile(profile, insecure_mode) {
                Ok(ec) => Some((ec, forced)),
                Err(e) => {
                    log::error!("Failed to initialize EC device: {e:#}");
                    return Err(e);
                }
            }
        }
        None => {
            if insecure_mode {
                log::error!(
                    "--insecure requires an explicit --profile <id>. Known: {}",
                    ec::PROFILES.iter().map(|p| p.id).collect::<Vec<_>>().join(", ")
                );
            }
            None
        }
    };

    let Some((ec, forced_profile)) = device else {
        let chip =
            ec::probe_chip_only().map(|(id1, id2, ver)| format!("IT{:02X}{:02X}-{:02X}", id1, id2, ver));

        let _ = UNSUPPORTED.set(caps::UnsupportedInfo { board: board.clone(), chip: chip.clone() });

        telemetry::send(TelemetryData::Unsupported { host: services::get_host_info(), chip });

        log::error!("Unsupported motherboard: {board}. Serving Unsupported over IPC.");
        serve_forever(server.context("Failed to bind IPC server")?);
    };

    if args.iter().any(|a| a == "--dump-profile") {
        println!("{}", ec::dump_profile(&ec)?);
        return Ok(());
    }

    let restore_error = daemon_state.restore_state(&ec).err().map(|e| {
        log::error!("Failed to restore EC state: {}", e);
        e.to_string()
    });

    if daemon_state.telemetry_enabled {
        let (chip_id1, chip_id2, chip_ver) = ec::read_system_info(&ec)?;

        telemetry::send(TelemetryData::Startup {
            host: services::get_host_info(),
            profile: ec.profile.id.to_string(),
            forced_profile,
            firmware: format!("IT{:02X}{:02X}-{:02X}", chip_id1, chip_id2, chip_ver),
            hram_offset: ec.hram_offset(),
            caps: Box::new(ec.profile.caps(VERSION)),
            restore_error,
        });
    }

    let _ = EC.set(ec);
    let _ = STATE.set(Mutex::new(daemon_state));

    #[cfg(target_os = "linux")]
    println!("Daemon started. For reading logs: \"journalctl -t lecoo-daemon -f\"");

    thread::Builder::new()
        .name("daemon-service-listener".into())
        .spawn(move || {
            process_service(rx_in_core);
        })
        .expect("failed to spawn daemon-service-listener");

    serve_forever(server.context("Failed to bind IPC server")?);
}
