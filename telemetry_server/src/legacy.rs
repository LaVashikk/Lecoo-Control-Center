//! `/telemetry`: the bincode wire format of daemons up to 0.5.1-beta.
//!
//! Kept alive until those clients are gone. The structs are copies frozen at
//! that release, not shared types: the point is to decode bytes nobody will
//! ever emit again, so they must not follow the model as it evolves.
//!
//! Decoded events are stored under `src = 'legacy'` and deliberately NOT
//! remapped onto the current schema. The old payloads lack almost every field
//! it has, and filling those with defaults would quietly poison aggregates.

use bincode::Decode;
use log::warn;
use rusqlite::Connection;
use serde::{Serialize, Serializer};

use crate::db;

#[derive(Debug, Decode, Serialize)]
enum PowerProfile {
    Silent,
    Default,
    Performance,
}

/// Format as of 0.5.1-beta.
#[derive(Debug, Decode, Serialize)]
#[serde(tag = "t", content = "c")]
enum DataV2 {
    Startup {
        firmware:    String,
        offset:      u16,
        cpu:         String,
        os:          String,
        motherboard: String,
    },
    Status {
        profile: PowerProfile,
        temps:   [u32; 2],
        fans:    [u32; 2],
    },
    Panic {
        error: String,
    },
}

/// The same, before `motherboard` was added. `Status` and `Panic` are laid out
/// identically in both, so only `Startup` really needs this fallback.
#[derive(Debug, Decode, Serialize)]
#[serde(tag = "t", content = "c")]
enum DataV1 {
    Startup {
        firmware: String,
        offset:   u16,
        cpu:      String,
        os:       String,
    },
    Status {
        profile: PowerProfile,
        temps:   [u32; 2],
        fans:    [u32; 2],
    },
    Panic {
        error: String,
    },
}

#[derive(Debug, Decode, Serialize)]
struct PayloadV2 {
    #[serde(serialize_with = "hex_id")]
    id:   u64,
    data: DataV2,
}

#[derive(Debug, Decode, Serialize)]
struct PayloadV1 {
    #[serde(serialize_with = "hex_id")]
    id:   u64,
    data: DataV1,
}

/// Matches how the current daemon writes the id, so `client_id` lines up
/// across legacy and v2 rows and a machine stays one machine in queries.
fn hex_id<S: Serializer>(value: &u64, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&format!("0x{value:016X}"))
}

pub fn store(conn: &Connection, version: &str, body: &[u8]) -> u16 {
    let config = bincode::config::standard().with_limit::<{ 64 * 1024 }>();

    // V2 first. bincode ignores trailing bytes, so a V2 payload would also
    // decode as V1 with the motherboard string left over, while a V1 payload
    // runs out of data when read as V2. Only this order distinguishes them.
    if let Ok((payload, _)) = bincode::decode_from_slice::<PayloadV2, _>(body, config) {
        return insert(conn, version, &payload);
    }
    if let Ok((payload, _)) = bincode::decode_from_slice::<PayloadV1, _>(body, config) {
        return insert(conn, version, &payload);
    }

    warn!("Legacy payload decoded as neither V2 nor V1");
    db::dead_letter(conn, version, body)
}

fn insert<T: Serialize>(conn: &Connection, version: &str, payload: &T) -> u16 {
    let Ok(body) = serde_json::to_string(payload) else {
        return 500;
    };

    if db::insert_event(conn, version, "legacy", &body) { 201 } else { 500 }
}
