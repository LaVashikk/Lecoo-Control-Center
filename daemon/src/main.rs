// #![windows_subsystem = "windows"]

use ipc::{BreathConfig, HardwareAnimation, IpcRequest, IpcResponse, IpcServer};
use log::info;
use std::{sync::OnceLock, thread};
use anyhow::Result;

mod ec;
mod handlers;
mod services;

pub use ec::EcDevice;

pub static EC: OnceLock<EcDevice> = OnceLock::new();

fn do_work(req: &IpcRequest) -> IpcResponse {
    let ec = EC.get().unwrap();

    let result = match req {

        IpcRequest::Ping => Ok(IpcResponse::Success),
        IpcRequest::Read(_offset) => todo!(), // Ok(IpcResponse::Message(format!("REG: {}", ec.read_reg(*offset).unwrap_or(0)))), // TODO: DEV STUFF!

        // GETTERS:
        IpcRequest::GetSystemState => handlers::get_system_state(ec),

        IpcRequest::GetFansRPM => handlers::get_fans_rpm(ec),

        IpcRequest::GetTemperatures => handlers::get_temperatures(ec),

        IpcRequest::GetChargeLimit => handlers::get_charge_limit(ec),

        IpcRequest::GetPowerProfile => handlers::get_power_profile(ec),

        IpcRequest::GetKeyboardBacklight => handlers::get_keyboard_backlight(ec),

        // SETTERS:
        IpcRequest::SetPowerProfile(profile) => handlers::set_power_profile(ec, profile),

        IpcRequest::SetFanMode { fan, mode } => handlers::set_fan_mode(ec, fan, mode),

        IpcRequest::SetKeyboardBacklight(level) => handlers::set_keyboard_backlight(ec, level),

        IpcRequest::SetChargeLimit(limit) => handlers::set_charge_limit(ec, limit),

        IpcRequest::SetLedMode(mode) => handlers::set_led_mode(ec, mode),
    };

    match result {
        Ok(success) => success,
        Err(err) => IpcResponse::Error(format!("Processing request failed: {}", err)),
    }
}


fn main() -> Result<()> {
    services::init_logger();

    let mut server = IpcServer::bind()?;
    let ec = EcDevice::new()?;
    // ec.dump_memory_range(0x0000, 0x0FFF);

    let _ = EC.set(ec);
    let (tx_to_core, rx_in_core) = std::sync::mpsc::channel();

    // println!("\nSystem info: {:?}", handlers::get_system_state(EC.get().unwrap()).unwrap());
    // loop {
    //     println!("{:?}", handlers::get_temperatures(EC.get().unwrap()).unwrap());
    //     println!("{:?}", handlers::get_fans_rpm(EC.get().unwrap()).unwrap());
    //     println!("-------");
    //     thread::sleep(std::time::Duration::from_secs(5));
    // }
    // return Ok(());

    // todo: logs
    // todo: restore last state
    // todo: add PrepareForSleep
    // todo: telemetry
    // build version

    let _service_worker = services::start(tx_to_core);
    info!("Daemon started.");

    thread::Builder::new()
        .name("daemon-service-listener".into())
        .spawn(move || {
            loop {
                match rx_in_core.recv() {
                    Ok(event) => {
                        info!("Received service event: {:?}", event);
                        match event {
                            services::InternalEvent::SystemShuttingDown => {
                                let ec = EC.get().unwrap();
                                // handlers::set_led_mode(ec, &ipc::PowerLedMode::Auto).unwrap();
                                handlers::set_led_mode(ec, &ipc::PowerLedMode::Auto).unwrap();
                            },

                            services::InternalEvent::SystemSleeping => {
                                let ec = EC.get().unwrap();
                                handlers::set_led_mode(ec, &ipc::PowerLedMode::Animation(HardwareAnimation::Breathing(BreathConfig::vacuum()))).unwrap();
                            },

                            services::InternalEvent::SystemWakingUp => {
                                let ec = EC.get().unwrap();
                                let _ = handlers::set_led_mode(ec, &ipc::PowerLedMode::Custom(50)); // wrong! restore last state
                            },
                        };

                    }
                    Err(_) => break,
                }
            }
        })
        .expect("failed to spawn daemon-service-listener");

    loop {
        match server.accept() {
            Ok(mut conn) => {
                thread::spawn(move || {
                    loop {
                        match conn.recv::<IpcRequest>() {
                            Ok(req) => {
                                let res = do_work(&req);

                                if let Err(e) = conn.send(&res) {
                                    eprintln!("Error sending response: {}", e);
                                    break;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                });
            }
            Err(e) => eprintln!("Accept error: {}", e),
        }
    }
}
