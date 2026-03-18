use std::{sync::{OnceLock, atomic::{AtomicBool, AtomicU64, Ordering}, mpsc::{Receiver, RecvTimeoutError, Sender}}, time::Duration};

use ipc::{TelemetryData, TelemetryPayload};

use crate::ec;

static TELEMETRY_INTERVAL: Duration = Duration::from_secs(300);
static TELEMETRY_ENABLED: AtomicBool = AtomicBool::new(true);
static TELEMETRY_TX: OnceLock<Sender<TelemetryData>> = OnceLock::new();
static TELEMETRY_ID: AtomicU64 = AtomicU64::new(0);

pub fn init(start_enabled: bool, client_id: u64) {
    let (tx, rx) = std::sync::mpsc::channel();

    TELEMETRY_ENABLED.store(start_enabled, Ordering::Relaxed);
    TELEMETRY_TX.set(tx).expect("Telemetry already initialized");
    TELEMETRY_ID.store(client_id, Ordering::Relaxed);

    std::thread::Builder::new()
        .name("telemetry-worker".into())
        .spawn(|| worker_loop(rx))
        .expect("Failed to spawn telemetry worker");
}

pub fn send(data: TelemetryData) {
    if is_enabled() {
        if let Some(tx) = TELEMETRY_TX.get() {
            let _ = tx.send(data);
        }
    }
}

pub fn enable() {
    TELEMETRY_ENABLED.store(true, Ordering::Relaxed);
    log::info!("Telemetry enabled");
}

pub fn disable() {
    TELEMETRY_ENABLED.store(false, Ordering::Relaxed);
    log::info!("Telemetry disabled");
}

pub fn is_enabled() -> bool {
    TELEMETRY_ENABLED.load(Ordering::Relaxed)
}

fn worker_loop(rx: Receiver<TelemetryData>) {
    loop {
        match rx.recv_timeout(TELEMETRY_INTERVAL) {
            Ok(message) => {
                if is_enabled() {
                    send_to_server(message);
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                if is_enabled() {
                    if let Some(ec) = crate::EC.get() {
                        if let Ok(profile) = ec::read_power_profile(ec) {
                            if let Ok((cpu_temp, sys_temp)) = ec::read_temperatures(ec) {
                                if let Ok((cpu_rpm, gpu_rpm)) = ec::read_fans_rpm(ec) {
                                    let status = TelemetryData::Status {
                                        profile,
                                        temps: [cpu_temp as u32, sys_temp as u32],
                                        fans: [cpu_rpm as u32, gpu_rpm as u32],
                                    };
                                    send_to_server(status);
                                }
                            }
                        }
                    }
                }
            }
            Err(RecvTimeoutError::Disconnected) => {
                log::warn!("Telemetry channel disconnected, exiting worker.");
                break;
            }
        }
    }
}

fn send_to_server(data: TelemetryData) {
    let id = TELEMETRY_ID.load(Ordering::Relaxed);

    let payload = TelemetryPayload {
        id,
        data
    };

    let config = bincode::config::standard();
    let encoded_bytes = match bincode::encode_to_vec(&payload, config) {
        Ok(bytes) => bytes,
        Err(e) => {
            log::error!("Failed to encode telemetry data: {}", e);
            return;
        }
    };

    match ureq::post("https://lab.lavashik.dev/telemetry")
        .header("X-Daemon-Version", crate::VERSION)
        .header("Content-Type", "application/octet-stream")
        .send(&encoded_bytes)
    {
        Ok(_response) => {},
        Err(e) => log::warn!("Failed to send telemetry: {}", e),
    }
}
