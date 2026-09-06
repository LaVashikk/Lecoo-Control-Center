use anyhow::{Result, bail};
use lecoo_types::caps::SensorRole;
use std::sync::Mutex;

#[cfg(target_os = "linux")]
mod sys_linux;
#[cfg(target_os = "linux")]
pub use sys_linux::RawPortIo;

#[cfg(target_os = "windows")]
mod sys_windows;
#[cfg(target_os = "windows")]
pub use sys_windows::RawPortIo;

mod hw;
pub use hw::*;
mod profile;
pub use profile::*;

// DEFAULT REGS FOR ITE IT5570/IT8987 CHIPS
pub(crate) const REG_CHIP_ID1: u16 = 0x2000;
pub(crate) const REG_CHIP_ID2: u16 = 0x2001;
pub(crate) const REG_CHIP_VER: u16 = 0x2002;
const MIN_PLAUSIBLE_TEMPERATURE_C: u8 = 0x10;
const MAX_PLAUSIBLE_TEMPERATURE_C: u8 = 110;

/// Returns whether a raw CPU-temperature byte is credible enough to identify
/// an HRAM window. This must include real high-load temperatures: N155A can
/// legitimately exceed the former 80°C ceiling while the daemon starts.
const fn is_plausible_temperature(value: u8) -> bool {
    value > MIN_PLAUSIBLE_TEMPERATURE_C && value <= MAX_PLAUSIBLE_TEMPERATURE_C
}

pub struct EcDevice {
    /// Mutex wraps the low-level I/O backend.
    /// Locking it ensures atomic multi-step Super I/O transactions,
    /// preventing thread race conditions during Index/Data port writes.
    io: Mutex<RawPortIo>,
    /// Selected profile for runtime
    pub profile: &'static BoardProfile,
    pub rt: EcRuntime,
}

impl EcDevice {
    /// Detects the board from DMI. Fails on unknown boards; the caller decides
    /// whether to serve Unsupported over IPC.
    pub fn new(insecure_mode: bool) -> Result<Self> {
        let board = crate::services::get_board_name();
        let profile = detect(&board).ok_or_else(|| anyhow::anyhow!("Unsupported motherboard: {board}"))?;
        log::info!("Detected motherboard {}.", profile.id);
        Self::new_with_profile(profile, insecure_mode)
    }

    pub fn new_with_profile(profile: &'static BoardProfile, insecure_mode: bool) -> Result<Self> {
        let io = RawPortIo::new()?;
        let (port, chip) = probe_chip(&io, insecure_mode)?;

        let mut device = Self {
            io: Mutex::new(io),
            profile,
            rt: EcRuntime {
                port,
                hram_offset: 0,
                chip_id1: chip.0,
                chip_id2: chip.1,
                chip_ver: chip.2,
            },
        };

        device.rt.hram_offset = device.detect_hram()?;
        log::info!(
            "Board {}, chip IT{:02X}{:02X}-{:02X}, HRAM window {:#06X}",
            profile.id,
            chip.0,
            chip.1,
            chip.2,
            device.rt.hram_offset
        );
        Ok(device)
    }

    #[inline]
    pub fn hram_offset(&self) -> u16 {
        self.rt.hram_offset
    }

    /// Temperature stays the primary signal; RSOC only breaks ties between
    /// several plausible windows, so this never rejects what used to work.
    fn detect_hram(&self) -> Result<u16> {
        let temp = match self.profile.sensor(SensorRole::Cpu).map(|s| s.addr) {
            Some(Addr::Ram(off)) => off,
            _ => bail!("Profile {} has no RAM-based CPU sensor", self.profile.id),
        };
        let rsoc = self.profile.battery.and_then(|b| match b.rsoc {
            Addr::Ram(off) => Some(off),
            _ => None,
        });

        let mut candidates = Vec::new();
        for &base in self.profile.hram_candidates {
            if let Ok(t) = self.read_abs(base + temp) {
                // A deliberately conservative read-only heuristic for
                // detecting the HRAM window. It follows the same 1–110°C
                // sanity range used by telemetry, rather than rejecting a
                // valid high-load CPU temperature above 80°C.
                if is_plausible_temperature(t) {
                    candidates.push(base);
                }
            }
        }

        match candidates.len() {
            0 => bail!("Failed to detect HRAM window base address"),
            1 => Ok(candidates[0]),
            _ => {
                // unlikely case!!!
                if let Some(off) = rsoc {
                    for &base in &candidates {
                        if matches!(self.read_abs(base + off), Ok(v) if v <= 100) {
                            return Ok(base);
                        }
                    }
                }
                log::warn!("Ambiguous HRAM window, candidates: {:04X?}", candidates);
                Ok(candidates[0])
            }
        }
    }

    /// Executes a closure safely within a locked Mutex context.
    /// This ensures atomic multi-step Super I/O transactions (like reading MSB and LSB),
    /// preventing thread race conditions and data tearing.
    pub fn with_batch<F, R>(&self, f: F) -> Result<R>
    where
        F: FnOnce(&EcBatch) -> Result<R>,
    {
        let guard = self.io.lock().unwrap();
        let batch = EcBatch { io: guard, rt: self.rt, profile: self.profile };
        f(&batch)
    }

    // --- High-Level Facades ---
    pub fn read(&self, addr: Addr) -> Result<u8> {
        self.with_batch(|b| b.read(addr))
    }

    pub fn write(&self, addr: Addr, val: u8) -> Result<()> {
        self.with_batch(|b| b.write(addr, val))
    }

    /// Read-modify-write for bytes shared with firmware. Returns the value read back.
    pub fn update_bits(&self, addr: Addr, mask: u8, val: u8) -> Result<u8> {
        self.with_batch(|b| {
            let cur = b.read(addr)?;
            b.write(addr, (cur & !mask) | (val & mask))?;
            b.read(addr)
        })
    }

    pub(crate) fn read_abs(&self, addr: u16) -> Result<u8> {
        self.with_batch(|b| b.read_abs(addr))
    }
}

#[cfg(test)]
mod tests {
    use super::is_plausible_temperature;

    #[test]
    fn hram_temperature_heuristic_accepts_safe_high_load_values() {
        assert!(!is_plausible_temperature(0x10));
        assert!(is_plausible_temperature(0x11));
        assert!(is_plausible_temperature(80));
        assert!(is_plausible_temperature(89));
        assert!(is_plausible_temperature(110));
        assert!(!is_plausible_temperature(111));
        assert!(!is_plausible_temperature(u8::MAX));
    }
}

fn probe_chip(io: &RawPortIo, insecure_mode: bool) -> Result<(u16, (u8, u8, u8))> {
    let probe_ports = [0x2E, 0x4E, 0x6E];
    let mut last = probe_ports[0];

    for &p in &probe_ports {
        last = p;
        let Ok(id1) = raw_read(io, p, REG_CHIP_ID1) else {
            continue;
        };
        if matches!(id1, 0x55 | 0x81 | 0x85 | 0x89 | 0x90) {
            if id1 != 0x55 {
                log::warn!("Found chip ID {:#X} at port {:#X}", id1, p);
                log::warn!("Note: This chip may not be fully supported");
            }
            let id2 = raw_read(io, p, REG_CHIP_ID2).unwrap_or(0);
            let ver = raw_read(io, p, REG_CHIP_VER).unwrap_or(0);
            return Ok((p, (id1, id2, ver)));
        }
    }

    if insecure_mode {
        log::warn!("ITE chip not detected on any known port.");
        log::warn!(
            "INSECURE MODE: Proceeding blindly. Interacting with unknown hardware may cause system instability or damage!"
        );
        Ok((last, (0, 0, 0)))
    } else {
        bail!("ITE IT5570/IT8987 chip not found on any known port")
    }
}

/// Diagnostic probe that never initializes the EC. Used for the Unsupported hint.
pub fn probe_chip_only() -> Option<(u8, u8, u8)> {
    let io = RawPortIo::new().ok()?;
    probe_chip(&io, false).ok().map(|(_, chip)| chip)
}

/// Small helper for this module
fn raw_read(io: &RawPortIo, port: u16, addr: u16) -> Result<u8> {
    io.outb(port, 0x2E)?;
    io.outb(port + 1, 0x11)?;
    io.outb(port, 0x2F)?;
    io.outb(port + 1, (addr >> 8) as u8)?;
    io.outb(port, 0x2E)?;
    io.outb(port + 1, 0x10)?;
    io.outb(port, 0x2F)?;
    io.outb(port + 1, (addr & 0xFF) as u8)?;
    io.outb(port, 0x2E)?;
    io.outb(port + 1, 0x12)?;
    io.outb(port, 0x2F)?;
    io.inb(port + 1)
}

/// A short-lived transaction guard holding the hardware mutex.
/// Contains the actual low-level port read/write implementations.
pub struct EcBatch<'a> {
    io: std::sync::MutexGuard<'a, RawPortIo>,
    pub rt: EcRuntime,
    pub profile: &'static BoardProfile,
}

impl<'a> EcBatch<'a> {
    #[inline(always)]
    fn resolve(&self, addr: Addr) -> u16 {
        match addr {
            Addr::Reg(x) => x,
            Addr::Ram(x) => self.rt.hram_offset + x,
            Addr::Banked(x) => x + (self.rt.hram_offset & 0xF000),
        }
    }

    pub fn read(&self, addr: Addr) -> Result<u8> {
        self.read_abs(self.resolve(addr))
    }

    pub fn write(&self, addr: Addr, val: u8) -> Result<()> {
        self.write_abs(self.resolve(addr), val)
    }

    pub fn read_abs(&self, addr: u16) -> Result<u8> {
        raw_read(&self.io, self.rt.port, addr)
    }

    /// Writes a single byte to the specified EC absolute register address.
    pub fn write_abs(&self, addr: u16, val: u8) -> Result<()> {
        let p = self.rt.port;
        self.io.outb(p, 0x2E)?;
        self.io.outb(p + 1, 0x11)?;
        self.io.outb(p, 0x2F)?;
        self.io.outb(p + 1, (addr >> 8) as u8)?;
        self.io.outb(p, 0x2E)?;
        self.io.outb(p + 1, 0x10)?;
        self.io.outb(p, 0x2F)?;
        self.io.outb(p + 1, (addr & 0xFF) as u8)?;
        self.io.outb(p, 0x2E)?;
        self.io.outb(p + 1, 0x12)?;
        self.io.outb(p, 0x2F)?;
        self.io.outb(p + 1, val)?;
        Ok(())
    }
}
