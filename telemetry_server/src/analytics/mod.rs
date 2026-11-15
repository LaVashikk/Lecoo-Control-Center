//! Reporting and incident lookup over the telemetry database.
//!
//! Each command flattens the storage formats into one temp table and queries
//! it. How much gets flattened depends on the command.

use clap::{Parser, Subcommand};
use rusqlite::Connection;

mod client;
mod lookup;
mod render;
mod report;
mod sql;
mod unified;

pub use report::Section;

/// Long help for the `sql` subcommand.
const EV_SCHEMA: &str = "\
Runs a read-only query against `ev`, a temp table built for this run only.

Three storage formats are projected onto one row shape, so a query does not
have to know whether an event came from the current daemon, an old bincode
one, or the tables the pre-0.6 server wrote. A column a format never carried
is NULL, so mixing eras usually needs COALESCE.

Always present
  ts           UTC text, compare with datetime('now', '-7 days')
  client_id    stable per machine, format 0x%016X
  version      daemon version
  kind         Startup | Status | Unsupported | ErrorLog | Panic
  src          v2 | legacy | archive
  session_id   one daemon process, v2 only

Startup and Unsupported
  os, cpu, motherboard, firmware, hram_offset
  arch, chip   v2 only
  board        resolved profile id such as n155, v2 only
               motherboard is the DMI string instead, and the old daemon
               wrote it as 'product (board)', so the two do not compare

Status
  power_profile, cpu_temp, sys_temp, cpu_fan, gpu_fan
  soc, uptime_s, fan_mode, kbd, led, charge    v2 only
  Readings can be junk: a dead EC register reads 0xFF and the RPM counter
  overflows, so filter cpu_temp BETWEEN 1 AND 110 for averages.

ErrorLog and Panic
  message      also holds the partial-restore text on a Startup row
  level        WARN or ERROR, v2 only
  hits         occurrence number within the session, v2 only. Repeats are
               sampled at 1, 2, 4, 8..., so hits jumps and is not a count
               of delivered events.

Examples
  sql \"SELECT board, COUNT(DISTINCT client_id) FROM ev
        WHERE kind='Startup' GROUP BY board ORDER BY 2 DESC\"

  sql \"SELECT ts, level, hits, message FROM ev
        WHERE client_id LIKE '%E1DE%' AND message IS NOT NULL
        ORDER BY ts DESC\"

The source tables are reachable as well: events holds the raw JSON body,
raw_telemetry holds payloads that never decoded.";

#[derive(Parser)]
#[command(
    name = "telemetry_stats",
    version,
    about = "Fleet report and incident lookup over the telemetry database",
    long_about = None,
    disable_help_subcommand = true
)]
pub struct Cli {
    /// Path to the telemetry database
    #[arg(long, global = true, default_value = "telemetry.db", value_name = "PATH")]
    db: String,

    /// Window that counts as active, in days
    #[arg(long, short, global = true, default_value_t = 7, value_name = "N")]
    days: i64,

    /// Rows per table
    #[arg(long, short = 'n', global = true, default_value_t = 20, value_name = "N")]
    limit: i64,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Fleet-wide report. Runs by default when no command is given
    Report {
        /// Print only these sections
        #[arg(long, value_enum, value_delimiter = ',', value_name = "SECTION")]
        only: Vec<Section>,
    },

    /// Everything known about one machine: id, full or partial
    Client { id: String },

    /// Find machines by id, board, OS, CPU, chip or version
    Find { pattern: String },

    /// Panics and error logs across the fleet
    Errors {
        /// Restrict to one machine
        #[arg(long, value_name = "ID")]
        client: Option<String>,
        /// Limit to the active window instead of all time
        #[arg(long)]
        recent: bool,
    },

    /// Run a read-only query against the flattened event table
    #[command(long_about = EV_SCHEMA)]
    Sql { query: String },
}

pub struct Opts {
    pub days:  i64,
    pub limit: i64,
}

pub fn run(cli: Cli) -> rusqlite::Result<()> {
    let started = std::time::Instant::now();

    // Connection::open would create the file, hiding a typo as an empty report.
    if !std::path::Path::new(&cli.db).exists() {
        eprintln!("No such database: {}", cli.db);
        std::process::exit(1);
    }

    // Read-write because SQLite cannot open a WAL database read-only. Only the
    // temp schema is written.
    let conn = Connection::open(&cli.db)?;
    let opts = Opts { days: cli.days, limit: cli.limit };
    let command = cli.command.unwrap_or(Command::Report { only: Vec::new() });

    // Only flatten what the command can actually look at.
    let scope = match &command {
        Command::Report { .. } | Command::Sql { .. } => unified::Scope::everything(),
        Command::Client { id } => unified::Scope::client(id),
        Command::Find { .. } => unified::Scope::kinds(sql::IDENTITY_KINDS),
        Command::Errors { client: Some(id), .. } => unified::Scope::client(id).limited_to(sql::FAILURE_KINDS),
        Command::Errors { client: None, .. } => unified::Scope::kinds(sql::FAILURE_KINDS),
    };

    let sources = unified::build(&conn, &scope)?;
    let flattened = started.elapsed();

    // A scoped flatten can be empty just because nothing matched; those
    // commands report that themselves.
    if sources.rows == 0 && matches!(command, Command::Report { .. } | Command::Sql { .. }) {
        println!("No telemetry found in {}", cli.db);
        return Ok(());
    }

    match &command {
        Command::Report { only } => report::run(&conn, &opts, &sources, only)?,
        Command::Client { id } => client::show(&conn, id, sources.clients, opts.limit)?,
        Command::Find { pattern } => lookup::find(&conn, pattern, opts.limit)?,
        Command::Errors { recent, .. } => lookup::errors(&conn, recent.then_some(opts.days), opts.limit)?,
        Command::Sql { query } => lookup::query(&conn, query)?,
    }

    println!(
        "\n{}{} events flattened in {:.1}s, answered in {:.1}s{}",
        render::c(render::DIM),
        sources.rows,
        flattened.as_secs_f64(),
        (started.elapsed() - flattened).as_secs_f64(),
        render::c(render::RESET)
    );

    Ok(())
}
