// #![windows_subsystem = "windows"]

use anyhow::Result;
use ipc::{ChargeLimit, CurrentSettings, IpcConnection, IpcRequest, IpcServer};
use log::info;
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

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Process an incoming IPC connection by handling requests in a loop
fn process_ipc_connection(mut conn: IpcConnection) {
    thread::spawn(move || {
        if let Err(e) = conn.accept_handshake() {
            log::error!("Handshake rejected: {}", e);
            return;
        }

        loop {
            match conn.recv::<IpcRequest>() {
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
    use ipc::{BreathConfig, PowerLedMode};
    let ec = EC.get().unwrap();
    let read_and_save_state = |ec: &ec::EcDevice| {
        if let Ok(mut state) = handlers::get_state() {
            let _ = ec::read_keyboard_backlight(ec).map(|kbd| state.keyboard_backlight = kbd);
            let _ = ec::read_power_profile(ec).map(|profile| state.power_profile = profile);
            if let Ok((min, max)) = ec::read_charge_limit(ec)
                && let Some(limit) = ChargeLimit::from_predefined(min, max)
            {
                state.charge_limit = limit;
            }
            let _ = state.save();
        } else {
            log::error!("Incomplete state save on event");
        }
    };

    while let Ok(event) = rx_in_core.recv() {
        match event {
            services::InternalEvent::SystemShuttingDown
            | services::InternalEvent::SystemHibernating => {
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
                if let Ok(current) = ec::read_battery_rsoc(ec)
                    && let Ok(charge_limits) = ec::read_charge_limit(ec)
                {
                    if current >= charge_limits.1 {
                        let _ = ec::apply_battery_leds(ec, false, true);
                    } else {
                        let _ = ec::apply_battery_leds(ec, true, false);
                    }
                }
            }

            services::InternalEvent::ChargerDisconnected => {
                let _ = ec::apply_battery_leds(ec, false, false);
            }

            #[cfg(windows)]
            services::InternalEvent::Inited => {}
        };
    }
}

fn main() -> Result<()> {
    services::init_logger();
    let (tx_to_core, rx_in_core) = std::sync::mpsc::channel();
    let args: Vec<String> = std::env::args().collect();

    // Let's give this MicroSLOP piece of the ~~shit~~ OS time to initialize the service
    #[cfg(windows)]
    if args.iter().any(|arg| arg == "--service") {
        let _service_worker = services::start(tx_to_core);
        let _ = rx_in_core.recv();
        thread::sleep(std::time::Duration::from_secs(2));
    } else {
        println!(
            "The daemon has been launched in manual mode! Please, run it as a service. Otherwise, it will not work properly."
        );
        log::warn!(
            "The daemon has been launched in manual mode! Please, run it as a service. Otherwise, it will not work properly."
        );
    }

    // Linux just start the service
    #[cfg(not(windows))]
    let _service_worker = services::start(tx_to_core);

    let mut server = IpcServer::bind().map_err(|e| {
        log::error!("Failed to bind IPC server: {}", e);
        e
    })?;

    let insecure_mode = args.iter().any(|arg| arg == "--insecure");
    let ec = ec::EcDevice::new(insecure_mode).map_err(|e| {
        log::error!("Failed to initialize EC device: {}", e);
        e
    })?;

    let daemon_state = CurrentSettings::load_or_default();
    if let Err(e) = daemon_state.restore_state(&ec) {
        log::error!("Failed to restore EC state: {}", e);
    }

    telemetry::init(
        daemon_state.telemetry_enabled,
        daemon_state.telemetry_client_id,
    );

    if daemon_state.telemetry_enabled {
        let (cpu_name, os_name, motherboard) = services::get_system_info();

        let (chip_id1, chip_id2, chip_ver) = ec::read_system_info(&ec)?;
        telemetry::send(ipc::TelemetryData::Startup {
            firmware: format!("IT{:02X}{:02X}-{:02X}", chip_id1, chip_id2, chip_ver),
            offset: ec.hram_offset,
            cpu: cpu_name,
            os: os_name,
            motherboard,
        });
    }

    let _ = EC.set(ec);
    let _ = STATE.set(Mutex::new(daemon_state));

    #[cfg(target_os = "linux")]
    println!("Daemon started. For reading logs: \"journalctl -t lecoo-daemon -f\"");
    info!("Daemon started.");

    thread::Builder::new()
        .name("daemon-service-listener".into())
        .spawn(move || {
            process_service(rx_in_core);
        })
        .expect("failed to spawn daemon-service-listener");

    loop {
        match server.accept() {
            Ok(conn) => {
                process_ipc_connection(conn);
            }
            Err(e) => eprintln!("Accept error: {}", e),
        }
    }
}
