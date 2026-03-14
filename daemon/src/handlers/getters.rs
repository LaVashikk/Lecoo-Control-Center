use ipc::IpcResponse;
use anyhow::Result;
use crate::EcDevice;

const REG_CHIP_ID1: u16 = 0x2000;
const REG_CHIP_ID2: u16 = 0x2001;
const REG_CHIP_VER: u16 = 0x2002;

const RAM_TEMP_CPU: u16 = 0x70;
const RAM_TEMP_SYS: u16 = 0x93;

const RAM_FAN_CPU_MSB: u16 = 0x76;
const RAM_FAN_CPU_LSB: u16 = 0x77;
const RAM_FAN_GPU_MSB: u16 = 0x79;
const RAM_FAN_GPU_LSB: u16 = 0x7A;

// ======================

fn read_system_info(ec: &EcDevice) -> Result<(u8, u8, u8)> {
    let chip_id1 = ec.read_reg(REG_CHIP_ID1)?;
    let chip_id2 = ec.read_reg(REG_CHIP_ID2)?;
    let chip_ver = ec.read_reg(REG_CHIP_VER)?;
    Ok((chip_id1, chip_id2, chip_ver))
}

fn read_fans_rpm(ec: &EcDevice) -> Result<(u16, u16)> {
    let cpu_msb = ec.read_ram(RAM_FAN_CPU_MSB)? as u16;
    let cpu_lsb = ec.read_ram(RAM_FAN_CPU_LSB)? as u16;
    let cpu_rpm = ((cpu_msb << 8) | cpu_lsb) as u16;

    let gpu_msb = ec.read_ram(RAM_FAN_GPU_MSB)? as u16;
    let gpu_lsb = ec.read_ram(RAM_FAN_GPU_LSB)? as u16;
    let gpu_rpm = ((gpu_msb << 8) | gpu_lsb) as u16;

    Ok((cpu_rpm, gpu_rpm))
}

fn read_temperatures(ec: &EcDevice) -> Result<(u8, u8)> {
    let cpu_temp = ec.read_ram(RAM_TEMP_CPU)? as u8;
    let sys_temp = ec.read_ram(RAM_TEMP_SYS)? as u8;
    Ok((cpu_temp, sys_temp))
}

pub fn get_system_state(ec: &EcDevice) -> Result<IpcResponse> {
    let (chip_id1, chip_id2, chip_ver) = read_system_info(ec)?;

    let chip_name = format!("IT{:02X}{:02X}", chip_id1, chip_id2);
    let revision = format!("{:02X}", chip_ver);

    let sys_info = format!("Controller: {} (Rev {})", chip_name, revision);

    Ok(IpcResponse::Message(sys_info))
}

pub fn get_fans_rpm(ec: &EcDevice) -> Result<IpcResponse> {
    let (cpu_rpm, gpu_rpm) = read_fans_rpm(ec)?;
    Ok(IpcResponse::FanRPM(cpu_rpm, gpu_rpm))
}

pub fn get_temperatures(ec: &EcDevice) -> Result<IpcResponse> {
    let (cpu_temp, sys_temp) = read_temperatures(ec)?;
    Ok(IpcResponse::Temp(cpu_temp, sys_temp))
}
