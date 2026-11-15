//! The one place that polls the EC on a timer
//!
//! Two paces: passive in the background, active while some client is watching.
//! "Watching" is a lease, not a flag: every `GetMetrics { live: Some(ms) }` renews it.
// todo: histograms, telemetry here

mod overheat;
use std::{
    sync::{Mutex, OnceLock},
    thread::Thread,
    time::{Duration, Instant},
};

use anyhow::Result;
use lecoo_types::metrics::{Metrics, Sample};

use crate::ec::{self, EcDevice};

/// A sample is ~6 EC bytes, each one a dozen syscalls plus LPC cycles on one core.
/// Cheap, but nothing in the background needs more than a minute
const PASSIVE_TICK: Duration = Duration::from_secs(60);
/// The EC itself refreshes these registers about once a second, faster just reads the same bytes
const MIN_TICK: Duration = Duration::from_secs(1);
/// A few missed client polls in a row before we call it gone
const LEASE: Duration = Duration::from_secs(15);

pub static METRICS: Mutex<Metrics> = Mutex::new(Metrics { last: None });

/// Fastest pace any live client asked for, and until when that lease holds
static LIVE: Mutex<Option<(Duration, Instant)>> = Mutex::new(None);
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

/// Someone is looking at the numbers and wants a sample every `want`, for the next `LEASE`.
/// Keeps the fastest live request. When that client goes quiet its lease runs out,
/// and the next slower client to renew takes over within one of its own polls
pub fn watch(want: Duration) {
    let want = want.clamp(MIN_TICK, PASSIVE_TICK);
    let now = Instant::now();
    let Ok(mut live) = LIVE.lock() else { return };

    let wake = match *live {
        Some((pace, until)) if now < until && want > pace => return,
        Some((pace, until)) => until <= now || want < pace,
        None => true,
    };
    *live = Some((want, now + LEASE));
    drop(live);

    // the thread may be parked or on a slower pace rn. so ВСТАВАЙ, ЕРЖАН!
    if wake && let Some(t) = THREAD.get() {
        t.unpark();
    }
}

fn pace() -> Duration {
    match LIVE.lock().ok().and_then(|l| *l) {
        Some((pace, until)) if Instant::now() < until => pace,
        _ => PASSIVE_TICK,
    }
}

fn run() {
    let ec = crate::EC.get().expect("metrics started before EC");
    let mut overheat = overheat::Overheat::default();
    let mut last: Option<Instant> = None;

    loop {
        // pace is re-read every round, so a fresh lease shortens the current wait
        let due = last.map_or_else(Instant::now, |l| l + pace());
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
        retry_pending_charge(ec);
    }
}

/// TODO: FREAKING WORKAROUND!!
// Charge preconditions come and go without any event: AC fires before the charger ramps up,
/// and charging up into the N161A hold window fires nothing at all. So we here do some shit
fn retry_pending_charge(ec: &EcDevice) {
    let Ok(desired) = crate::handlers::get_state().map(|s| s.charge) else {
        return;
    };
    if let Err(e) = ec::reconcile(ec, &desired) {
        log::debug!("Charge reconcile skipped: {e}");
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
        uptime_s: crate::telemetry::uptime_s(),
    })
}
