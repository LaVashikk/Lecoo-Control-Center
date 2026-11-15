//! The fleet report. Every section is one query over the unified `ev` table.
//!
//! Sections count distinct clients, not events: one machine sending a Status
//! every five minutes would otherwise dominate every ranking.

use clap::ValueEnum;
use rusqlite::Connection;

use super::Opts;
use super::render::{self, Facts, Table};
use super::sql;
use super::unified::Sources;

type Result = rusqlite::Result<()>;

/// Report sections, selectable with `--only`.
#[derive(Copy, Clone, PartialEq, Eq, ValueEnum)]
pub enum Section {
    Overview,
    Activity,
    Versions,
    Platforms,
    Boards,
    Thermals,
    Features,
    Errors,
    Clients,
    Health,
}

pub fn run(conn: &Connection, opts: &Opts, sources: &Sources, only: &[Section]) -> Result {
    let wanted = |section: Section| only.is_empty() || only.contains(&section);

    if wanted(Section::Overview) {
        overview(conn, sources)?;
    }
    if wanted(Section::Activity) {
        activity(conn, opts)?;
    }
    if wanted(Section::Versions) {
        versions(conn, opts)?;
    }
    if wanted(Section::Platforms) {
        platforms(conn, opts)?;
    }
    if wanted(Section::Boards) {
        boards(conn)?;
    }
    if wanted(Section::Thermals) {
        thermals(conn)?;
    }
    if wanted(Section::Features) {
        features(conn)?;
    }
    if wanted(Section::Errors) {
        failures(conn, opts)?;
    }
    if wanted(Section::Clients) {
        clients(conn, opts)?;
    }
    if wanted(Section::Health) {
        health(conn, sources)?;
    }

    Ok(())
}

fn scalar(conn: &Connection, sql: &str) -> rusqlite::Result<i64> {
    conn.query_row(sql, [], |row| row.get(0))
}

fn overview(conn: &Connection, sources: &Sources) -> Result {
    render::section("Overview");

    let span: (Option<String>, Option<String>) =
        conn.query_row("SELECT MIN(ts), MAX(ts) FROM ev", [], |r| Ok((r.get(0)?, r.get(1)?)))?;

    let mut facts = Facts::new();
    facts.add("events", sources.rows);
    facts.add("clients", sources.clients);
    facts
        .add("span", format!("{} .. {}", span.0.as_deref().unwrap_or("-"), span.1.as_deref().unwrap_or("-")));
    facts.print();

    Table::query(
        conn,
        "SELECT src, COUNT(*) AS events, COUNT(DISTINCT client_id) AS clients,
                MIN(ts) AS first, MAX(ts) AS last
         FROM ev GROUP BY src ORDER BY events DESC",
        [],
        &["source", "events", "clients", "first", "last"],
    )?
    .print();

    Ok(())
}

/// Active means the client sent anything within the window; 30 days of
/// silence counts as churn.
fn activity(conn: &Connection, opts: &Opts) -> Result {
    render::section("Activity");

    let active = |days: i64| -> rusqlite::Result<i64> {
        conn.query_row(
            "SELECT COUNT(DISTINCT client_id) FROM ev WHERE ts >= datetime('now', ?1)",
            [format!("-{days} days")],
            |row| row.get(0),
        )
    };

    let total = scalar(conn, "SELECT COUNT(DISTINCT client_id) FROM ev")?;
    let (day, week, month) = (active(1)?, active(7)?, active(30)?);

    let share = |value: i64| {
        if total > 0 { format!("{:.1}%", value as f64 / total as f64 * 100.0) } else { "-".into() }
    };

    println!("  daily     {day:>6}  {}", share(day));
    println!("  weekly    {week:>6}  {}", share(week));
    println!("  monthly   {month:>6}  {}", share(month));
    println!("  all time  {total:>6}");

    let new = conn.query_row(
        "SELECT COUNT(*) FROM (SELECT client_id FROM ev GROUP BY client_id
                               HAVING MIN(ts) >= datetime('now', ?1))",
        [format!("-{} days", opts.days)],
        |row| row.get::<_, i64>(0),
    )?;
    let churned = scalar(
        conn,
        "SELECT COUNT(*) FROM (SELECT client_id FROM ev GROUP BY client_id
                               HAVING MAX(ts) < datetime('now','-30 days'))",
    )?;

    println!("\n  new in last {} days  {new}", opts.days);
    println!("  silent over 30 days  {churned}");

    Ok(())
}

fn versions(conn: &Connection, opts: &Opts) -> Result {
    render::section(&format!("Versions in use (last {} days)", opts.days));

    Table::query(
        conn,
        "SELECT version, COUNT(DISTINCT client_id) AS clients, COUNT(*) AS events,
                MAX(ts) AS last_seen
         FROM ev WHERE ts >= datetime('now', ?1)
         GROUP BY version ORDER BY clients DESC, events DESC",
        [format!("-{} days", opts.days)],
        &["version", "clients", "events", "last seen"],
    )?
    .with_share(1, "share")
    .print();

    Ok(())
}

fn platforms(conn: &Connection, opts: &Opts) -> Result {
    let window = [format!("-{} days", opts.days)];

    render::section(&format!("Operating systems (last {} days)", opts.days));
    Table::query(
        conn,
        "SELECT os, COUNT(DISTINCT client_id) AS clients FROM ev
         WHERE os IS NOT NULL AND ts >= datetime('now', ?1)
         GROUP BY os ORDER BY clients DESC LIMIT 15",
        window.clone(),
        &["os", "clients"],
    )?
    .with_share(1, "share")
    .print();

    render::section(&format!("CPUs (last {} days)", opts.days));
    Table::query(
        conn,
        "SELECT cpu, COUNT(DISTINCT client_id) AS clients FROM ev
         WHERE cpu IS NOT NULL AND ts >= datetime('now', ?1)
         GROUP BY cpu ORDER BY clients DESC LIMIT 15",
        window,
        &["cpu", "clients"],
    )?
    .print();

    Ok(())
}

fn boards(conn: &Connection) -> Result {
    // Grouped by source too: only v2 reports the profile id, and the old daemon
    // wrote DMI as "product (board)", so one machine can appear under two
    // labels. The strings do not match, so they are not merged.
    render::section("Supported boards");
    Table::query(
        conn,
        "SELECT src, COALESCE(board, '-') AS profile, COALESCE(motherboard, '?') AS dmi,
                COUNT(DISTINCT client_id) AS clients, MAX(ts) AS last_seen
         FROM ev WHERE kind = 'Startup'
         GROUP BY src, profile, dmi ORDER BY clients DESC LIMIT 20",
        [],
        &["source", "profile", "dmi", "clients", "last seen"],
    )?
    .print();

    render::section("Unsupported boards");
    let unsupported = Table::query(
        conn,
        "SELECT motherboard, COALESCE(chip, '?') AS chip,
                COUNT(DISTINCT client_id) AS clients, MAX(ts) AS last_seen
         FROM ev WHERE kind = 'Unsupported'
         GROUP BY motherboard, chip ORDER BY clients DESC LIMIT 20",
        [],
        &["dmi", "chip", "clients", "last seen"],
    )?;

    if unsupported.is_empty() {
        render::note("  none reported");
    } else {
        unsupported.print();
    }

    Ok(())
}

/// Status rows carry no board, so each is attributed to the board its client
/// reported at startup.
fn thermals(conn: &Connection) -> Result {
    render::section("Thermals by board and profile");

    Table::query(
        conn,
        &format!(
            // Profile id when the client sends one, DMI string otherwise:
            // keying on the profile alone buckets every pre-v2 machine
            // under '?', which is most of the history.
            "WITH client_board AS (
                 SELECT client_id, COALESCE(board, motherboard) AS known_board FROM ev
                 WHERE kind = 'Startup' AND COALESCE(board, motherboard) IS NOT NULL
                 GROUP BY client_id
             )
             SELECT COALESCE(b.known_board, '?') AS board,
                    COALESCE(s.power_profile, '?') AS profile,
                    COUNT(DISTINCT s.client_id) AS clients,
                    COUNT(*) AS samples,
                    ROUND(AVG(s.cpu_temp), 1) AS avg_cpu,
                    MAX(s.cpu_temp) AS max_cpu,
                    ROUND(AVG(s.sys_temp), 1) AS avg_sys,
                    CAST(ROUND(AVG(CASE WHEN s.cpu_fan {sane_fan} THEN s.cpu_fan END)) AS INTEGER) AS avg_fan,
                    MAX(CASE WHEN s.cpu_fan {sane_fan} THEN s.cpu_fan END) AS max_fan
             FROM ev s LEFT JOIN client_board b ON b.client_id = s.client_id
             WHERE s.kind = 'Status' AND s.cpu_temp {sane_temp}
             -- Grouped by the expressions, not the aliases: `board` also names a
             -- column of `ev` and SQLite binds to that one, collapsing every
             -- board into a single bucket.
             GROUP BY b.known_board, s.power_profile
             ORDER BY samples DESC LIMIT 20",
            sane_fan = sql::SANE_FAN,
            sane_temp = sql::SANE_TEMP
        ),
        [],
        &["board", "profile", "clients", "samples", "avg cpu", "max cpu", "avg sys", "avg fan", "max fan"],
    )?
    .print();

    let (bad_temp, bad_fan): (i64, i64) = conn.query_row(
        &format!(
            "SELECT SUM(cpu_temp IS NOT NULL AND cpu_temp NOT {sane_temp}),
                    SUM(cpu_fan IS NOT NULL AND cpu_fan NOT {sane_fan})
             FROM ev WHERE kind = 'Status'",
            sane_temp = sql::SANE_TEMP,
            sane_fan = sql::SANE_FAN
        ),
        [],
        |row| Ok((row.get(0).unwrap_or(0), row.get(1).unwrap_or(0))),
    )?;

    if bad_temp > 0 || bad_fan > 0 {
        render::note(&format!(
            "  discarded {bad_temp} out-of-range temperatures and {bad_fan} fan readings (dead sensor or counter overflow)"
        ));
    }

    Ok(())
}

/// Current setting per client, taken from its most recent Status. Counting
/// every sample would place one client under every mode it ever passed through.
///
/// Power profile is present in every era; the rest needs a v2 daemon.
fn features(conn: &Connection) -> Result {
    render::section("Feature usage (current setting per client)");

    let table = Table::query(
        conn,
        // Bare columns come from the row that produced the single max(), which
        // walks the (client_id, ts) index; ROW_NUMBER() sorts the whole table
        // instead, 12s against 0.4s on 2.4M rows.
        //
        // A second min()/max() here would make the bare columns arbitrary, and
        // without MATERIALIZED the CTE is recomputed per UNION branch.
        "WITH current AS MATERIALIZED (
             SELECT client_id, fan_mode, kbd, led, charge, power_profile, max(ts)
             FROM ev WHERE kind = 'Status' GROUP BY client_id
         )
         SELECT 'fan mode' AS feature, fan_mode AS value, COUNT(*) AS clients
             FROM current WHERE fan_mode IS NOT NULL GROUP BY fan_mode
         UNION ALL
         SELECT 'keyboard', kbd, COUNT(*) FROM current WHERE kbd IS NOT NULL GROUP BY kbd
         UNION ALL
         SELECT 'led', led, COUNT(*) FROM current WHERE led IS NOT NULL GROUP BY led
         UNION ALL
         SELECT 'charge', charge, COUNT(*) FROM current WHERE charge IS NOT NULL GROUP BY charge
         UNION ALL
         SELECT 'power profile', power_profile, COUNT(*)
             FROM current WHERE power_profile IS NOT NULL GROUP BY power_profile
         ORDER BY 1, 3 DESC",
        [],
        &["feature", "value", "clients"],
    )?;

    if table.is_empty() {
        render::note("  no status events yet");
    } else {
        table.print();
        render::note("  fan mode, keyboard, led and charge are reported by v2 daemons only");
    }

    Ok(())
}

/// Grouped by message and ranked by how many machines hit it, so one client in
/// a crash loop does not become the top entry.
fn failures(conn: &Connection, opts: &Opts) -> Result {
    render::section("Panics and errors");

    let table = Table::query(
        conn,
        "SELECT kind, COUNT(DISTINCT client_id) AS clients, COUNT(*) AS hits,
                MAX(ts) AS last_seen, substr(message, 1, 90) AS message
         FROM ev WHERE kind IN ('Panic', 'ErrorLog') AND message IS NOT NULL
         GROUP BY kind, message ORDER BY clients DESC, hits DESC LIMIT ?1",
        [opts.limit],
        &["kind", "clients", "hits", "last seen", "message"],
    )?;

    if table.is_empty() {
        println!("  {}none recorded{}", render::c(render::GREEN), render::c(render::RESET));
    } else {
        table.print();
    }

    Ok(())
}

/// One row per machine.
fn clients(conn: &Connection, opts: &Opts) -> Result {
    render::section(&format!("Clients (most recent {})", opts.limit));

    Table::query(
        conn,
        // `latest` and `hw` use the same bare-column rule as `features`, so each
        // keeps exactly one max(). `agg` is separate because it needs both MIN
        // and MAX, which would make bare columns arbitrary.
        "WITH agg AS (
             SELECT client_id,
                    MIN(ts) AS first_seen,
                    MAX(ts) AS last_seen,
                    -- Archive and legacy events carry no session, where 0 would
                    -- read as a real count.
                    NULLIF(COUNT(DISTINCT session_id), 0) AS sessions,
                    COUNT(*) AS events
             FROM ev GROUP BY client_id
         ),
         latest AS (
             SELECT client_id, version, max(ts) FROM ev GROUP BY client_id
         ),
         hw AS (
             SELECT client_id, COALESCE(board, motherboard) AS board, os, max(ts)
             FROM ev WHERE kind IN ('Startup', 'Unsupported') GROUP BY client_id
         )
         -- Full id, not a prefix: ids share long runs of leading zeros.
         SELECT a.client_id AS client,
                CASE WHEN a.last_seen >= datetime('now', ?1) THEN 'yes' ELSE '' END AS active,
                l.version,
                COALESCE(hw.board, '?') AS board,
                COALESCE(hw.os, '?') AS os,
                a.sessions,
                a.events,
                substr(a.first_seen, 1, 10) AS first_seen,
                a.last_seen
         FROM agg a
         LEFT JOIN latest l ON l.client_id = a.client_id
         LEFT JOIN hw ON hw.client_id = a.client_id
         ORDER BY a.last_seen DESC LIMIT ?2",
        rusqlite::params![format!("-{} days", opts.days), opts.limit],
        &["client", "active", "version", "board", "os", "sessions", "events", "first seen", "last seen"],
    )?
    .print();

    Ok(())
}

/// Event kinds this build does not recognise, and payloads that never decoded.
/// An unknown kind means a newer daemon is already in the field.
fn health(conn: &Connection, sources: &Sources) -> Result {
    render::section("Data health");

    let known = "('Startup','Status','Unsupported','ErrorLog','Panic')";
    let unknown = Table::query(
        conn,
        &format!(
            "SELECT kind, src, COUNT(*) AS events, COUNT(DISTINCT client_id) AS clients
             FROM ev WHERE kind NOT IN {known} GROUP BY kind, src ORDER BY events DESC"
        ),
        [],
        &["unknown kind", "source", "events", "clients"],
    )?;

    if unknown.is_empty() {
        println!("  every event kind is recognised");
    } else {
        println!(
            "  {}events this build does not know about:{}",
            render::c(render::RED),
            render::c(render::RESET)
        );
        unknown.print();
    }

    // The pre-0.6 server mirrored every event into raw_telemetry, so only rows
    // without a parsed sibling are actual decode failures.
    let siblings: Vec<String> = sources.archive.iter().map(|t| format!("SELECT raw_id FROM {t}")).collect();

    let dead: i64 = if siblings.is_empty() {
        scalar(conn, "SELECT COUNT(*) FROM raw_telemetry").unwrap_or(0)
    } else {
        scalar(
            conn,
            &format!("SELECT COUNT(*) FROM raw_telemetry WHERE id NOT IN ({})", siblings.join(" UNION ALL ")),
        )
        .unwrap_or(0)
    };

    println!("  undecodable payloads kept raw: {dead}");
    Ok(())
}
