use std::io::{Read, Write};

use crate::{IpcConnection, get_socket_name};
use bincode::{Decode, Encode};

pub struct IpcClient {
    conn: IpcConnection,
    pub daemon_version: (u8, u8),
}

impl IpcClient {
    pub fn connect() -> std::io::Result<Self> {
        let name = get_socket_name()?;
        let mut stream = interprocess::local_socket::ConnectOptions::new()
            .name(name.borrow())
            .connect_sync()?;

        // Handshake
        let handshake = [
            b'L',
            b'C',
            b'C',
            crate::IPC_PROTOCOL_VERSION[0],
            crate::IPC_PROTOCOL_VERSION[1],
        ];
        stream.write_all(&handshake)?;

        let mut resp = [0u8; 5];
        stream.read_exact(&mut resp)?;
        let daemon_major_ver = resp[3];
        let daemon_minor_ver = resp[4];

        if &resp[0..3] == b"ERR" {
            return Err(std::io::Error::new(
                std::io::ErrorKind::ConnectionRefused,
                format!(
                    "Daemon rejected connection: Version mismatch! Daemon is v{}.{}, Client is v{}.{}.{}. Please update.",
                    daemon_major_ver,
                    daemon_minor_ver,
                    crate::IPC_PROTOCOL_VERSION[0],
                    crate::IPC_PROTOCOL_VERSION[1],
                    crate::IPC_PROTOCOL_VERSION[2]
                ),
            ));
        } else if &resp[0..3] != b"OKK" {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Invalid IPC handshake",
            ));
        }

        Ok(Self {
            conn: IpcConnection { stream },
            daemon_version: (daemon_major_ver, daemon_minor_ver),
        })
    }

    pub fn request<Req: Encode, Res: Decode<()>>(&mut self, req: &Req) -> std::io::Result<Res> {
        self.conn.send(req)?;
        self.conn.recv()
    }
}
