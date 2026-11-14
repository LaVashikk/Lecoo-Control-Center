use super::*;
use std::collections::HashSet;

/// Documented address overlaps that are not yet resolved on hardware.
const KNOWN_CONFLICTS: &[(&str, &str, &str)] = &[("N155D", "fan.Cpu.policy", "led.bypass")];

fn is_known(board: &str, a: &str, b: &str) -> bool {
    KNOWN_CONFLICTS
        .iter()
        .any(|(bd, x, y)| *bd == board && ((*x == a && *y == b) || (*x == b && *y == a)))
}

#[test]
fn ids_and_dmi_patterns_do_not_shadow() {
    let mut ids = HashSet::new();
    for p in PROFILES {
        assert!(ids.insert(p.id), "duplicate profile id: {}", p.id);
    }
    for a in PROFILES {
        for b in PROFILES {
            if a.id == b.id {
                continue;
            }
            for pa in a.dmi {
                for pb in b.dmi {
                    assert!(!pa.contains(pb), "DMI pattern {pa} ({}) shadows {pb} ({})", a.id, b.id);
                }
            }
        }
    }
}

#[test]
fn topology_is_consistent() {
    for p in PROFILES {
        let mut fans = HashSet::new();
        for f in p.fans {
            assert!(fans.insert(f.index), "{}: duplicate fan {:?}", p.id, f.index);
            assert!(f.duty_full <= f.duty_max, "{}: duty_full > duty_max", p.id);
        }
        let mut sensors = HashSet::new();
        for s in p.sensors {
            assert!(sensors.insert(s.role), "{}: duplicate sensor {:?}", p.id, s.role);
        }
        assert!(!p.hram_candidates.is_empty(), "{}: no HRAM candidates", p.id);
    }
}

#[test]
fn no_undocumented_address_collisions() {
    for p in PROFILES {
        let map = p.addr_map();
        for i in 0..map.len() {
            for j in (i + 1)..map.len() {
                let (na, aa) = &map[i];
                let (nb, ab) = &map[j];
                if aa == ab && !is_known(p.id, na, nb) {
                    panic!("{}: {na} and {nb} share {:?}", p.id, aa);
                }
            }
        }
    }
}

#[test]
fn caps_are_derived_from_ops() {
    for p in PROFILES {
        let c = p.caps("test");
        assert_eq!(c.charge.supported, !matches!(p.charge, ChargeOps::None), "{}", p.id);
        assert_eq!(c.led.animation, matches!(p.led, LedOps::PwmBreath { .. }), "{}", p.id);
        assert_eq!(c.battery_leds, p.battery_leds.is_some(), "{}", p.id);
        assert_eq!(c.battery_info, p.battery_gauge.is_some(), "{}", p.id);
        for (name, intent) in &c.charge.presets {
            assert!(p.supports_intent(intent), "{}: preset {name} not supported", p.id);
        }
    }
}

#[test]
fn advertised_presets_have_unique_names() {
    for p in PROFILES {
        let mut names = HashSet::new();
        for ps in p.charge_presets {
            assert!(names.insert(ps.name), "{}: duplicate preset {}", p.id, ps.name);
        }
    }
}
