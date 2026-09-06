use interprocess::local_socket::{GenericNamespaced, ToNsName};

use crate::{DaemonClient, IpcRequest, IpcResponse, get_socket_name};

pub struct IpcClient {
    conn: DaemonClient,
}

impl IpcClient {
    pub fn connect() -> std::io::Result<Self> {
        Self::connect_with_name(get_socket_name()?)
    }

    /// Connects to a caller-supplied local socket name.
    ///
    /// This is primarily useful for isolated integrations and tests; normal
    /// clients should use [`Self::connect`] to reach the installed daemon.
    pub fn connect_to(name: &str) -> std::io::Result<Self> {
        let name = name.to_ns_name::<GenericNamespaced>().map(|name| name.into_owned())?;
        Self::connect_with_name(name)
    }

    fn connect_with_name(name: interprocess::local_socket::Name<'static>) -> std::io::Result<Self> {
        let stream = interprocess::local_socket::ConnectOptions::new()
            .name(name.borrow())
            .connect_sync()?;

        Ok(Self {
            conn: DaemonClient::connect_client_handshake(stream)?,
            // daemon_version: (0, 0) // todo: now we don't know the daemon version
        })
    }

    pub fn request(&mut self, req: &IpcRequest) -> std::io::Result<IpcResponse> {
        self.conn.send(req)?;
        self.conn.recv()
    }
}
