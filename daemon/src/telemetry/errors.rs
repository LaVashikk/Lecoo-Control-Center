//! Forwards `log::error!` records into telemetry.

use std::{
    collections::BTreeMap,
    hash::{DefaultHasher, Hash, Hasher},
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
    },
};

use lecoo_types::telemetry::TelemetryData;
use log::{Level, Log, Metadata, Record};

/// Distinct call sites tracked per process. A new site past this point is
/// dropped, but sites already known keep counting.
const MAX_DISTINCT: usize = 32;

/// Occurrence counter per call site. Never reset: the count is meaningful only
/// against the session id that travels with every payload.
static SEEN: Mutex<BTreeMap<u64, u32>> = Mutex::new(BTreeMap::new());

/// Set while the panic hook runs. The hook logs the panic *and* emits a
/// dedicated `Panic` event; without this the same text would arrive twice.
pub(super) static IN_PANIC: AtomicBool = AtomicBool::new(false);

struct Forwarder {
    inner: Box<dyn Log>,
}

impl Log for Forwarder {
    fn enabled(&self, metadata: &Metadata) -> bool {
        self.inner.enabled(metadata)
    }

    fn flush(&self) {
        self.inner.flush();
    }

    fn log(&self, record: &Record) {
        self.inner.log(record);

        if record.level() > Level::Warn || IN_PANIC.load(Ordering::Relaxed) {
            return;
        }

        let module = record.module_path().unwrap_or("<unknown>");
        if is_own_module(module) {
            return;
        }

        let line = record.line().unwrap_or(0);
        let Some(count) = occurrence(module, line) else {
            return;
        };

        super::send(TelemetryData::ErrorLog {
            level: record.level().as_str().to_owned(),
            message: record.args().to_string(),
            module: module.to_owned(),
            line,
            count,
        });
    }
}

fn is_own_module(module: &str) -> bool {
    match module_path!().rsplit_once("::") {
        Some((telemetry_root, _)) => module.starts_with(telemetry_root),
        None => false,
    }
}

/// Counts this hit and decides whether it is worth reporting, returning the
/// occurrence number when it is.
fn occurrence(module: &str, line: u32) -> Option<u32> {
    let mut hasher = DefaultHasher::new();
    module.hash(&mut hasher);
    line.hash(&mut hasher);
    let key = hasher.finish();

    let mut seen = SEEN.lock().ok()?;

    match seen.get_mut(&key) {
        Some(count) => {
            *count = count.saturating_add(1);
            count.is_power_of_two().then_some(*count)
        }
        None => {
            // The cap only refuses new sites; a known one must keep counting,
            // otherwise the loudest bug goes quiet once the table fills up.
            if seen.len() >= MAX_DISTINCT {
                return None;
            }
            seen.insert(key, 1);
            Some(1)
        }
    }
}

/// Installs `inner` as the global logger, wrapped so error records also reach telemetry
pub fn install(inner: Box<dyn Log>, level: log::LevelFilter) {
    if log::set_boxed_logger(Box::new(Forwarder { inner })).is_err() {
        eprintln!("Logger already installed, telemetry error forwarding is off");
        return;
    }
    log::set_max_level(level);
}
