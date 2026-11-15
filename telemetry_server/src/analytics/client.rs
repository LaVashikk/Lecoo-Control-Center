//! Everything the database knows about one machine.

use rusqlite::Connection;

use super::render::{self, Facts, Table};
use super::sql;

type Result = rusqlite::Result<()>;

/// Prints the full view for the machine matching `needle`, or the candidate
/// list when the pattern matches more than one.
///
/// The flatten was scoped by a LIKE pattern, so `ev` may hold several machines
/// or none.
pub fn show(conn: &Connection, needle: &str, clients: i64, limit: i64) -> Result {
    if clients == 0 {
        render::warn(&format!("\nNo client matches '{needle}'"));
        return Ok(());
    }

    if clients > 1 {
        render::warn(&format!("\n'{needle}' matches {clients} clients:"));
        Table::query(
            conn,
            &format!(
                "SELECT client_id, COUNT(*) AS events, MAX(ts) AS last_seen, {}
                 FROM ev GROUP BY client_id ORDER BY last_seen DESC LIMIT ?1",
                sql::ago("MAX(ts)")
            ),
            [limit],
            &["client", "events", "last seen", "age"],
        )?
        .print();
        render::note("\n  narrow the pattern to pick one");
        return Ok(());
    }

    let id: String = conn.query_row("SELECT client_id FROM ev LIMIT 1", [], |row| row.get(0))?;
    render::title(&format!("Client {id}"));

    identity(conn)?;
    configurations(conn)?;
    activity(conn)?;
    thermals(conn)?;
    settings(conn)?;
    failures(conn, limit)?;
    timeline(conn, limit)?;

    Ok(())
}

fn identity(conn: &Connection) -> Result {
    let (first, last, age, events, sessions, srcs): (String, String, String, i64, Option<i64>, String) = conn
        .query_row(
            &format!(
                "SELECT MIN(ts), MAX(ts), {}, COUNT(*),
                    NULLIF(COUNT(DISTINCT session_id), 0),
                    (SELECT GROUP_CONCAT(DISTINCT src) FROM ev)
             FROM ev",
                sql::ago("MAX(ts)")
            ),
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?)),
        )?;

    let mut facts = Facts::new();
    facts.add("first seen", &first);
    facts.add("last seen", format!("{last}  ({age})"));
    facts.add("events", events);
    // Sessions exist only in v2 payloads, where 0 would read as a real count.
    facts.add("sessions", sessions.map_or("-".to_string(), |n| n.to_string()));
    facts.add("reported via", srcs);
    facts.print();

    Ok(())
}

/// Distinct configurations the machine has reported, not the boot log.
fn configurations(conn: &Connection) -> Result {
    render::section("Configurations seen");

    Table::query(
        conn,
        "SELECT COUNT(*) AS boots,
                COALESCE(version, '?') AS version,
                COALESCE(board, motherboard, '?') AS board,
                COALESCE(firmware, '?') AS firmware,
                COALESCE(hram_offset, '-') AS hram,
                COALESCE(os, '?') AS os,
                COALESCE(cpu, '?') AS cpu,
                substr(MIN(ts), 1, 10) AS since,
                substr(MAX(ts), 1, 10) AS until
         FROM ev WHERE kind IN ('Startup', 'Unsupported')
         GROUP BY version, board, motherboard, firmware, hram_offset, os, cpu
         ORDER BY MAX(ts) DESC",
        [],
        &["boots", "version", "board", "firmware", "hram", "os", "cpu", "since", "until"],
    )?
    .print();

    Ok(())
}

fn activity(conn: &Connection) -> Result {
    render::section("Events by kind");

    Table::query(
        conn,
        &format!(
            "SELECT kind, src, COUNT(*) AS events, MIN(ts) AS first, MAX(ts) AS last, {}
             FROM ev GROUP BY kind, src ORDER BY events DESC",
            sql::ago("MAX(ts)")
        ),
        [],
        &["kind", "source", "events", "first", "last", "age"],
    )?
    .print();

    Ok(())
}

fn thermals(conn: &Connection) -> Result {
    let table = Table::query(
        conn,
        &format!(
            "SELECT COALESCE(power_profile, '?') AS profile,
                    COUNT(*) AS samples,
                    ROUND(AVG(cpu_temp), 1) AS avg_cpu,
                    MAX(cpu_temp) AS max_cpu,
                    ROUND(AVG(sys_temp), 1) AS avg_sys,
                    MAX(sys_temp) AS max_sys,
                    CAST(ROUND(AVG(CASE WHEN cpu_fan {SANE_FAN} THEN cpu_fan END)) AS INTEGER) AS avg_fan,
                    MAX(CASE WHEN cpu_fan {SANE_FAN} THEN cpu_fan END) AS max_fan
             FROM ev WHERE kind = 'Status' AND cpu_temp {SANE_TEMP}
             GROUP BY power_profile ORDER BY samples DESC",
            SANE_FAN = sql::SANE_FAN,
            SANE_TEMP = sql::SANE_TEMP
        ),
        [],
        &["profile", "samples", "avg cpu", "max cpu", "avg sys", "max sys", "avg fan", "max fan"],
    )?;

    if table.is_empty() {
        return Ok(());
    }

    render::section("Thermals by profile");
    table.print();

    let discarded: i64 = conn.query_row(
        &format!(
            "SELECT COUNT(*) FROM ev
             WHERE kind = 'Status'
               AND (cpu_temp IS NOT NULL AND cpu_temp NOT {})",
            sql::SANE_TEMP
        ),
        [],
        |row| row.get(0),
    )?;

    // A large count here means a dead sensor rather than a hot machine.
    if discarded > 0 {
        render::note(&format!("  {discarded} samples had an out-of-range temperature"));
    }

    Ok(())
}

struct Applied {
    ts:       String,
    fan_mode: Option<String>,
    kbd:      Option<String>,
    led:      Option<String>,
    charge:   Option<String>,
    soc:      Option<i64>,
    uptime_s: Option<i64>,
}

/// Last configuration the daemon applied. Reported by v2 clients only.
fn settings(conn: &Connection) -> Result {
    let latest = conn
        .query_row(
            "SELECT ts, fan_mode, kbd, led, charge, soc, uptime_s
             FROM ev WHERE kind = 'Status' AND fan_mode IS NOT NULL
             ORDER BY ts DESC LIMIT 1",
            [],
            |row| {
                Ok(Applied {
                    ts:       row.get(0)?,
                    fan_mode: row.get(1)?,
                    kbd:      row.get(2)?,
                    led:      row.get(3)?,
                    charge:   row.get(4)?,
                    soc:      row.get(5)?,
                    uptime_s: row.get(6)?,
                })
            },
        )
        .ok();

    let Some(applied) = latest else { return Ok(()) };
    let or_dash = |value: Option<String>| value.unwrap_or_else(|| "-".to_string());

    render::section("Applied settings");
    let mut facts = Facts::new();
    facts.add("as of", &applied.ts);
    facts.add("fan mode", or_dash(applied.fan_mode));
    facts.add("keyboard", or_dash(applied.kbd));
    facts.add("led", or_dash(applied.led));
    facts.add("charge", or_dash(applied.charge));
    facts.add("battery", applied.soc.map_or("-".to_string(), |v| format!("{v}%")));
    facts.add("daemon uptime", applied.uptime_s.map_or("-".to_string(), format_uptime));
    facts.print();

    Ok(())
}

fn format_uptime(seconds: i64) -> String {
    match seconds {
        s if s < 3600 => format!("{} min", s / 60),
        s if s < 86400 => format!("{} h {} min", s / 3600, (s % 3600) / 60),
        s => format!("{} d {} h", s / 86400, (s % 86400) / 3600),
    }
}

fn failures(conn: &Connection, limit: i64) -> Result {
    let table = Table::query(
        conn,
        &format!(
            // `hits` is the occurrence number the daemon attached to the report.
            "SELECT ts, {}, COALESCE(level, kind) AS level, COALESCE(hits, 1) AS seen,
                    COALESCE(version, '?') AS version, message
             FROM ev WHERE kind IN ({}) AND message IS NOT NULL
             ORDER BY ts DESC LIMIT ?1",
            sql::ago("ts"),
            sql::kind_list(sql::FAILURE_KINDS)
        ),
        [limit],
        &["when", "age", "level", "seen", "version", "message"],
    )?;

    render::section("Panics and errors");
    if table.is_empty() {
        println!("  {}none recorded{}", render::c(render::GREEN), render::c(render::RESET));
    } else {
        table.print();
    }

    Ok(())
}

/// Recent events other than Status samples.
fn timeline(conn: &Connection, limit: i64) -> Result {
    render::section("Recent events (excluding status samples)");

    Table::query(
        conn,
        &format!(
            // A Startup carries a board and, when the restore failed, a
            // message; both belong in the detail column.
            "SELECT ts, {}, kind, src, COALESCE(version, '?') AS version,
                    TRIM(COALESCE(board, motherboard, '') ||
                         CASE WHEN message IS NOT NULL
                              THEN '  ' || substr(message, 1, 70) ELSE '' END) AS detail
             FROM ev WHERE kind != 'Status' ORDER BY ts DESC LIMIT ?1",
            sql::ago("ts")
        ),
        [limit],
        &["when", "age", "kind", "source", "version", "detail"],
    )?
    .print();

    Ok(())
}
