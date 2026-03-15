use anyhow::{Context, Result, bail};

use libloading::{Library, Symbol};

const DLL_BYTES: &[u8] = include_bytes!("../../../libs/inpoutx64.dll");

type IsDriverOpenFn = unsafe extern "system" fn() -> u32;
type Out32Fn = unsafe extern "system" fn(port: i32, data: i32);
type Inp32Fn = unsafe extern "system" fn(port: i32) -> u8;

pub struct RawPortIo {
    _lib: Library,
    is_open: IsDriverOpenFn,
    out32: Out32Fn,
    inp32: Inp32Fn,
}

impl RawPortIo {
    pub fn new() -> Result<Self> {
        let mut temp_path = std::env::temp_dir();
        temp_path.push("inpoutx64_embedded.dll");
        std::fs::write(&temp_path, DLL_BYTES)
                    .context("Failed to write embedded DLL to temp directory")?;

        unsafe {
            let lib = Library::new(&temp_path)
                .with_context(|| anyhow::anyhow!("Failed to load inpoutx64.dll from temp path: {:?}", temp_path))?;

            let is_open_sym: Symbol<IsDriverOpenFn> = lib.get(b"IsInpOutDriverOpen\0")
                    .context("IsInpOutDriverOpen export not found in DLL")?;
            let out32_sym: Symbol<Out32Fn> = lib.get(b"Out32\0")
                .context("Out32 export not found in DLL")?;
            let inp32_sym: Symbol<Inp32Fn> = lib.get(b"Inp32\0")
                .context("Inp32 export not found in DLL")?;

            let is_open = *is_open_sym;
            let out32 = *out32_sym;
            let inp32 = *inp32_sym;

            if is_open() == 0 {
                bail!("InpOut driver failed to open. Try running as Administrator.");
            }

            Ok(Self {
                _lib: lib,
                is_open,
                out32,
                inp32,
            })
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
        unsafe {
            Ok((self.inp32)(port as i32))
        }
    }
}
