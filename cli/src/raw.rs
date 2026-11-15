//! Hidden `read` command: raw EC bytes by absolute address, for reversing

use anyhow::{Result, bail};
use ipc::{DaemonCommand, IpcClient, IpcRequest, IpcResponse};

const ROW: u16 = 16;

/// One IPC round-trip per byte. Slow for big ranges, but the daemon stays dumb
pub fn dump(client: &mut IpcClient, addr: u16, count: u16) -> Result<()> {
    let mut row = String::new();
    for i in 0..count.max(1) {
        let a = addr.wrapping_add(i);
        let b = match client.request(&IpcRequest::DaemonCommand(DaemonCommand::Read(a)))? {
            IpcResponse::Byte(b) => b,
            IpcResponse::Error(e) => bail!("{:?}: {}", e.code, e.message),
            other => bail!("Unexpected response: {other:?}"),
        };

        if count <= 1 {
            println!("{a:#06X} = {b:#04X} ({b})");
            return Ok(());
        }

        if i % ROW == 0 {
            row = format!("{a:#06X}:");
        }

        row.push_str(&format!(" {b:02X}"));
        if i % ROW == ROW - 1 || i == count - 1 {
            println!("{row}");
        }
    }
    Ok(())
}
