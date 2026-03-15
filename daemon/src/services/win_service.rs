use std::fs::create_dir_all;
use std::path::Path;
use std::sync::{OnceLock, mpsc::Sender};
use std::time::Duration;
use file_rotate::compression::Compression;
use file_rotate::suffix::AppendCount;
use file_rotate::{ContentLimit, FileRotate};
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
    init_logger();
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
    ).expect("Failed to initialize logger");
}


// pub fn install_service() -> windows_service::Result<()> {
//     // 1. Подключаемся к менеджеру служб Windows (Service Control Manager)
//     // Требуются права Администратора!
//     let manager_access = ServiceManagerAccess::CONNECT | ServiceManagerAccess::CREATE_SERVICE;
//     let service_manager = ServiceManager::local_computer(None::<&str>, manager_access)?;

//     // 2. Получаем полный путь к нашему текущему .exe файлу
//     let exe_path = env::current_exe().expect("Failed to get executable path");

//     // 3. Формируем базовую информацию о службе
//     let service_info = ServiceInfo {
//         name: OsString::from(SERVICE_NAME),                 // Внутреннее имя службы
//         display_name: OsString::from(SERVICE_DISPLAY_NAME), // Короткое имя (колонка "Имя" в диспетчере)
//         service_type: ServiceType::OWN_PROCESS,
//         start_type: ServiceStartType::AutoStart,            // Автозапуск вместе с Windows
//         error_control: ServiceErrorControl::Normal,
//         executable_path: exe_path,                          // Путь к нашему бинарнику
//         launch_arguments: vec![],                           // Можно добавить аргументы, если нужно
//         dependencies: vec![],                               // Зависимости (например, если нужна сеть)
//         account_name: None,                                 // None означает запуск от системного аккаунта NT AUTHORITY\SYSTEM
//         account_password: None,
//     };

//     // 4. Создаем службу в системе.
//     // Обязательно запрашиваем право CHANGE_CONFIG, чтобы сразу после создания добавить длинное описание.
//     let service = service_manager.create_service(&service_info, ServiceAccess::CHANGE_CONFIG)?;

//     // 5. Устанавливаем длинное описание (колонка "Описание" в диспетчере служб)
//     service.set_description(SERVICE_LONG_DESCRIPTION)?;

//     // Опционально: можно сразу настроить перезапуск службы при падении
//     // service.set_failure_actions_on_non_crash_failures(true)?;

//     println!("Service '{}' successfully installed!", SERVICE_NAME);
//     Ok(())
// }
