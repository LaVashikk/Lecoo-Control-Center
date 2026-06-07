use bincode::config::standard;
use ipc::{TelemetryData, TelemetryDataV1, TelemetryPayload, TelemetryPayloadV1};
use log::{error, info, warn};
use rusqlite::{Connection, params};
use simplelog::SimpleLogger;
use std::{
    sync::{Arc, Mutex},
    thread,
};
use tiny_http::{Method, Response, Server};

static SERVER_ADDR: &str = "127.0.0.1:8368";
const MAX_BODY_SIZE: usize = 512 * 1024;

fn main() {
    SimpleLogger::init(log::LevelFilter::Info, simplelog::Config::default())
        .expect("Failed to initialize logger");

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
    )
    .expect("Failed to create raw table");

    conn.execute(
        "CREATE TABLE IF NOT EXISTS startup_events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            raw_id INTEGER NOT NULL,
            client_uuid TEXT NOT NULL,
            daemon_version TEXT NOT NULL,
            firmware TEXT NOT NULL,
            offset TEXT NOT NULL,
            cpu TEXT NOT NULL,
            os TEXT NOT NULL,
            motherboard TEXT,
            FOREIGN KEY(raw_id) REFERENCES raw_telemetry(id)
        )",
        [],
    )
    .expect("Failed to create startup_events table");

    // TODO: remove later
    let _ = conn.execute("ALTER TABLE startup_events ADD COLUMN motherboard TEXT", []);

    conn.execute(
        "CREATE TABLE IF NOT EXISTS status_events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            raw_id INTEGER NOT NULL,
            client_uuid TEXT NOT NULL,
            daemon_version TEXT NOT NULL,
            profile TEXT NOT NULL,
            temp_1 INTEGER NOT NULL,
            temp_2 INTEGER NOT NULL,
            fan_1 INTEGER NOT NULL,
            fan_2 INTEGER NOT NULL,
            FOREIGN KEY(raw_id) REFERENCES raw_telemetry(id)
        )",
        [],
    )
    .expect("Failed to create status_events table");

    conn.execute(
        "CREATE TABLE IF NOT EXISTS panic_events (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            raw_id INTEGER NOT NULL,
            client_uuid TEXT NOT NULL,
            daemon_version TEXT NOT NULL,
            error_msg TEXT NOT NULL,
            FOREIGN KEY(raw_id) REFERENCES raw_telemetry(id)
        )",
        [],
    )
    .expect("Failed to create panic_events table");

    let db = Arc::new(Mutex::new(conn));

    let server = Server::http(SERVER_ADDR).expect("Failed to start server");
    info!("Telemetry server listening on http://{}", SERVER_ADDR);

    for mut request in server.incoming_requests() {
        let db_clone = Arc::clone(&db);

        thread::spawn(move || {
            if request.method() == &Method::Get && request.url() == "/telemetry/health" {
                let _ = request.respond(Response::empty(200));
                return;
            }
            if request.method() != &Method::Post || request.url() != "/telemetry" {
                let _ = request.respond(Response::empty(404));
                return;
            }

            let daemon_version = request
                .headers()
                .iter()
                .find(|h| h.field.equiv("X-Daemon-Version"))
                .map(|h| h.value.as_str().to_string())
                .unwrap_or_else(|| "Unknown".to_string());

            let content_length = request.body_length().unwrap_or(0);
            if content_length > MAX_BODY_SIZE {
                let _ = request.respond(Response::empty(413));
                return;
            }

            let mut buffer = Vec::with_capacity(content_length.min(MAX_BODY_SIZE));
            if request.as_reader().read_to_end(&mut buffer).is_err() || buffer.len() > MAX_BODY_SIZE
            {
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
            let config = standard().with_limit::<{ 64 * 1024 }>();

            // Try to deserialize as V2 (new format with motherboard)
            match bincode::decode_from_slice::<TelemetryPayload, _>(&buffer, config) {
                Ok((payload, _)) => {
                    let client_uuid = format!("0x{:016X}", payload.id);

                    let insert_result = match payload.data {
                        TelemetryData::Startup { firmware, offset, cpu, os, motherboard } => {
                            let hex_offset = format!("0x{:04X}", offset);
                            lock.execute(
                                "INSERT INTO startup_events (raw_id, client_uuid, daemon_version, firmware, offset, cpu, os, motherboard) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                                params![raw_id, client_uuid, &daemon_version, firmware, hex_offset, cpu, os, motherboard],
                            )
                        }
                        TelemetryData::Status { profile, temps, fans } => {
                            lock.execute(
                                "INSERT INTO status_events (raw_id, client_uuid, daemon_version, profile, temp_1, temp_2, fan_1, fan_2) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                                params![raw_id, client_uuid, &daemon_version, format!("{:?}", profile), temps[0], temps[1], fans[0], fans[1]],
                            )
                        }
                        TelemetryData::Panic { error } => {
                            lock.execute(
                                "INSERT INTO panic_events (raw_id, client_uuid, daemon_version, error_msg) VALUES (?1, ?2, ?3, ?4)",
                                params![raw_id, client_uuid, &daemon_version, error],
                            )
                        }
                    };

                    if let Err(e) = insert_result {
                        error!(
                            "Failed to insert parsed telemetry V2 (Raw ID: {}): {}",
                            raw_id, e
                        );
                        let _ = request.respond(Response::empty(500));
                    } else {
                        let _ = request.respond(Response::empty(201));
                    }
                }
                Err(_) => {
                    // Try to deserialize as V1 (old format without motherboard)
                    match bincode::decode_from_slice::<TelemetryPayloadV1, _>(&buffer, config) {
                        Ok((payload, _)) => {
                            let client_uuid = format!("0x{:016X}", payload.id);

                            let insert_result = match payload.data {
                                TelemetryDataV1::Startup { firmware, offset, cpu, os } => {
                                    let hex_offset = format!("0x{:04X}", offset);
                                    lock.execute(
                                        "INSERT INTO startup_events (raw_id, client_uuid, daemon_version, firmware, offset, cpu, os, motherboard) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                                        params![raw_id, client_uuid, &daemon_version, firmware, hex_offset, cpu, os, None::<String>],
                                    )
                                }
                                TelemetryDataV1::Status { profile, temps, fans } => {
                                    lock.execute(
                                        "INSERT INTO status_events (raw_id, client_uuid, daemon_version, profile, temp_1, temp_2, fan_1, fan_2) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                                        params![raw_id, client_uuid, &daemon_version, format!("{:?}", profile), temps[0], temps[1], fans[0], fans[1]],
                                    )
                                }
                                TelemetryDataV1::Panic { error } => {
                                    lock.execute(
                                        "INSERT INTO panic_events (raw_id, client_uuid, daemon_version, error_msg) VALUES (?1, ?2, ?3, ?4)",
                                        params![raw_id, client_uuid, &daemon_version, error],
                                    )
                                }
                            };

                            if let Err(e) = insert_result {
                                error!(
                                    "Failed to insert parsed telemetry V1 (Raw ID: {}): {}",
                                    raw_id, e
                                );
                                let _ = request.respond(Response::empty(500));
                            } else {
                                let _ = request.respond(Response::empty(201));
                            }
                        }
                        Err(e) => {
                            // Both V1 and V2 deserialization failed, but raw data is securely stored
                            warn!(
                                "Deserialization failed for both V1 and V2. Raw ID: {}, Error: {}",
                                raw_id, e
                            );
                            let _ = request.respond(Response::empty(202));
                        }
                    }
                }
            }
        });
    }
}
