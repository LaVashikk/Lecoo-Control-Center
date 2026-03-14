// #![windows_subsystem = "windows"]

use ipc::{IpcServer, IpcRequest, IpcResponse};
use std::{sync::OnceLock, thread};
use anyhow::Result;

mod ec;
pub use ec::EcDevice;

mod handlers;

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
    let mut server = IpcServer::bind()?;
    let _ = EC.set(EcDevice::new()?);

    // println!("System info: {:?}", handlers::get_system_state(EC.get().unwrap()).unwrap());
    // loop {
    //     println!("{:?}", handlers::get_temperatures(EC.get().unwrap()).unwrap());
    //     println!("{:?}", handlers::get_fans_rpm(EC.get().unwrap()).unwrap());
    //     println!("-------");
    //     thread::sleep(std::time::Duration::from_secs(1));
    // }
    // return Ok(());

    println!("Started! todo");
    // todo: logs
    // todo: restore last state
    // todo: add PrepareForSleep

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
