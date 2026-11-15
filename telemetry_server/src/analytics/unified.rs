//! Flattens every storage format into one temp table.
//!
//! Three coexist in a production database:
//!   * `events` with `src='v2'`      - current JSON schema
//!   * `events` with `src='legacy'`  - bincode from old daemons, decoded by this server
//!   * `startup_events` and friends  - written by the pre-0.6 server, one table per kind
//!
//! `client_id` has the same `0x%016X` shape in all three, so a machine keeps one
//! identity across eras. That is why this is a projection and not a migration.
//!
//! Materialized rather than a view: the JSON extraction is the expensive part
//! and the report runs a dozen queries over the same rows. Nothing is written
//! to the source database.

use rusqlite::{Connection, OptionalExtension};

use super::sql;

/// Narrows what gets flattened.
///
/// Status rows are ~95% of a production database and no lookup needs them, so
/// filtering at the source keeps a lookup under a second. `client` is a LIKE
/// pattern, not an exact id: resolving a partial id would otherwise need its
/// own pass over the same data.
#[derive(Default)]
pub struct Scope {
    pub client: Option<String>,
    /// None means every kind.
    pub kinds:  Option<&'static [&'static str]>,
}

impl Scope {
    pub fn everything() -> Self {
        Self::default()
    }

    pub fn kinds(kinds: &'static [&str]) -> Self {
        Self { client: None, kinds: Some(kinds) }
    }

    /// Turns a full or partial id into a LIKE pattern, case and `0x` insensitive.
    pub fn client(needle: &str) -> Self {
        let cleaned = needle.trim().trim_start_matches("0x").trim_start_matches("0X");
        Self { client: Some(format!("%{}%", cleaned.to_uppercase())), kinds: None }
    }

    /// Narrows an existing scope to a set of kinds.
    pub fn limited_to(mut self, kinds: &'static [&str]) -> Self {
        self.kinds = Some(kinds);
        self
    }

    fn wants(&self, kind: &str) -> bool {
        self.kinds.is_none_or(|kinds| kinds.contains(&kind))
    }

    fn kind_filter(&self, column: &str) -> String {
        match self.kinds {
            Some(kinds) => format!(" AND {column} IN ({})", sql::kind_list(kinds)),
            None => String::new(),
        }
    }

    fn client_filter(&self, column: &str) -> String {
        match self.client {
            Some(_) => format!(" AND {column} LIKE ?1"),
            None => String::new(),
        }
    }
}

/// Every INSERT below is positional, so this column order must match them.
const CREATE: &str = "
CREATE TEMP TABLE ev (
    ts            TEXT,
    client_id     TEXT,
    session_id    TEXT,
    version       TEXT,
    kind          TEXT,
    src           TEXT,
    os            TEXT,
    arch          TEXT,
    cpu           TEXT,
    motherboard   TEXT,
    chip          TEXT,
    board         TEXT,
    firmware      TEXT,
    hram_offset   TEXT,
    level         TEXT,
    hits          INTEGER,
    power_profile TEXT,
    cpu_temp      INTEGER,
    sys_temp      INTEGER,
    cpu_fan       INTEGER,
    gpu_fan       INTEGER,
    soc           INTEGER,
    uptime_s      INTEGER,
    fan_mode      TEXT,
    kbd           TEXT,
    led           TEXT,
    charge        TEXT,
    message       TEXT
)";

/// `Startup.profile` is the board profile id while `Status.profile` is the
/// power profile, hence the split by kind.
///
/// `hram_offset` is formatted to hex here because the archive stored it as text
/// and the JSON formats as an integer.
const FROM_V2: &str = "
INSERT INTO ev SELECT
    ts,
    client_id,
    session_id,
    daemon_version,
    kind,
    'v2',
    body ->> '$.data.c.host.os',
    body ->> '$.data.c.host.arch',
    body ->> '$.data.c.host.cpu',
    body ->> '$.data.c.host.motherboard',
    body ->> '$.data.c.chip',
    CASE WHEN kind = 'Startup' THEN body ->> '$.data.c.profile' END,
    body ->> '$.data.c.firmware',
    CASE WHEN body ->> '$.data.c.hram_offset' IS NOT NULL
         THEN printf('0x%04X', body ->> '$.data.c.hram_offset') END,
    body ->> '$.data.c.level',
    body ->> '$.data.c.count',
    CASE WHEN kind = 'Status' THEN body ->> '$.data.c.profile' END,
    body ->> '$.data.c.cpu_temp_c',
    body ->> '$.data.c.sys_temp_c',
    body ->> '$.data.c.cpu_fan_rpm',
    body ->> '$.data.c.gpu_fan_rpm',
    body ->> '$.data.c.soc',
    body ->> '$.data.c.uptime_s',
    body ->> '$.data.c.fan_mode_cpu.t',
    body ->> '$.data.c.kbd.t',
    body ->> '$.data.c.led.t',
    body ->> '$.data.c.charge.t',
    -- One free-text column per event: the failure message for ErrorLog and
    -- Panic, the partial-restore text for Startup.
    COALESCE(body ->> '$.data.c.message', body ->> '$.data.c.restore_error')
FROM events WHERE src = 'v2'";

/// The old payloads packed sensors into fixed arrays, so they are read by
/// index here and nowhere else.
const FROM_LEGACY_JSON: &str = "
INSERT INTO ev SELECT
    ts,
    client_id,
    NULL,
    daemon_version,
    kind,
    'legacy',
    body ->> '$.data.c.os',
    NULL,
    body ->> '$.data.c.cpu',
    body ->> '$.data.c.motherboard',
    NULL,
    NULL,
    body ->> '$.data.c.firmware',
    CASE WHEN body ->> '$.data.c.offset' IS NOT NULL
         THEN printf('0x%04X', body ->> '$.data.c.offset') END,
    NULL, NULL,
    CASE WHEN kind = 'Status' THEN body ->> '$.data.c.profile' END,
    body ->> '$.data.c.temps[0]',
    body ->> '$.data.c.temps[1]',
    body ->> '$.data.c.fans[0]',
    body ->> '$.data.c.fans[1]',
    NULL, NULL, NULL, NULL, NULL, NULL,
    body ->> '$.data.c.error'
FROM events WHERE src = 'legacy'";

/// Tables the pre-0.6 server wrote, with the kind each one represents. They
/// carry no timestamp, so each joins `raw_telemetry` through `raw_id` for it.
const ARCHIVE: &[(&str, &str)] = &[
    ("startup_events", "Startup"),
    ("status_events", "Status"),
    ("panic_events", "Panic"),
    ("error_events", "ErrorLog"),
    ("unsupported_events", "Unsupported"),
];

fn archive_insert(table: &str, kind: &str, scope: &Scope) -> String {
    let (os, cpu, motherboard, chip) = match table {
        "startup_events" => ("e.os", "e.cpu", "e.motherboard", "NULL"),
        "unsupported_events" => ("NULL", "NULL", "e.motherboard", "e.chip"),
        _ => ("NULL", "NULL", "NULL", "NULL"),
    };
    let (firmware, offset) = match table {
        "startup_events" => ("e.firmware", "e.offset"),
        _ => ("NULL", "NULL"),
    };
    let (profile, sensors) = match table {
        "status_events" => ("e.profile", "e.temp_1, e.temp_2, e.fan_1, e.fan_2"),
        _ => ("NULL", "NULL, NULL, NULL, NULL"),
    };
    let message = match table {
        "panic_events" | "error_events" => "e.error_msg",
        _ => "NULL",
    };

    format!(
        "INSERT INTO ev SELECT
            r.timestamp, e.client_uuid, NULL, e.daemon_version, '{kind}', 'archive',
            {os}, NULL, {cpu}, {motherboard}, {chip}, NULL,
            {firmware}, {offset},
            NULL, NULL,
            {profile}, {sensors},
            NULL, NULL, NULL, NULL, NULL, NULL,
            {message}
        FROM {table} e JOIN raw_telemetry r ON r.id = e.raw_id
        WHERE 1{}",
        scope.client_filter("e.client_uuid")
    )
}

pub struct Sources {
    pub archive: Vec<&'static str>,
    pub rows:    i64,
    pub clients: i64,
}

pub fn build(conn: &Connection, scope: &Scope) -> rusqlite::Result<Sources> {
    conn.execute_batch("PRAGMA temp_store = MEMORY")?;
    conn.execute_batch(CREATE)?;

    let run = |sql: &str| -> rusqlite::Result<()> {
        match &scope.client {
            Some(pattern) => conn.execute(sql, [pattern])?,
            None => conn.execute(sql, [])?,
        };
        Ok(())
    };

    let mut sources = Sources { archive: Vec::new(), rows: 0, clients: 0 };

    if has_table(conn, "events")? {
        let filter = format!("{}{}", scope.kind_filter("kind"), scope.client_filter("client_id"));
        run(&format!("{FROM_V2}{filter}"))?;
        run(&format!("{FROM_LEGACY_JSON}{filter}"))?;
    }

    // Without raw_telemetry the archive rows have no timestamp, so they are
    // skipped rather than placed at a guessed time.
    if has_table(conn, "raw_telemetry")? {
        for (table, kind) in ARCHIVE {
            if scope.wants(kind) && has_table(conn, table)? {
                sources.archive.push(table);
                run(&archive_insert(table, kind, scope))?;
            }
        }
    }

    // Composite on (client_id, ts): serves the same lookups as client_id alone
    // and matches the ordering the per-client queries rely on.
    conn.execute_batch(
        "CREATE INDEX temp.ev_client_ts ON ev(client_id, ts);
         CREATE INDEX temp.ev_kind      ON ev(kind);
         CREATE INDEX temp.ev_ts        ON ev(ts)",
    )?;

    (sources.rows, sources.clients) =
        conn.query_row("SELECT COUNT(*), COUNT(DISTINCT client_id) FROM ev", [], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?;

    Ok(sources)
}

fn has_table(conn: &Connection, name: &str) -> rusqlite::Result<bool> {
    Ok(conn
        .query_row("SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1", [name], |_| Ok(()))
        .optional()?
        .is_some())
}
