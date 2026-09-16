use anyhow::{Context, Result, bail};

use libloading::{Library, Symbol};

type IsDriverOpenFn = unsafe extern "system" fn() -> u32;
type Out32Fn = unsafe extern "system" fn(port: i32, data: i32);
type Inp32Fn = unsafe extern "system" fn(port: i32) -> u8;

use windows_service::{
    service::{Service, ServiceAccess, ServiceState},
    service_manager::{ServiceManager, ServiceManagerAccess},
};

use std::{
    thread,
    time::{Duration, Instant},
};

const INPOUT_SERVICE_NAME: &str = "inpoutx64";
const ERROR_SERVICE_DOES_NOT_EXIST: i32 = 1060;
const ERROR_SERVICE_ALREADY_RUNNING: i32 = 1056;

pub struct RawPortIo {
    _lib: Library,
    #[allow(unused)]
    is_open: IsDriverOpenFn,
    out32: Out32Fn,
    inp32: Inp32Fn,
}

impl RawPortIo {
    pub fn new() -> Result<Self> {
        Self::ensure_ready()?;

        unsafe {
            let lib = Library::new("inpoutx64.dll").map_err(|_| {
                anyhow::anyhow!("Failed to load inpoutx64.dll. Ensure it's placed next to daemon.exe.")
            })?;

            let is_open_sym: Symbol<IsDriverOpenFn> = lib
                .get(b"IsInpOutDriverOpen\0")
                .context("IsInpOutDriverOpen export not found in DLL")?;
            let out32_sym: Symbol<Out32Fn> = lib.get(b"Out32\0").context("Out32 export not found in DLL")?;
            let inp32_sym: Symbol<Inp32Fn> = lib.get(b"Inp32\0").context("Inp32 export not found in DLL")?;

            let is_open = *is_open_sym;
            let out32 = *out32_sym;
            let inp32 = *inp32_sym;

            if is_open() == 0 {
                bail!("InpOut driver failed to open. Try running as Administrator.");
            }

            Ok(Self { _lib: lib, is_open, out32, inp32 })
        }
    }

    fn ensure_ready() -> Result<()> {
        let manager = ServiceManager::local_computer(None::<&str>, ServiceManagerAccess::CONNECT)?;

        let service = match manager
            .open_service(INPOUT_SERVICE_NAME, ServiceAccess::QUERY_STATUS | ServiceAccess::START)
        {
            Ok(service) => service,
            Err(error) => {
                if let windows_service::Error::Winapi(winapi_error) = &error {
                    if winapi_error.raw_os_error() == Some(ERROR_SERVICE_DOES_NOT_EXIST) {
                        return Ok(());
                    }
                }
                return Err(error.into());
            }
        };
        let status = service.query_status()?;

        match status.current_state {
            ServiceState::Running => {
                return Ok(());
            }
            ServiceState::StartPending => {
                Self::wait_until_running(&service)?;
                return Ok(());
            }
            ServiceState::Stopped => {
                Self::start_service(&service)?;
                Self::wait_until_running(&service)?;
                return Ok(());
            }
            ServiceState::StopPending => {
                Self::wait_until_stopped(&service)?;
                Self::start_service(&service)?;
                Self::wait_until_running(&service)?;
                return Ok(());
            }
            state => {
                bail!("Unexpected inpoutx64 service state: {state:?}");
            }
        }
    }

    fn wait_until_running(service: &Service) -> Result<()> {
        let timeout = Duration::from_secs(10);
        let started_at = Instant::now();

        loop {
            let status = service.query_status()?;

            match status.current_state {
                ServiceState::Running => return Ok(()),
                ServiceState::StartPending => {
                    if started_at.elapsed() >= timeout {
                        bail!("Timed out waiting for inpoutx64 to start");
                    }

                    thread::sleep(Duration::from_millis(100));
                }
                state => {
                    bail!("inpoutx64 changed to unexpected state while starting: {state:?}");
                }
            }
        }
    }

    fn wait_until_stopped(service: &Service) -> Result<()> {
        let timeout = Duration::from_secs(10);
        let started_at = Instant::now();

        loop {
            let status = service.query_status()?;

            match status.current_state {
                ServiceState::Stopped => return Ok(()),
                ServiceState::StopPending => {
                    if started_at.elapsed() >= timeout {
                        bail!("Timed out waiting for inpoutx64 to stop");
                    }

                    thread::sleep(Duration::from_millis(100));
                }
                state => {
                    bail!("inpoutx64 changed to unexpected state while stopping: {state:?}");
                }
            }
        }
    }

    fn start_service(service: &Service) -> Result<()> {
        match service.start::<&str>(&[]) {
            Ok(()) => Ok(()),
            Err(error) => {
                if let windows_service::Error::Winapi(winapi_error) = &error {
                    if winapi_error.raw_os_error() == Some(ERROR_SERVICE_ALREADY_RUNNING) {
                        return Ok(());
                    }
                }

                return Err(error.into());
            }
        }
    }

    #[inline(always)]
    pub fn outb(&self, port: u16, val: u8) -> Result<()> {
        unsafe {
            (self.out32)(port as i32, val as i32);
        }
        Ok(())
    }

    #[inline(always)]
    pub fn inb(&self, port: u16) -> Result<u8> {
        unsafe { Ok((self.inp32)(port as i32)) }
    }
}
