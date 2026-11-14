use super::*;
use lecoo_types::caps::SensorRole;
use lecoo_types::ec_types::{ChargeIntent, ChargeRange, FanIndex, PowerProfile};

#[rustfmt::skip]
const PROFILE_MAP: &[(u8, PowerProfile)] = &[
    (1, PowerProfile::Silent),
    (2, PowerProfile::Default),
    (3, PowerProfile::Performance),
];

const N155A_FANS: &[FanSpec] = &[
    FanSpec {
        index: FanIndex::Cpu,
        rpm_msb: Addr::Ram(0x76),
        rpm_lsb: Addr::Ram(0x77),
        duty: Addr::Ram(0x4B),
        policy: Addr::Ram(0x4F),
        policy_auto: 0x00,
        policy_manual: 0x40,
        duty_full: 165,
        duty_max: 220,
    },
    FanSpec {
        index: FanIndex::Gpu,
        rpm_msb: Addr::Ram(0x79),
        rpm_lsb: Addr::Ram(0x7A),
        duty: Addr::Ram(0x4D),
        policy: Addr::Ram(0x4E),
        policy_auto: 0x00,
        policy_manual: 0x40,
        duty_full: 165,
        duty_max: 220,
    },
];

const N155A_SENSORS: &[SensorSpec] = &[
    SensorSpec { role: SensorRole::Cpu, addr: Addr::Ram(0x70) },
    SensorSpec { role: SensorRole::Sys, addr: Addr::Ram(0x62) },
];

#[rustfmt::skip]
const N155A_PWM: PwmSpec = PwmSpec {
    bypass:     Addr::Ram(0x55),
    mux:        Addr::Reg(0x1610),
    prescaler:  Addr::Reg(0x1800),
    cycle:      Addr::Reg(0x1801),
    duty:       Addr::Reg(0x1802),
    clock_ctrl: Addr::Reg(0x1823),
};

#[rustfmt::skip]
const N155A_PRESETS: &[PresetSpec] = &[
    PresetSpec { name: "full",     intent: ChargeIntent::Full },
    PresetSpec { name: "desk",     intent: ChargeIntent::Preserve(Some(ChargeRange { min: 40, max: 50 })) },
    PresetSpec { name: "lifespan", intent: ChargeIntent::Preserve(Some(ChargeRange { min: 55, max: 60 })) },
    PresetSpec { name: "balanced", intent: ChargeIntent::Preserve(Some(ChargeRange { min: 70, max: 80 })) },
    PresetSpec { name: "high",     intent: ChargeIntent::Preserve(Some(ChargeRange { min: 90, max: 95 })) },
];

/// No custom range on this board, so "lifespan" maps directly to the firmware
/// policy instead of being silently downgraded from a range at call time.
#[rustfmt::skip]
const N161A_PRESETS: &[PresetSpec] = &[
    PresetSpec { name: "full",     intent: ChargeIntent::Full },
    PresetSpec { name: "lifespan", intent: ChargeIntent::Preserve(None) },
    PresetSpec { name: "hold",     intent: ChargeIntent::Freeze },
];

#[rustfmt::skip]
pub const N155A: BoardProfile = BoardProfile {
    id: "N155A",
    dmi: &["N155A"],
    hram_candidates: &[0xC400, 0xC000, 0x0400, 0x0000, 0xE000],

    fans: N155A_FANS,
    sensors: N155A_SENSORS,
    power: Some(PowerSpec { reg: Addr::Ram(0xB1), map: PROFILE_MAP }),
    battery: Some(BatterySpec {
        rsoc: Addr::Ram(0x93),
        charge_current: None,
    }),
    battery_gauge: Some(BatteryGaugeSpec {
        cells:       4,
        design_cap:  Addr::Ram(0x84),
        design_volt: Addr::Ram(0x86),
        full_cap:    Addr::Ram(0x88),
        status:      Addr::Ram(0x8C),
        current:     Addr::Ram(0x8D),
        rem_cap:     Addr::Ram(0x8F),
        voltage:     Addr::Ram(0x91),
        cycles:      Addr::Ram(0x95),
    }),

    kbd: KbdOps::Levels {
        reg:            Addr::Banked(0x0F05),
        custom_val:     Addr::Reg(0x1806),
        bypass_timeout: Addr::Ram(0xA6),
        mux:            Addr::Reg(0x1614),
    },
    led: LedOps::PwmBreath {
        pwm: N155A_PWM,
        breath_en:    Addr::Reg(0x1850),
        breath_step:  Addr::Reg(0x1851),
        breath_delay: Addr::Reg(0x1852),
    },
    battery_leds: Some(BatteryLedsSpec {
        port: Addr::Reg(0x1601),
        orange_mask: 0x02,
        white_mask: 0x04,
        active_low: true,
    }),
    charge: ChargeOps::FlexiCharger {
        min: Addr::Ram(0xBC),
        max: Addr::Ram(0xBB),
        bounds: (20, 100),
        full_max: 0,
        freeze_verified: false,
    },
    charge_presets: N155A_PRESETS,
};

pub const N155C: BoardProfile = BoardProfile { id: "N155C", dmi: &["N155C"], ..N155A };

pub const N155D: BoardProfile = BoardProfile {
    id: "N155D",
    dmi: &["N155D"],
    // TODO: https://4pda.to/forum/index.php?showtopic=1108499&view=findpost&p=144510075
    // for 155D for now Idk the correct breath_en registry!! So for now led not working :<
    led: LedOps::None,
    // led: LedOps::PwmBreath {
    //     pwm: PwmSpec { bypass: Addr::Ram(0x4F), ..N155A_PWM },
    //     breath_en:    Addr::Reg(0x1850),
    //     breath_step:  Addr::Reg(0x1851),
    //     breath_delay: Addr::Reg(0x1852),
    // },
    // TODO: rn I have no idea, will that works or not and what const-health value
    battery_gauge: None,
    ..N155A
};

pub const N161A: BoardProfile = BoardProfile {
    id: "N161A",
    dmi: &["N161A"],
    kbd: KbdOps::PwmDuty { reg: Addr::Banked(0x1803) },
    charge: ChargeOps::PolicyByte {
        reg: Addr::Reg(0x0414),
        fw_thresholds: (61, 76),
        arm_below: 60,
        freeze_soc: (62, 84),
    },
    charge_presets: N161A_PRESETS,
    // TODO: rn I have no idea, will that works or not
    battery_gauge: None,
    ..N155A
};

pub static PROFILES: &[&BoardProfile] = &[&N155A, &N155C, &N155D, &N161A];

pub fn detect(board_name: &str) -> Option<&'static BoardProfile> {
    PROFILES.iter().copied().find(|p| p.dmi.iter().any(|pat| board_name.contains(pat)))
}

pub fn by_id(id: &str) -> Option<&'static BoardProfile> {
    PROFILES.iter().copied().find(|p| p.id.eq_ignore_ascii_case(id))
}
