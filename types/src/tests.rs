//! Wire-format guards. The whole point of the serde migration is that a field
//! added on one side does not break the other, so the tests check exactly that
//! rather than re-asserting the struct definitions.

use crate::caps::*;
use crate::ec_types::*;
use crate::settings::CurrentSettings;
use crate::telemetry::*;

fn roundtrip<T>(value: T)
where
    T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
{
    let json = serde_json::to_string(&value).expect("serialize");
    let back: T = serde_json::from_str(&json).unwrap_or_else(|e| panic!("deserialize {json}: {e}"));
    assert_eq!(value, back, "json = {json}");
}

#[test]
fn ec_types_roundtrip() {
    for mode in [FanMode::Auto, FanMode::Full, FanMode::Turbo, FanMode::Custom(180)] {
        roundtrip(mode);
    }
    for level in
        [KeyboardBacklightLevel::Off, KeyboardBacklightLevel::High, KeyboardBacklightLevel::Custom(7)]
    {
        roundtrip(level);
    }
    roundtrip(PowerLedMode::Auto);
    roundtrip(PowerLedMode::Custom(200));
    roundtrip(PowerLedMode::Animation(BreathConfig::sleep()));
    roundtrip(ChargeIntent::Full);
    roundtrip(ChargeIntent::Freeze);
    roundtrip(ChargeIntent::Preserve(None));
    roundtrip(ChargeIntent::Preserve(Some(ChargeRange { min: 70, max: 80 })));
}

#[test]
fn telemetry_roundtrip() {
    roundtrip(TelemetryData::Unsupported { host: HostInfo::default(), chip: None });
    roundtrip(TelemetryData::ErrorLog {
        level: "ERROR".into(),
        message: "ec write failed".into(),
        module: "lecoo_ec_daemon::ec".into(),
        line: 42,
        count: 4,
    });
    roundtrip(TelemetryData::Panic {
        message: "unwrap on None".into(),
        file: "src/main.rs".into(),
        line: 7,
        thread: Some("telemetry-worker".into()),
    });
}

/// The server reads `$.id` and `$.data.t` through generated columns, so those
/// two paths are load-bearing: renaming either silently empties the table.
#[test]
fn payload_shape_matches_server_columns() {
    let payload = TelemetryPayload {
        id: 0xDEAD_BEEF,
        session: 1,
        data: TelemetryData::ErrorLog {
            level: "WARN".into(),
            message: "boom".into(),
            module: "m".into(),
            line: 1,
            count: 1,
        },
    };

    let json: serde_json::Value = serde_json::to_value(&payload).unwrap();
    assert_eq!(json["id"], "0x00000000DEADBEEF");
    assert_eq!(json["session"], "0x0000000000000001");
    assert_eq!(json["data"]["t"], "ErrorLog");
    assert_eq!(json["data"]["c"]["message"], "boom");
}

/// `#[serde(other)]` only covers an unknown tag with no content: the variant it
/// maps to is a unit one, so a payload that does carry `c` still fails. Pinned
/// here because the limitation is easy to mistake for full forward compat.
#[test]
fn unknown_event_degrades_only_without_content() {
    let tag_only: TelemetryData = serde_json::from_str(r#"{"t":"HwTest"}"#).unwrap();
    assert_eq!(tag_only, TelemetryData::Unknown);

    assert!(serde_json::from_str::<TelemetryData>(r#"{"t":"HwTest","c":{"board":"n155"}}"#).is_err());
}

/// A config written by an older version must still load: this is what replaced
/// `SETINGS_SCHEMA_VER` and the full settings reset that came with it.
#[test]
fn settings_backward_compat() {
    let old = r#"{"telemetry_enabled":true,"telemetry_client_id":"0x00000000DEADBEEF"}"#;
    let settings: CurrentSettings = serde_json::from_str(old).unwrap();
    assert_eq!(settings.telemetry_client_id, 0xDEAD_BEEF);
    assert_eq!(settings.power_profile, PowerProfile::Default);
}

/// Unknown fields are ignored rather than rejected. That property is the whole
/// compatibility story, so it gets a test of its own.
#[test]
fn unknown_field_ignored() {
    let future = r#"{"min":70,"max":80,"hysteresis":3}"#;
    let range: ChargeRange = serde_json::from_str(future).unwrap();
    assert_eq!((range.min, range.max), (70, 80));
}

/// Range validation comes free with the type; no manual check to forget.
#[test]
fn out_of_range_rejected() {
    assert!(serde_json::from_str::<FanMode>(r#"{"t":"Custom","c":300}"#).is_err());
}

/// Numbers from a real N155A, 80% charged
#[test]
fn battery_eta() {
    let bat = |current_ma| BatteryInfo {
        design_mah: 5200,
        design_mv: 15400,
        full_mah: 5200,
        remaining_mah: 4160,
        voltage_mv: 16580,
        cycles: 161,
        current_ma,
        cells: 4,
    };

    assert_eq!(bat(-520).eta_min(100), Some(480));
    // Charging aims at the limit, not at 100%: 5200 * 90% - 4160 = 520 mAh left
    assert_eq!(bat(1040).eta_min(90), Some(30));
    assert_eq!(bat(1040).eta_min(80), None, "already at the limit");
    assert_eq!(bat(0).eta_min(100), None);
    assert_eq!(bat(0).cell_mv(), Some(4145));
}

/// Prints a payload shaped exactly like the daemon's. Kept as an ignored test
/// so the server can be probed with real bytes instead of hand-written JSON:
/// `cargo test -p lecoo-types sample_payload -- --ignored --nocapture`
#[test]
#[ignore]
fn sample_payload() {
    let payload = TelemetryPayload {
        id: 0x0123_4567_89AB_CDEF,
        session: 0xFEED_FACE,
        data: TelemetryData::Startup {
            host: HostInfo {
                vendor: "Lecoo".into(),
                product: "Ling Jiu 16p".into(),
                motherboard: "N155".into(),
                bios: Some("1.09 (03/14/2025)".into()),
                cpu: "Intel(R) Core(TM) i5-12450H".into(),
                os: "Arch Linux".into(),
                arch: "x86_64".into(),
            },
            profile: "n155".into(),
            forced_profile: false,
            firmware: "IT5570-A1".into(),
            hram_offset: 0x0800,
            caps: Box::new(Capabilities {
                board: "n155".into(),
                daemon_version: "0.5.2-beta".into(),
                fans: vec![FanCaps { index: FanIndex::Cpu, duty_max: 100 }],
                sensors: vec![SensorRole::Cpu, SensorRole::Sys],
                power_profiles: vec![PowerProfile::Silent, PowerProfile::Default],
                kbd: KbdCaps { on_off: true, levels: true, custom: true },
                led: LedCaps { on_off: true, brightness: true, animation: true },
                battery_leds: true,
                battery_info: true,
                charge: ChargeCaps::default(),
            }),
            restore_error: None,
        },
    };

    println!("{}", serde_json::to_string(&payload).unwrap());
}
