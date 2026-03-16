use std::sync::mpsc::Sender;
use std::thread;
use zbus::blocking::Connection;
use super::InternalEvent;

pub fn init_logger() {
    systemd_journal_logger::JournalLog::new()
        .unwrap()
        .with_extra_fields(vec![("VERSION", crate::VERSION)])
        .with_syslog_identifier("lecoo-daemon".to_string())
        .install().unwrap();
    log::set_max_level(log::LevelFilter::Info);
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

    let tx_sleep = tx.clone();
    let conn_sleep = conn.clone();

    let _sleep_thread = thread::Builder::new()
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
                let event = if args.start {
                    InternalEvent::SystemSleeping
                } else {
                    InternalEvent::SystemWakingUp
                };
                if tx_sleep.send(event).is_err() {
                    return;
                }
            }
        })
        .expect("failed to spawn logind-sleep");

    let proxy = LoginManagerProxyBlocking::new(&conn)?;

    for sig in proxy.receive_prepare_for_shutdown()? {
        let Ok(args) = sig.args() else { continue };
        if args.start {
            let _ = tx.send(InternalEvent::SystemShuttingDown);
            // no need to listen further after shutdown
        }
    }

    Ok(())
}
