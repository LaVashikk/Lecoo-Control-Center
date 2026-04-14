use std::sync::mpsc::Sender;
use std::{panic, thread};
use zbus::blocking::Connection;
use super::InternalEvent;

pub fn init_logger() {
    systemd_journal_logger::JournalLog::new()
        .unwrap()
        .with_extra_fields(vec![("VERSION", crate::VERSION)])
        .with_syslog_identifier("lecoo-daemon".to_string())
        .install().unwrap();
    log::set_max_level(log::LevelFilter::Info);

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
            ipc::TelemetryData::Panic { error: error.clone() }
        );
    }));
}

#[zbus::proxy(
    interface = "org.freedesktop.systemd1.Manager",
    default_service = "org.freedesktop.systemd1",
    default_path = "/org/freedesktop/systemd1"
)]
trait SystemdManager {
    /// systemd signals subscription
    fn subscribe(&self) -> zbus::Result<()>;

    /// New signal for systemd job creation
    #[zbus(signal)]
    fn job_new(&self, id: u32, job: zbus::zvariant::OwnedObjectPath, unit: String) -> zbus::Result<()>;
}

#[zbus::proxy(
    interface = "org.freedesktop.systemd1.Job",
    default_service = "org.freedesktop.systemd1"
)]
trait SystemdJob {
    #[zbus(property)]
    fn job_type(&self) -> zbus::Result<String>;
}

#[zbus::proxy(
    interface = "org.freedesktop.login1.Manager",
    default_service = "org.freedesktop.login1",
    default_path = "/org/freedesktop/login1"
)]
trait LoginManager {
    /// PrepareForSleep(start: bool)
    #[zbus(signal)]
    fn prepare_for_sleep(&self, start: bool) -> zbus::Result<()>;

    /// PrepareForShutdown(start: bool)
    #[zbus(signal)]
    fn prepare_for_shutdown(&self, start: bool) -> zbus::Result<()>;
}

pub fn run_as_service(tx: Sender<InternalEvent>) -> zbus::Result<()> {
    let conn = Connection::system()?;

    // Suspend/Hibernate detection
    let tx_systemd = tx.clone();
    let conn_systemd = conn.clone();
    let _sleep_thread = thread::Builder::new()
        .name("systemd-jobs".into())
        .spawn(move || {
            let manager = match SystemdManagerProxyBlocking::new(&conn_systemd) {
                Ok(m) => m,
                Err(e) => { log::error!("systemd proxy error: {e}"); return; }
            };

            if let Err(e) = manager.subscribe() {
                log::error!("failed to subscribe to systemd signals: {e}");
                return;
            }

            let signals = match manager.receive_job_new() {
                Ok(s) => s,
                Err(e) => { log::error!("job_new subscribe error: {e}"); return; }
            };

            for sig in signals {
                let Ok(args) = sig.args() else { continue };

                let is_sleep_unit = match args.unit.as_str() {
                    "suspend.target" | "hibernate.target" | "hybrid-sleep.target" | "suspend-then-hibernate.target" => true,
                    _ => false,
                };

                if !is_sleep_unit { continue; }

                if let Ok(job_proxy) = SystemdJobProxyBlocking::builder(&conn_systemd)
                    .path(args.job.clone())
                    .unwrap()
                    .build()
                {
                    if let Ok(job_type) = job_proxy.job_type() {
                        if job_type != "start" {
                            log::debug!("Ignoring '{}' job for {}", job_type, args.unit);
                            continue;
                        }
                    } else {
                        continue;
                    }
                }
                let event = match args.unit.as_str() {
                    "suspend.target" => InternalEvent::SystemSleeping,
                    _ => InternalEvent::SystemHibernating,
                };

                if tx_systemd.send(event).is_err() {
                    break;
                }
            }
        })
        .expect("failed to spawn systemd-jobs thread");

    // wakeups
    let tx_sleep = tx.clone();
    let conn_sleep = conn.clone();
    let _wakeups_thread = thread::Builder::new()
        .name("logind-sleep".into())
        .spawn(move || {
            let proxy = match LoginManagerProxyBlocking::new(&conn_sleep) {
                Ok(p) => p,
                Err(e) => { log::error!("sleep proxy: {e}"); return; }
            };
            let signals = match proxy.receive_prepare_for_sleep() {
                Ok(s) => s,
                Err(e) => { log::error!("sleep subscribe: {e}"); return; }
            };

            for sig in signals {
                let Ok(args) = sig.args() else { continue };

                if !args.start {
                    if tx_sleep.send(InternalEvent::SystemWakingUp).is_err() {
                        break;
                    }
                }
            }
        })
        .expect("failed to spawn logind-sleep");

    // battery status
    let tx_power = tx.clone();
    let conn_power = conn.clone();
    let _power_thread = thread::Builder::new()
        .name("upower-monitor".into())
        .spawn(move || {
            let proxy = match UPowerProxyBlocking::new(&conn_power) {
                Ok(p) => p,
                Err(e) => { log::error!("upower proxy error: {e}"); return; }
            };

            let changed_stream = proxy.receive_on_battery_changed();

            // TODO: Yes, if connect the charger to 98% and wait until it reaches max, the indicator WILL NOT update.
            // and yes, i need to find a way to update it, or use full UPower proxy. but I'M SOOOO LAZY.
            // who cares about power indicator :) It just works.
            for changed in changed_stream {
                if let Ok(on_battery) = changed.get() {
                    let event = if on_battery {
                        InternalEvent::ChargerDisconnected
                    } else {
                        InternalEvent::ChargerConnected
                    };

                    if tx_power.send(event).is_err() {
                        break;
                    }
                }
            }
        })
        .expect("failed to spawn upower-monitor thread");

    let proxy = LoginManagerProxyBlocking::new(&conn)?;

    for sig in proxy.receive_prepare_for_shutdown()? {
        let Ok(args) = sig.args() else { continue };
        if args.start {
            let _ = tx.send(InternalEvent::SystemShuttingDown);
            break; // no need to listen further after shutdown
        }
    }

    Ok(())
}

#[cfg(target_os = "linux")]
pub fn get_system_info() -> (String, String) {
    use std::fs;

    let cpu_name = fs::read_to_string("/proc/cpuinfo")
        .unwrap_or_default()
        .lines()
        .find(|line| line.starts_with("model name"))
        .and_then(|line| line.split(':').nth(1))
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "Unknown CPU".to_string());

    let os_name = fs::read_to_string("/etc/os-release")
        .unwrap_or_default()
        .lines()
        .find(|line| line.starts_with("PRETTY_NAME="))
        .and_then(|line| line.split('=').nth(1))
        .map(|s| s.trim_matches('"').to_string())
        .unwrap_or_else(|| "Linux".to_string());

    (cpu_name, os_name)
}
