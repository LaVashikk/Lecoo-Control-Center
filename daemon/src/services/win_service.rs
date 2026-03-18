use std::fs::create_dir_all;
use std::panic;
use std::path::Path;
use std::sync::{OnceLock, mpsc::Sender};
use std::time::Duration;
use file_rotate::compression::Compression;
use file_rotate::suffix::AppendCount;
use file_rotate::{ContentLimit, FileRotate};
use ipc::TelemetryData;
use log::{LevelFilter, info};
use simplelog::{Config, WriteLogger};
use windows_service::service::ServiceType;
use windows_service::{
    define_windows_service, service::{
        PowerEventParam, ServiceControl, ServiceControlAccept, ServiceExitCode, ServiceState, ServiceStatus
    }, service_control_handler::{self, ServiceControlHandlerResult}, service_dispatcher
};

use crate::services::InternalEvent;

const SERVICE_NAME: &str = "LecooControlDaemon";
static EVENT_SENDER: OnceLock<Sender<InternalEvent>> = OnceLock::new();

define_windows_service!(ffi_service_main, my_service_main);

pub fn run_as_service(tx: Sender<InternalEvent>) -> Result<(), windows_service::Error> {
    let _ = EVENT_SENDER.set(tx);
    service_dispatcher::start(SERVICE_NAME, ffi_service_main)
}

fn my_service_main(_arguments: Vec<std::ffi::OsString>) {
    info!("Starting {}...", SERVICE_NAME);
    let tx = EVENT_SENDER.get().expect("TX not initialized");
    let (tx_to_stop, rx_to_stop) = std::sync::mpsc::channel();

    let status_handle = service_control_handler::register(
        SERVICE_NAME,
        move |control_event| -> ServiceControlHandlerResult {
            match control_event {
                ServiceControl::Stop => {
                    let _ = tx_to_stop.send(());
                    let _ = tx.send(InternalEvent::SystemShuttingDown);
                    ServiceControlHandlerResult::NoError
                }
                ServiceControl::Shutdown => {
                    let _ = tx.send(InternalEvent::SystemShuttingDown);
                    ServiceControlHandlerResult::NoError
                }
                ServiceControl::Interrogate => ServiceControlHandlerResult::NoError,

                ServiceControl::PowerEvent(power_event) => {
                    match power_event {
                        PowerEventParam::Suspend => {
                            // INFO: The Lecoo Pro 14's sleep state (s2idle) is not functional, it's broken, but Fast Boot is considered suspended for the service.
                            // Treat as shutdown since we can't actually enter a proper sleep state.
                            let _ = tx.send(InternalEvent::SystemShuttingDown);
                        }

                        PowerEventParam::ResumeAutomatic
                        | PowerEventParam::ResumeSuspend => {
                            let _ = tx.send(InternalEvent::SystemWakingUp);
                        }

                        _ => {}
                    }
                    ServiceControlHandlerResult::NoError
                }
                _ => ServiceControlHandlerResult::NotImplemented,
            }
        },
    ).expect("Failed to register service control handler");

    // Notify the Service Control Manager that the service is running & ready
    let next_status = ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Running,
        controls_accepted: ServiceControlAccept::STOP
                | ServiceControlAccept::POWER_EVENT
                | ServiceControlAccept::SHUTDOWN,
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    };
    status_handle.set_service_status(next_status).unwrap();

    let _ = rx_to_stop.recv();
    // Give some time for cleanup
    std::thread::sleep(Duration::from_millis(500));

    info!("TIME TO STOP!");
    status_handle.set_service_status(ServiceStatus {
        service_type: ServiceType::OWN_PROCESS,
        current_state: ServiceState::Stopped,
        controls_accepted: ServiceControlAccept::empty(),
        exit_code: ServiceExitCode::Win32(0),
        checkpoint: 0,
        wait_hint: Duration::default(),
        process_id: None,
    }).unwrap();
}

pub fn init_logger() {
    let log_dir = Path::new("C:\\ProgramData\\LecooControl");
    if !log_dir.exists() {
        let _ = create_dir_all(log_dir);
    }

    let log_file = log_dir.join("daemon.log");

    let writer = FileRotate::new(
        log_file,
        AppendCount::new(3),
        ContentLimit::Bytes(5 * 1024 * 1024),
        Compression::None,
        None
    );

    WriteLogger::init(
        LevelFilter::Info,
        Config::default(),
        writer
    ).unwrap_or_else(|err| log::error!("Something try init logger again. {:?}", err));

    panic::set_hook(Box::new(|panic_info| {
        let location = panic_info.location().unwrap();

        let msg = match panic_info.payload().downcast_ref::<&'static str>() {
            Some(s) => *s,
            None => match panic_info.payload().downcast_ref::<String>() {
                Some(s) => &s[..],
                None => "Unknown panic message",
            },
        };

        let error = format!(
            "CRITICAL PANIC in file '{}' at line {}: {}",
            location.file(),
            location.line(),
            msg
        );

        log::error!("{}", error);
        crate::telemetry::send(
            TelemetryData::Panic { error }
        );
    }));
}
