//! `battery` command: gauge telemetry plus the charge limit while on AC,
//! so nobody has to run `charge` just to see where charging stops

use anyhow::{Result, bail};
use ipc::{IpcClient, IpcRequest, IpcResponse};
use lecoo_types::caps::ChargeStatus;
use lecoo_types::ec_types::{BatteryInfo, ChargeIntent};
use rust_i18n::t;

pub fn run(client: &mut IpcClient) -> Result<()> {
    let info = match client.request(&IpcRequest::GetBatteryInfo)? {
        IpcResponse::BatteryInfo(info) => info,
        IpcResponse::Error(err) => crate::exit_with_error(err),
        other => bail!("Unexpected response: {other:?}"),
    };

    // Limit only matters on AC, on battery it's one round-trip for nothing
    let status = if info.current_ma >= 0 {
        match client.request(&IpcRequest::GetChargeStatus)? {
            IpcResponse::ChargeStatus(s) => Some(s),
            _ => None,
        }
    } else {
        None
    };

    print(&info, status.as_ref());
    Ok(())
}

fn print(info: &BatteryInfo, status: Option<&ChargeStatus>) {
    let state = match info.current_ma.signum() {
        1 => t!("resp_battery_state_charging"),
        -1 => t!("resp_battery_state_discharging"),
        _ => t!("resp_battery_state_idle"),
    };

    println!("{}", t!("resp_battery_title"));
    println!("{}", t!("resp_battery_state", state = state));

    if info.current_ma != 0 {
        println!(
            "{}",
            t!("resp_battery_power", power = info.power_w() : {:+.1}, current = info.current_ma : {:+})
        );
    }

    let stop = status.map_or(100, |s| stop_pct(s));
    if let Some(eta) = info.eta_min(stop) {
        let key = if info.current_ma < 0 { "resp_battery_time_left" } else { "resp_battery_until_full" };
        println!("{}", t!(key, eta = format_eta(eta)));
    }

    let volt = info.voltage_mv as f32 / 1000.0;
    match info.cell_mv() {
        Some(cell) => println!(
            "{}",
            t!("resp_battery_voltage_cell", volt = volt : {:.2}, cell = cell as f32 / 1000.0 : {:.2})
        ),
        None => println!("{}", t!("resp_battery_voltage", volt = volt : {:.2})),
    }

    let pct = info.charge_pct().map_or("n/a".to_string(), |p| format!("{p:.1}%"));
    println!("{}", t!("resp_battery_charge", rem = info.remaining_mah, full = info.full_mah, pct = pct));
    println!(
        "{}",
        t!("resp_battery_energy", rem = info.remaining_wh() : {:.1}, full = info.full_wh() : {:.1})
    );

    let health = info.health_pct().map_or("n/a".to_string(), |h| format!("{h:.1}%"));
    println!("{}", t!("resp_battery_health", health = health));
    println!("{}", t!("resp_battery_cycles", cycles = info.cycles));

    if let Some(limit) = status.and_then(format_limit) {
        println!("{}", t!("resp_battery_limit", limit = limit));
    }
}

/// SoC where charging is expected to stop under the effective intent
fn stop_pct(s: &ChargeStatus) -> u8 {
    match s.effective {
        ChargeIntent::Preserve(Some(r)) => r.max,
        ChargeIntent::Preserve(None) => s.thresholds.map_or(100, |(_, max)| max),
        // thresholds here are raw EC bytes, FlexiCharger keeps max = 0 for Full
        ChargeIntent::Full | ChargeIntent::Freeze => 100,
    }
}

/// None for plain full charging, there is no limit to show
fn format_limit(s: &ChargeStatus) -> Option<String> {
    match s.effective {
        ChargeIntent::Full => None,
        ChargeIntent::Freeze => Some(t!("resp_charge_intent_freeze").to_string()),
        ChargeIntent::Preserve(Some(r)) => Some(format!("{}% .. {}%", r.min, r.max)),
        ChargeIntent::Preserve(None) => Some(match s.thresholds {
            Some((min, max)) => format!("{min}% .. {max}%"),
            None => t!("resp_charge_intent_preserve_auto").to_string(),
        }),
    }
}

fn format_eta(min: u32) -> String {
    if min < 60 {
        t!("resp_battery_eta_m", m = min).to_string()
    } else {
        t!("resp_battery_eta_hm", h = min / 60, m = min % 60 : {:02}).to_string()
    }
}
