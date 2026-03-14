use std::fs::{File, OpenOptions};
use std::os::unix::fs::FileExt;
use anyhow::{Context, Result, bail};

pub struct RawPortIo {
    file: File,
}

impl RawPortIo {
    pub fn new() -> Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open("/dev/port")      // todo: Race condition with ACPI/OS itself. ec_sys can be a better solution!
            .context("Failed to open /dev/port. Are you root?")?;
        Ok(Self { file })
    }

    #[inline(always)]
    pub fn outb(&self, port: u16, val: u8) -> Result<()> {
        self.file.write_at(&[val], port as u64)
            .with_context(|| format!("outb failed at port {:#X}", port))?;
        Ok(())
    }

    #[inline(always)]
    pub fn inb(&self, port: u16) -> Result<u8> {
        let mut buf = [0u8; 1];
        self.file.read_at(&mut buf, port as u64)
            .with_context(|| format!("inb failed at port {:#X}", port))?;
        Ok(buf[0])
    }
}
