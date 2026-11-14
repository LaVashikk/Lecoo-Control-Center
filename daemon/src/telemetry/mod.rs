use std::{
    sync::{
        OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::Sender,
    },
    time::Instant,
};

use lecoo_types::telemetry::{TelemetryData, TelemetryPayload};

mod errors;
mod panic;
mod worker;

pub use errors::install as install_logger;
pub use panic::install as install_panic_hook;

static TELEMETRY_ENABLED: AtomicBool = AtomicBool::new(true);
static TELEMETRY_TX: OnceLock<Sender<TelemetryData>> = OnceLock::new();
static TELEMETRY_ID: AtomicU64 = AtomicU64::new(0);
static SESSION_ID: OnceLock<u64> = OnceLock::new();
static STARTED_AT: OnceLock<Instant> = OnceLock::new();

pub fn init(start_enabled: bool, client_id: u64) {
    let (tx, rx) = std::sync::mpsc::channel();

    TELEMETRY_ENABLED.store(start_enabled, Ordering::Relaxed);
    TELEMETRY_TX.set(tx).expect("Telemetry already initialized");
    TELEMETRY_ID.store(client_id, Ordering::Relaxed);
    let _ = STARTED_AT.set(Instant::now());

    std::thread::Builder::new()
        .name("telemetry-worker".into())
        .spawn(|| worker::run(rx))
        .expect("Failed to spawn telemetry worker");
}

/// Queues an event. Dropped silently when telemetry is off or before `init`,
/// which is why callers never have to check either.
pub fn send(data: TelemetryData) {
    if is_enabled()
        && let Some(tx) = TELEMETRY_TX.get()
    {
        let _ = tx.send(data);
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

/// Identifies one daemon process
fn session_id() -> u64 {
    *SESSION_ID.get_or_init(|| {
        use std::hash::{DefaultHasher, Hash, Hasher};
        let mut hasher = DefaultHasher::new();
        std::time::SystemTime::now().hash(&mut hasher);
        hasher.finish()
    })
}

pub(crate) fn uptime_s() -> u64 {
    STARTED_AT.get().map(|t| t.elapsed().as_secs()).unwrap_or(0)
}

fn wrap(data: TelemetryData) -> TelemetryPayload {
    TelemetryPayload { id: TELEMETRY_ID.load(Ordering::Relaxed), session: session_id(), data }
}
