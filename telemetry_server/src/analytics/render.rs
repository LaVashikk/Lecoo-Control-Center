//! Terminal output helpers.

use std::io::IsTerminal;
use std::sync::OnceLock;

use rusqlite::{Connection, Params, Row, types::ValueRef};

pub const BOLD: &str = "\x1b[1m";
pub const DIM: &str = "\x1b[2m";
pub const CYAN: &str = "\x1b[96m";
pub const GREEN: &str = "\x1b[92m";
pub const RED: &str = "\x1b[91m";
pub const RESET: &str = "\x1b[0m";

pub fn c(code: &str) -> &str {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    let enabled =
        *ENABLED.get_or_init(|| std::env::var_os("NO_COLOR").is_none() && std::io::stdout().is_terminal());
    if enabled { code } else { "" }
}

pub fn section(title: &str) {
    println!("\n{}{}{}{}", c(BOLD), c(CYAN), title, c(RESET));
}

/// Header for a whole view, one level above `section`.
pub fn title(text: &str) {
    println!("\n{}{}{}", c(BOLD), text, c(RESET));
}

pub fn note(text: &str) {
    println!("{}{}{}", c(DIM), text, c(RESET));
}

pub fn warn(text: &str) {
    println!("{}{}{}", c(RED), text, c(RESET));
}

/// Key/value block, aligned on the longest key.
#[derive(Default)]
pub struct Facts(Vec<(String, String)>);

impl Facts {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, key: &str, value: impl std::fmt::Display) {
        self.0.push((key.to_string(), value.to_string()));
    }

    pub fn print(&self) {
        let width = self.0.iter().map(|(k, _)| k.chars().count()).max().unwrap_or(0);
        for (key, value) in &self.0 {
            println!("  {}{key:width$}{}  {value}", c(DIM), c(RESET));
        }
    }
}

/// Renders any column type as a display string. NULL and an unreadable value
/// become "-" rather than failing the whole query.
fn cell(row: &Row, idx: usize) -> String {
    match row.get_ref(idx) {
        Ok(ValueRef::Null) | Err(_) => "-".to_string(),
        Ok(ValueRef::Integer(v)) => v.to_string(),
        Ok(ValueRef::Real(v)) => format!("{v:.1}"),
        Ok(ValueRef::Text(v)) => String::from_utf8_lossy(v).into_owned(),
        Ok(ValueRef::Blob(v)) => format!("<{} bytes>", v.len()),
    }
}

pub struct Table {
    headers: Vec<String>,
    rows:    Vec<Vec<String>>,
}

impl Table {
    pub fn query(
        conn: &Connection,
        sql: &str,
        params: impl Params,
        headers: &[&str],
    ) -> rusqlite::Result<Self> {
        let mut stmt = conn.prepare(sql)?;
        let width = headers.len();

        let rows = stmt
            .query_map(params, |row| Ok((0..width).map(|i| cell(row, i)).collect::<Vec<_>>()))?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(Self { headers: headers.iter().map(|h| h.to_string()).collect(), rows })
    }

    /// Like `query`, with headers taken from the statement. For queries whose
    /// shape this build does not know.
    pub fn query_free(conn: &Connection, sql: &str) -> rusqlite::Result<Self> {
        let mut stmt = conn.prepare(sql)?;
        let headers: Vec<String> = stmt.column_names().iter().map(|n| n.to_string()).collect();
        let width = headers.len();

        let rows = stmt
            .query_map([], |row| Ok((0..width).map(|i| cell(row, i)).collect::<Vec<_>>()))?
            .collect::<rusqlite::Result<Vec<_>>>()?;

        Ok(Self { headers, rows })
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Appends a share-of-total column computed from an existing numeric one.
    pub fn with_share(mut self, column: usize, label: &str) -> Self {
        let total: f64 = self.rows.iter().filter_map(|r| r[column].parse::<f64>().ok()).sum();

        self.headers.push(label.to_string());
        for row in &mut self.rows {
            let value = row[column].parse::<f64>().unwrap_or(0.0);
            row.push(if total > 0.0 { format!("{:.1}%", value / total * 100.0) } else { "-".to_string() });
        }
        self
    }

    pub fn print(&self) {
        if self.rows.is_empty() {
            note("  (no data)");
            return;
        }

        let widths: Vec<usize> = self
            .headers
            .iter()
            .enumerate()
            .map(|(i, h)| {
                self.rows
                    .iter()
                    .map(|r| r[i].chars().count())
                    .max()
                    .unwrap_or(0)
                    .max(h.chars().count())
            })
            .collect();

        // Right-aligned only if every cell in the column parses as a number.
        let numeric: Vec<bool> = (0..self.headers.len())
            .map(|i| {
                self.rows.iter().all(|r| {
                    let v = r[i].trim_end_matches('%');
                    v == "-" || v.parse::<f64>().is_ok()
                })
            })
            .collect();

        let header: Vec<String> =
            self.headers.iter().enumerate().map(|(i, h)| pad(h, widths[i], numeric[i])).collect();
        println!("  {}{}{}", c(BOLD), header.join("  ").trim_end(), c(RESET));

        for row in &self.rows {
            let line: Vec<String> =
                row.iter().enumerate().map(|(i, v)| pad(v, widths[i], numeric[i])).collect();
            println!("  {}", line.join("  ").trim_end());
        }
    }
}

fn pad(value: &str, width: usize, right: bool) -> String {
    let len = value.chars().count();
    let fill = " ".repeat(width.saturating_sub(len));
    if right { format!("{fill}{value}") } else { format!("{value}{fill}") }
}
