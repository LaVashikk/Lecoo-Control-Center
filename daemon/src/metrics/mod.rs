//! The one place that polls the EC on a timer
//!
//! Two paces: passive in the background, active while some client is watching.
//! "Watching" is a lease, not a flag: every `GetMetrics { live: true }` extends it.
// todo: histograms, telemetry here

mod overheat;
use std::{
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    thread::Thread,
    time::{Duration, Instant},
};

use anyhow::Result;
use lecoo_types::metrics::{Metrics, Sample};

use crate::ec::{self, EcDevice};

/// A sample is ~6 EC bytes, each one a dozen syscalls plus LPC cycles on one core.
/// Cheap, but nothing in the background needs more than a minute
const PASSIVE_TICK: Duration = Duration::from_secs(60);
// todo: pick a better one, should match how often the GUI redraws its graphs
const ACTIVE_TICK: Duration = Duration::from_secs(4); // todo 2: or should we pass that FROM client? like `cli monitoring 0.2` then i needed 0.2s
/// A few missed client polls in a row before we call it gone
const LEASE: Duration = Duration::from_secs(15);

pub static METRICS: Mutex<Metrics> = Mutex::new(Metrics { last: None });

/// ms since `EPOCH` until which we stay on the active pace
static LIVE_UNTIL: AtomicU64 = AtomicU64::new(0);
static EPOCH: OnceLock<Instant> = OnceLock::new();
/// Handle of the metrics thread, only used to `unpark` it
static THREAD: OnceLock<Thread> = OnceLock::new();

/// Call after `EC` and `STATE` are set
pub fn start() {
    let handle = std::thread::Builder::new()
        .name("metrics".into())
        .spawn(run)
        .expect("failed to spawn metrics");
    let _ = THREAD.set(handle.thread().clone());
}

/// Someone is looking at the numbers, sample on the active pace for the next `LEASE`
pub fn watch() {
    let now = now_ms();
    let prev = LIVE_UNTIL.fetch_max(now + LEASE.as_millis() as u64, Ordering::Relaxed);

    // we were passive, so the thread may be parked rn. ВСТАВАЙ, ЕРЖАН!
    if prev <= now && let Some(t) = THREAD.get() {
        t.unpark();
    }
}

fn is_live() -> bool {
    now_ms() < LIVE_UNTIL.load(Ordering::Relaxed)
}

fn now_ms() -> u64 {
    EPOCH.get_or_init(Instant::now).elapsed().as_millis() as u64
}

fn run() {
    let ec = crate::EC.get().expect("metrics started before EC");
    let mut overheat = overheat::Overheat::default();
    let mut last: Option<Instant> = None;

    loop {
        // pace is re-read every round, so a fresh lease shortens the current wait
        let pace = if is_live() { ACTIVE_TICK } else { PASSIVE_TICK };
        let due = last.map_or_else(Instant::now, |l| l + pace);
        let now = Instant::now();

        if now < due {
            // woken early by watch() or spuriously, either way we just go round and recompute
            std::thread::park_timeout(due - now);
            continue;
        }
        last = Some(now);

        match sample(ec) {
            Ok(s) => {
                // log::info!("[DEV] new sample: {:?}", s); // todo: remove later
                if let Ok(mut m) = METRICS.lock() {
                    m.last = Some(s);
                }
                overheat.check(ec, s.cpu_c, PASSIVE_TICK);
                // todo: feed histograms here
            }
            Err(e) => log::debug!("Metrics tick skipped: {e}"),
        }
    }
}

fn sample(ec: &EcDevice) -> Result<Sample> {
    let (cpu_c, sys_c) = ec::read_temperatures(ec)?;
    let (cpu_fan_rpm, gpu_fan_rpm) = ec::read_fans_rpm(ec)?;
    Ok(Sample {
        cpu_c,
        sys_c,
        cpu_fan_rpm,
        gpu_fan_rpm,
        uptime_s: crate::telemetry::uptime_s()
    })
}
