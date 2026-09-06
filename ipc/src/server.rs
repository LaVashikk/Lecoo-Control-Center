use std::io;

use interprocess::local_socket::{GenericNamespaced, Listener, ListenerOptions, ToNsName, prelude::*};

use crate::{DaemonWorker, get_socket_name};

pub struct IpcServer {
    listener: Listener,
}

impl IpcServer {
    /// Binds the server and sets access permissions
    pub fn bind() -> io::Result<Self> {
        Self::bind_with_name(get_socket_name()?)
    }

    /// Binds an isolated caller-supplied local socket name.
    ///
    /// The production daemon uses [`Self::bind`]; this variant supports
    /// integration tests without colliding with an installed service.
    pub fn bind_to(name: &str) -> io::Result<Self> {
        let name = name.to_ns_name::<GenericNamespaced>().map(|name| name.into_owned())?;
        Self::bind_with_name(name)
    }

    fn bind_with_name(name: interprocess::local_socket::Name<'static>) -> io::Result<Self> {
        let mut options = ListenerOptions::new().name(name);

        #[cfg(windows)]
        {
            use interprocess::os::windows::local_socket::ListenerOptionsExt;
            use interprocess::os::windows::security_descriptor::SecurityDescriptor;
            use widestring::u16cstr;

            // SDDL: SY (System) and BA (Admins) - full access (GA)
            // BU (Built-in Users) - read/write (GRGW) so that clients can connect
            let sddl = u16cstr!("D:(A;;GA;;;SY)(A;;GA;;;BA)(A;;GRGW;;;BU)");
            let sd = SecurityDescriptor::deserialize(sddl)?;
            options = options.security_descriptor(sd);
        }

        #[cfg(unix)]
        {
            use interprocess::os::unix::local_socket::ListenerOptionsExt;
            // rw-rw-rw-
            options = options.mode(0o666);
        }

        let listener = options.create_sync()?;
        Ok(Self { listener })
    }

    /// Iterator over incoming connections
    pub fn accept(&mut self) -> io::Result<DaemonWorker> {
        let stream = self.listener.accept()?;
        Ok(DaemonWorker::new(stream))
    }
}
