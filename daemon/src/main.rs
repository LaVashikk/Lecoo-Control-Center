// #![windows_subsystem = "windows"]

use anyhow::Result;
use ipc::{CurrentSettings, IpcConnection, IpcRequest, IpcServer};
use log::info;
use std::{
    sync::{Mutex, OnceLock},
    thread,
};

use crate::handlers::DaemonState;

pub mod ec;
mod handlers;
mod services;

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
                            log::warn!(
                                "Could not acquire lock to save state on connection reset"
                            );
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
    use ipc::{PowerLedMode, BreathConfig, HardwareAnimation};
    let ec = EC.get().unwrap();

    loop {
        match rx_in_core.recv() {
            Ok(event) => {
                match event {
                    services::InternalEvent::SystemShuttingDown => {
                        ec::apply_led_mode(ec, &PowerLedMode::Auto).expect("Failed to set LED mode to Auto");
                        if let Ok(mut state) = handlers::get_state() {
                            state.keyboard_backlight = ec::read_keyboard_backlight(ec).expect("Failed to read keyboard backlight");
                            let _ = state.save();
                        } else {
                            log::error!("Incomplete state save on shutdown");
                        }
                    }

                    services::InternalEvent::SystemSleeping => {
                        let _ = ec::apply_led_mode(
                            ec,
                            &PowerLedMode::Animation(HardwareAnimation::Breathing(
                                BreathConfig::sleep(),
                            )),
                        );
                    }

                    services::InternalEvent::SystemWakingUp => {
                        let led_mode = handlers::get_state().map(|state| state.led_mode).unwrap_or(PowerLedMode::Auto);
                        let _ = ec::apply_led_mode(ec, &led_mode);
                    }
                };
            }
            Err(_) => break,
        }
    }
}

fn main() -> Result<()> {
    services::init_logger();

    let mut server = IpcServer::bind()?;
    let ec = ec::EcDevice::new()?;
    let daemon_state = CurrentSettings::load_or_default();

    if let Err(e) = daemon_state.restore_state(&ec) {
        log::error!("Failed to restore EC state: {}", e);
    }

    let _ = EC.set(ec);
    let _ = STATE.set(Mutex::new(daemon_state));
    let (tx_to_core, rx_in_core) = std::sync::mpsc::channel();

    // todo: telemetry
    // daemon commands

    let _service_worker = services::start(tx_to_core);
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
