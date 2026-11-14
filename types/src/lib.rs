pub mod caps;
pub mod ec_types;
pub mod metrics;
pub mod settings;
pub mod telemetry;

#[cfg(test)]
mod tests;

/// `u64` <-> `"0x0123456789ABCDEF"`.
pub mod hex_u64 {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(v: &u64, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&format!("0x{v:016X}"))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
        let s = String::deserialize(d)?;
        let hex = s.strip_prefix("0x").unwrap_or(&s);
        u64::from_str_radix(hex, 16).map_err(serde::de::Error::custom)
    }
}
