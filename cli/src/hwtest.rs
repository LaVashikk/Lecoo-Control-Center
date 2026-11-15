//! Interactive hardware feature tester.

use std::fmt;
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use anyhow::Result;
use ipc::{DaemonCommand, IpcClient, IpcRequest, IpcResponse};
use lecoo_types::caps::FanCaps;
use lecoo_types::ec_types::{FanIndex, FanMode, KeyboardBacklightLevel, PowerLedMode, PowerProfile};
use lecoo_types::settings::CurrentSettings;

use crate::help;

// --- Signal handling ---

static INTERRUPTED: AtomicBool = AtomicBool::new(false);

fn install_signal_handler() {
    let _ = ctrlc::set_handler(|| {
        INTERRUPTED.store(true, Ordering::Relaxed);
    });
}

fn is_interrupted() -> bool {
    INTERRUPTED.load(Ordering::Relaxed)
}

/// Sleep that checks for Ctrl+C every 100ms. Returns true if interrupted.
fn interruptible_sleep(dur: Duration) -> bool {
    let step = Duration::from_millis(100);
    let mut left = dur;
    while left > Duration::ZERO && !is_interrupted() {
        let chunk = left.min(step);
        thread::sleep(chunk);
        left = left.saturating_sub(chunk);
    }
    is_interrupted()
}

// --- Types ---

#[derive(Debug, Clone, Copy)]
enum Verdict {
    Yes,
    No,
    Unsure,
}

impl fmt::Display for Verdict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Verdict::Yes => write!(f, "✓"),
            Verdict::No => write!(f, "✗"),
            Verdict::Unsure => write!(f, "?"),
        }
    }
}

struct CategoryResult {
    name:    &'static str,
    overall: Verdict,
    details: Vec<(String, Verdict)>,
}

// --- Entry point ---

pub fn run(client: &mut IpcClient) -> Result<()> {
    let caps = match client.request(&IpcRequest::DaemonCommand(DaemonCommand::GetCapabilities))? {
        IpcResponse::Capabilities(c) => *c,
        other => anyhow::bail!("unexpected GetCapabilities response: {other:?}"),
    };

    let saved = match client.request(&IpcRequest::DaemonCommand(DaemonCommand::GetSettings))? {
        IpcResponse::Settings(s) => s,
        _ => anyhow::bail!("failed to read current daemon settings"),
    };

    install_signal_handler();

    println!();
    println!("{}Hardware Test — Board: {}{}", help::bold(), caps.board, help::reset());
    println!("Each feature will be demonstrated — watch/listen for changes.");
    #[rustfmt::skip]
    println!(
        "Answer: {}y{}es / {}n{}o / {}?{} unsure / {}q{}uit or Ctrl+C",
        help::bold(), help::reset(),
        help::bold(), help::reset(),
        help::bold(), help::reset(),
        help::bold(), help::reset(),
    );
    println!();

    let mut results: Vec<CategoryResult> = Vec::new();
    let mut aborted = false;

    // --- Keyboard backlight ---
    if caps.kbd.levels && !aborted {
        match test_kbd(client)? {
            None => aborted = true,
            Some(r) => results.push(r),
        }
    }

    // --- Fans ---
    if !caps.fans.is_empty() && !aborted {
        match test_fans(client, &caps.fans)? {
            None => aborted = true,
            Some(r) => results.push(r),
        }
    }

    // --- LED ring ---
    if (caps.led.on_off || caps.led.brightness) && !aborted {
        match test_led(client)? {
            None => aborted = true,
            Some(r) => results.push(r),
        }
    }

    // --- Power profiles (auto-verify, no user interaction) ---
    if caps.power_profiles.len() > 1 && !aborted {
        match test_power(client, &caps.power_profiles)? {
            None => aborted = true,
            Some(r) => results.push(r),
        }
    }

    restore_settings(client, &saved);

    if aborted {
        println!();
        println!("{}Test aborted.{}", help::err_bold_red(), help::err_reset());
    }

    if !results.is_empty() {
        println!();
        print_report(&results);
    }

    // TODO: send results via IPC to daemon for telemetry upload

    Ok(())
}

// --- Category tests ---

fn test_kbd(client: &mut IpcClient) -> Result<Option<CategoryResult>> {
    #[rustfmt::skip]
    let levels = [
        (KeyboardBacklightLevel::Off,    "off"),
        (KeyboardBacklightLevel::Low,    "low"),
        (KeyboardBacklightLevel::Medium, "medium"),
        (KeyboardBacklightLevel::High,   "high"),
    ];

    println!("  {}▶ Keyboard Backlight{}", help::bold(), help::reset());
    println!("    Watch your keyboard — backlight levels will cycle.");
    println!();

    for _ in 0..4 {
        for &(level, name) in &levels {
            if is_interrupted() {
                break;
            }
            set_quiet(client, &IpcRequest::SetKeyboardBacklight(level));
            print!("\r    cycling: {}{:<8}{}", help::bold(), name, help::reset());
            let _ = io::stdout().flush();
            if interruptible_sleep(Duration::from_millis(300)) {
                break;
            }
        }
        if is_interrupted() {
            break;
        }
    }
    print!("\r    cycling: done        ");
    println!();

    if is_interrupted() {
        return Ok(None);
    }

    let overall = match ask_verdict("    Did the backlight change?")? {
        Some(v) => v,
        None => return Ok(None),
    };

    let mut details = Vec::new();

    // Drill down into individual levels only if user confirmed it works
    if matches!(overall, Verdict::Yes) {
        println!("    Testing individual levels:");
        for &(level, name) in &levels {
            set_quiet(client, &IpcRequest::SetKeyboardBacklight(level));
            if interruptible_sleep(Duration::from_millis(500)) {
                return Ok(None);
            }
            match ask_verdict(&format!("      → {name}:"))? {
                Some(v) => details.push((name.to_owned(), v)),
                None => return Ok(None),
            }
        }
    }

    println!();
    Ok(Some(CategoryResult { name: "kbd", overall, details }))
}

fn test_fans(client: &mut IpcClient, fans: &[FanCaps]) -> Result<Option<CategoryResult>> {
    println!("  {}▶ Fan Control{}", help::bold(), help::reset());
    println!("    Listen — fans will spin up to full speed.");
    println!();

    for f in fans {
        set_quiet(client, &IpcRequest::SetFanMode { fan: f.index, mode: FanMode::Full });
    }

    // Let fans ramp up, then ask while they're still loud
    if interruptible_sleep(Duration::from_secs(3)) {
        spin_down(client, fans);
        return Ok(None);
    }

    let overall = match ask_verdict("    Can you hear the fans at full speed?")? {
        Some(v) => v,
        None => {
            spin_down(client, fans);
            return Ok(None);
        }
    };

    spin_down(client, fans);

    println!();
    Ok(Some(CategoryResult { name: "fan", overall, details: Vec::new() }))
}

fn test_led(client: &mut IpcClient) -> Result<Option<CategoryResult>> {
    println!("  {}▶ Power LED Ring{}", help::bold(), help::reset());
    println!("    Watch the rear LED — brightness will cycle.");
    println!();

    let steps: &[u8] = &[0, 80, 160, 255, 160, 80, 0, 255];
    for &val in steps {
        if is_interrupted() {
            break;
        }
        set_quiet(client, &IpcRequest::SetLedMode(PowerLedMode::Custom(val)));
        print!("\r    brightness: {}{:<5}{}", help::bold(), val, help::reset());
        let _ = io::stdout().flush();
        if interruptible_sleep(Duration::from_millis(400)) {
            break;
        }
    }
    print!("\r    cycling: done         ");
    println!();

    set_quiet(client, &IpcRequest::SetLedMode(PowerLedMode::Auto));

    if is_interrupted() {
        return Ok(None);
    }

    let overall = match ask_verdict("    Did the LED change?")? {
        Some(v) => v,
        None => return Ok(None),
    };

    println!();
    Ok(Some(CategoryResult { name: "led", overall, details: Vec::new() }))
}

/// Programmatic verification — set each profile, read back, compare.
/// No user interaction needed since profile changes aren't directly observable.
fn test_power(client: &mut IpcClient, profiles: &[PowerProfile]) -> Result<Option<CategoryResult>> {
    println!("  {}▶ Power Profiles (auto-verify){}", help::bold(), help::reset());

    let mut details = Vec::new();
    for &p in profiles {
        if is_interrupted() {
            return Ok(None);
        }

        let name = format!("{p:?}").to_lowercase();
        set_quiet(client, &IpcRequest::SetPowerProfile(p));
        interruptible_sleep(Duration::from_millis(200));

        let ok = matches!(
            client.request(&IpcRequest::GetPowerProfile),
            Ok(IpcResponse::PowerLimit(got)) if got == p
        );

        println!("    {} {}", if ok { "✓" } else { "✗" }, name);
        details.push((name, if ok { Verdict::Yes } else { Verdict::No }));
    }

    let all_ok = details.iter().all(|(_, v)| matches!(v, Verdict::Yes));

    println!();
    Ok(Some(CategoryResult {
        name: "power",
        overall: if all_ok { Verdict::Yes } else { Verdict::No },
        details,
    }))
}

// --- Helpers ---

fn set_quiet(client: &mut IpcClient, req: &IpcRequest) {
    let _ = client.request(req);
}

fn spin_down(client: &mut IpcClient, fans: &[FanCaps]) {
    for f in fans {
        set_quiet(client, &IpcRequest::SetFanMode { fan: f.index, mode: FanMode::Auto });
    }
}

fn ask_verdict(prompt: &str) -> Result<Option<Verdict>> {
    if is_interrupted() {
        return Ok(None);
    }

    print!("{} [y/n/?/q] ", prompt);
    io::stdout().flush()?;

    loop {
        let mut buf = String::new();
        match io::stdin().read_line(&mut buf) {
            Ok(0) => return Ok(None),
            Err(_) => return Ok(None),
            Ok(_) => {}
        }

        if is_interrupted() {
            return Ok(None);
        }

        match buf.trim().to_lowercase().as_str() {
            "y" | "yes" => return Ok(Some(Verdict::Yes)),
            "n" | "no" => return Ok(Some(Verdict::No)),
            "?" | "idk" | "unsure" => return Ok(Some(Verdict::Unsure)),
            "q" | "quit" | "exit" => return Ok(None),
            _ => {
                print!("    y/n/?/q: ");
                io::stdout().flush()?;
            }
        }
    }
}

fn restore_settings(client: &mut IpcClient, s: &CurrentSettings) {
    let _ = client.request(&IpcRequest::SetKeyboardBacklight(s.keyboard_backlight));
    let _ = client.request(&IpcRequest::SetFanMode { fan: FanIndex::Cpu, mode: s.fan_mode_cpu });
    let _ = client.request(&IpcRequest::SetFanMode { fan: FanIndex::Gpu, mode: s.fan_mode_gpu });
    let _ = client.request(&IpcRequest::SetPowerProfile(s.power_profile));
    let _ = client.request(&IpcRequest::SetLedMode(s.led_mode));
    println!();
    println!("{}Settings restored.{}", help::bold(), help::reset());
}

fn print_report(results: &[CategoryResult]) {
    println!("{}═══ Test Report ═══{}", help::bold(), help::reset());
    println!();

    for r in results {
        if r.details.is_empty() {
            println!("  {} {}", r.overall, r.name);
        } else {
            let parts: Vec<String> = r.details.iter().map(|(name, v)| format!("{name}: {v}")).collect();
            println!("  {} {}  ({})", r.overall, r.name, parts.join(", "));
        }
    }

    let passed = results.iter().filter(|r| matches!(r.overall, Verdict::Yes)).count();
    let failed = results.iter().filter(|r| matches!(r.overall, Verdict::No)).count();
    let unsure = results.iter().filter(|r| matches!(r.overall, Verdict::Unsure)).count();

    println!();
    println!(
        "  {} tested: {}✓ {}{} / {}✗ {}{} / {}? {}{}",
        results.len(),
        help::bold(),
        passed,
        help::reset(),
        help::bold(),
        failed,
        help::reset(),
        help::bold(),
        unsure,
        help::reset(),
    );
}
