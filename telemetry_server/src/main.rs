use bincode::config::standard;
use ipc::{TelemetryData, TelemetryPayload};
use log::{error, info, warn};
use rusqlite::{params, Connection};
use simplelog::SimpleLogger;
use std::{
    sync::{Arc, Mutex},
    thread,
};
use tiny_http::{Method, Response, Server};

static SERVER_ADDR: &str = "127.0.0.1:8368";
const MAX_BODY_SIZE: usize = 512 * 1024;

fn main() {
    SimpleLogger::init(log::LevelFilter::Info, simplelog::Config::default()).expect("Failed to initialize logger");

    let conn = Connection::open("telemetry.db").expect("Failed to open DB");
    conn.execute_batch("PRAGMA journal_mode = WAL;").unwrap();

    // Raw Event Sourcing pattern: store raw payloads immediately to prevent
    // data loss in case of deserialization failures or future schema changes
    conn.execute(
        "CREATE TABLE IF NOT EXISTS raw_telemetry (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp DATETIME DEFAULT CURRENT_TIMESTAMP,
            daemon_version TEXT NOT NULL,
            raw_data BLOB NOT NULL
        )",
        [],
    ).expect("Failed to create raw table");

    conn.execute(
        "CREATE TABLE IF NOT EXISTS parsed_telemetry (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            raw_id INTEGER NOT NULL,
            client_uuid TEXT NOT NULL,
            daemon_version TEXT NOT NULL,
            event_type TEXT NOT NULL,
            firmware TEXT,
            offset TEXT,
            profile TEXT,
            temp_1 INTEGER,
            temp_2 INTEGER,
            fan_1 INTEGER,
            fan_2 INTEGER,
            error_msg TEXT,
            FOREIGN KEY(raw_id) REFERENCES raw_telemetry(id)
        )",
        [],
    ).expect("Failed to create parsed table");

    let db = Arc::new(Mutex::new(conn));

    let server = Server::http(SERVER_ADDR).expect("Failed to start server");
    info!("Telemetry server listening on http://{}", SERVER_ADDR);

    for mut request in server.incoming_requests() {
        let db_clone = Arc::clone(&db);

        thread::spawn(move || {
            if request.method() != &Method::Post || request.url() != "/telemetry" {
                let _ = request.respond(Response::empty(404));
                return;
            }

            let daemon_version = request.headers().iter()
                .find(|h| h.field.equiv("X-Daemon-Version"))
                .map(|h| h.value.as_str().to_string())
                .unwrap_or_else(|| "Unknown".to_string());

            let content_length = request.body_length().unwrap_or(0);
            if content_length > MAX_BODY_SIZE {
                let _ = request.respond(Response::empty(413));
                return;
            }

            let mut buffer = Vec::with_capacity(content_length.min(MAX_BODY_SIZE));
            if request.as_reader().read_to_end(&mut buffer).is_err() || buffer.len() > MAX_BODY_SIZE {
                warn!("Failed to read request body");
                let _ = request.respond(Response::empty(400));
                return;
            }

            // Acquire DB lock for the entire transaction block
            let lock = db_clone.lock().unwrap();

            if let Err(e) = lock.execute(
                "INSERT INTO raw_telemetry (daemon_version, raw_data) VALUES (?1, ?2)",
                params![&daemon_version, &buffer],
            ) {
                error!("Failed to insert raw telemetry: {}", e);
                let _ = request.respond(Response::empty(500));
                return;
            }

            let raw_id = lock.last_insert_rowid();
            let config = standard()
                .with_limit::<{ 64 * 1024 }>();

            match bincode::decode_from_slice::<TelemetryPayload, _>(&buffer, config) {
                Ok((payload, _)) => {
                    let (ev_type, fw, off, prof, t1, t2, f1, f2, err_msg) = match payload.data {
                        TelemetryData::Startup { firmware, offset } => {
                            let hex_offset = format!("0x{:04X}", offset);
                            ("Startup", Some(firmware), Some(hex_offset), None, None, None, None, None, None)
                        }
                        TelemetryData::Status { profile, temps, fans } => {
                            ("Status", None, None, Some(format!("{:?}", profile)), Some(temps[0]), Some(temps[1]), Some(fans[0]), Some(fans[1]), None)
                        }
                        TelemetryData::Panic { error } => {
                            ("Panic", None, None, None, None, None, None, None, Some(error))
                        }
                    };

                    if let Err(e) = lock.execute(
                        "INSERT INTO parsed_telemetry (
                            raw_id, client_uuid, daemon_version, event_type, firmware, offset, profile, temp_1, temp_2, fan_1, fan_2, error_msg
                        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                        params![
                            raw_id, format!("0x{:016X}", payload.id), &daemon_version, ev_type, fw, off, prof, t1, t2, f1, f2, err_msg
                        ],
                    ) {
                        error!("Failed to insert parsed telemetry (Raw ID: {}): {}", raw_id, e);
                        let _ = request.respond(Response::empty(500));
                    } else {
                        let _ = request.respond(Response::empty(201));
                    }
                }
                Err(e) => {
                    // Deserialization failed, but raw data is securely stored
                    warn!("Deserialization failed. Raw ID: {}, Error: {}", raw_id, e);
                    let _ = request.respond(Response::empty(202));
                }
            }
        });
    }
}
