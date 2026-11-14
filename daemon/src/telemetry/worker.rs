use std::{
    collections::VecDeque,
    sync::mpsc::{Receiver, RecvTimeoutError},
    time::Duration,
};

use lecoo_types::telemetry::{TelemetryData, TelemetryPayload};

use crate::ec;

const URL: &str = "https://lab.lavashik.dev/telemetry/v2";
const INTERVAL: Duration = Duration::from_secs(300);
const HTTP_TIMEOUT: Duration = Duration::from_secs(35);
const MAX_PENDING: usize = 32;

pub(super) fn run(rx: Receiver<TelemetryData>) {
    let agent = ureq::Agent::new_with_config(
        ureq::Agent::config_builder().timeout_global(Some(HTTP_TIMEOUT)).build(),
    );

    let mut pending: VecDeque<TelemetryPayload> = VecDeque::new();

    loop {
        match rx.recv_timeout(INTERVAL) {
            Ok(data) => queue(&mut pending, super::wrap(data)),

            Err(RecvTimeoutError::Timeout) => {
                if super::is_enabled()
                    && let Some(status) = collect_status()
                {
                    queue(&mut pending, super::wrap(status));
                }
            }

            Err(RecvTimeoutError::Disconnected) => {
                log::warn!("Telemetry channel disconnected, exiting worker.");
                break;
            }
        }

        if super::is_enabled() {
            flush(&agent, &mut pending);
        }
    }
}

fn queue(pending: &mut VecDeque<TelemetryPayload>, payload: TelemetryPayload) {
    if pending.len() >= MAX_PENDING {
        pending.pop_front();
    }
    pending.push_back(payload);
}

/// Sends everything queued as one request. On failure the queue is left intact
/// and the next tick tries again.
fn flush(agent: &ureq::Agent, pending: &mut VecDeque<TelemetryPayload>) {
    if pending.is_empty() {
        return;
    }

    // A batch only appears after a failed delivery, so the common case stays a
    // plain object and the array form doubles as a "this was retried" marker.
    let encoded = if pending.len() == 1 {
        serde_json::to_vec(&pending[0])
    } else {
        serde_json::to_vec(&pending.iter().collect::<Vec<_>>())
    };

    let body = match encoded {
        Ok(b) => b,
        Err(e) => {
            log::warn!("Failed to encode telemetry, dropping {} event(s): {e}", pending.len());
            pending.clear();
            return;
        }
    };

    match agent
        .post(URL)
        .header("X-Daemon-Version", crate::VERSION)
        .header("Content-Type", "application/json")
        .send(&body)
    {
        Ok(_) => pending.clear(),
        Err(e) => log::warn!("Failed to send telemetry, {} event(s) queued: {e}", pending.len()),
    }
}

/// A tick is skipped entirely if any part is unreadable: a half-filled sample
/// with defaults standing in for real registers would be worse than no sample.
fn collect_status() -> Option<TelemetryData> {
    let ec = crate::EC.get()?;
    let state = crate::handlers::get_state().ok()?;

    let profile = ec::read_power_profile(ec).ok()?;
    // todo: take temps and rpm from crate::metrics instead of poking the EC a second time
    // or move this whole Status tick into the metrics thread, idk yet
    let (cpu_temp_c, sys_temp_c) = ec::read_temperatures(ec).ok()?;
    let (cpu_fan_rpm, gpu_fan_rpm) = ec::read_fans_rpm(ec).ok()?;

    Some(TelemetryData::Status {
        profile,
        cpu_temp_c,
        sys_temp_c,
        cpu_fan_rpm,
        gpu_fan_rpm,
        fan_mode_cpu: state.fan_mode_cpu,
        fan_mode_gpu: state.fan_mode_gpu,
        kbd: state.keyboard_backlight,
        led: state.led_mode,
        charge: state.charge,
        soc: ec::read_battery_rsoc(ec).ok(),
        uptime_s: super::uptime_s(),
    })
}
