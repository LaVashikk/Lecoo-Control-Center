#![allow(dead_code)]
use std::{sync::mpsc::Sender, thread::JoinHandle};

#[cfg(target_os = "windows")]
pub mod win_service;
#[cfg(target_os = "linux")]
pub mod systemd_service;

#[cfg(target_os = "windows")]
pub use win_service::{init_logger, get_system_info};
#[cfg(target_os = "linux")]
pub use systemd_service::{init_logger, get_system_info};

#[derive(Debug, Clone, Copy)]
pub enum InternalEvent {
    SystemShuttingDown,
    SystemSleeping,
    SystemHibernating,
    SystemWakingUp,
    #[cfg(windows)]
    Inited,
}

pub fn start(tx: Sender<InternalEvent>) -> JoinHandle<()> {
    std::thread::Builder::new()
        .name("service-listener".into())
        .spawn(move || {
            #[cfg(target_os = "linux")]
            if let Err(e) = systemd_service::run_as_service(tx) {
                log::error!("logind listener failed: {e}");
            }
            #[cfg(windows)]
            if let Err(e) = win_service::run_as_service(tx) {
                log::error!("Windows service listener failed: {e}");
            }
        })
        .expect("failed to spawn service-listener")
}
