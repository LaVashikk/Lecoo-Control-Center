use std::time::{Duration, Instant};

use lecoo_types::ec_types::BreathConfig;

use crate::ec::{self, EcDevice};

/// Alarm stays on until the CPU drops this far below the threshold, so 94/95 doesn't flicker
const HYSTERESIS_C: u8 = 5;

/// Every hot tick re-arms a flash that outlives the tick.
#[derive(Default)]
pub(super) struct Overheat {
    active: bool,
    armed_at: Option<Instant>,
}

impl Overheat {
    pub(super) fn check(&mut self, ec: &'static EcDevice, cpu_c: u8, rearm_every: Duration) {
        let Ok((threshold, restore)) = crate::handlers::get_state().map(|st| (st.overheat_alert_c, st.led_mode)) else {
            return;
        };

        let was = self.active;
        self.active = match threshold {
            None => false,
            Some(t) if was => cpu_c >= t.saturating_sub(HYSTERESIS_C),
            Some(t) => cpu_c >= t,
        };

        match (was, self.active) {
            // todo: maybe send some sys-notification?
            (false, true) => log::warn!("CPU at {cpu_c}°C, overheat alarm on"),
            (true, false) => {
                log::info!("CPU back to {cpu_c}°C, overheat alarm off");
                self.armed_at = None;
                // the pending flash would hold PANIC for up to a minute more, kill it now
                // bumps the LED generation, so that flash won't restore anything later either
                if let Err(e) = ec::apply_led_mode(ec, &restore) {
                    log::debug!("LED restore after overheat failed: {e}");
                }
                return;
            }
            _ => {}
        }

        if !self.active || self.armed_at.is_some_and(|t| t.elapsed() < rearm_every) {
            return;
        }

        // user switching the LED while it's hot gets overridden next tick, ON PURPOSE!
        match ec::flash_led(ec, BreathConfig::PANIC, rearm_every + Duration::from_secs(5), restore) {
            Ok(()) => self.armed_at = Some(Instant::now()),
            Err(e) => log::debug!("Overheat flash failed: {e}"),
        }
    }
}
