use ipc::{ChargeLimit, CurrentSettings, FanIndex, FanMode, IpcResponse, KeyboardBacklightLevel, PowerLedMode, PowerProfile};
use anyhow::Result;
use crate::ec::{self, EcDevice};

#[derive(Debug, Default)]
pub struct DaemonState {
    pub settings: CurrentSettings,
}

impl DaemonState {
    pub fn load_or_default() -> Self {
        let state = CurrentSettings::load().unwrap_or_else(|| {
            log::error!("New state, why?");
            CurrentSettings::default()
        });
        DaemonState { // todo
            settings: state
        }
    }
}


// Getters

pub fn get_charge_limit(ec: &EcDevice) -> Result<IpcResponse> {
    let (min, max) = ec::read_charge_limit(ec)?;
    Ok(IpcResponse::ChargeLimit(min, max))
}

pub fn get_power_profile(ec: &EcDevice) -> Result<IpcResponse> {
    let profile = ec::read_power_profile(ec)?;
    Ok(IpcResponse::PowerLimit(profile))
}

pub fn get_keyboard_backlight(ec: &EcDevice) -> Result<IpcResponse> {
    let level = ec::read_keyboard_backlight(ec)?;
    Ok(IpcResponse::KeyboardBacklight(level))
}

pub fn get_system_state(ec: &EcDevice) -> Result<IpcResponse> {
    let (chip_id1, chip_id2, chip_ver) = ec::read_system_info(ec)?;

    let chip_name = format!("IT{:02X}{:02X}", chip_id1, chip_id2);
    let revision = format!("{:02X}", chip_ver);

    let sys_info = format!("Controller: {} (Rev {})", chip_name, revision);

    Ok(IpcResponse::Message(sys_info))
}

pub fn get_fans_rpm(ec: &EcDevice) -> Result<IpcResponse> {
    let (cpu_rpm, gpu_rpm) = ec::read_fans_rpm(ec)?;
    Ok(IpcResponse::FanRPM(cpu_rpm, gpu_rpm))
}

pub fn get_temperatures(ec: &EcDevice) -> Result<IpcResponse> {
    let (cpu_temp, sys_temp) = ec::read_temperatures(ec)?;
    Ok(IpcResponse::Temp(cpu_temp, sys_temp))
}

// Setters

pub fn set_charge_limit(ec: &EcDevice, profile: &ChargeLimit) -> Result<IpcResponse> {
    let mut state = crate::STATE.lock().unwrap();
    state.settings.charge_limit = profile.clone();

    ec::apply_charge_limit(ec, &profile)?;
    Ok(IpcResponse::Success)
}

pub fn set_keyboard_backlight(ec: &EcDevice, level: &KeyboardBacklightLevel) -> Result<IpcResponse> {
    let mut state = crate::STATE.lock().unwrap();
    state.settings.keyboard_backlight = *level;

    ec::apply_keyboard_backlight(ec, level)?;
    Ok(IpcResponse::Success)
}

pub fn set_fan_mode(ec: &EcDevice, fan: &FanIndex, mode: &FanMode) -> Result<IpcResponse> {
    let mut state = crate::STATE.lock().unwrap();
    match fan {
        FanIndex::Cpu => state.settings.fan_mode_cpu = *mode,
        FanIndex::Gpu => state.settings.fan_mode_gpu = *mode,
    }

    ec::apply_fan_mode(ec, fan, mode)?;
    Ok(IpcResponse::Success)
}

pub fn set_power_profile(ec: &EcDevice, profile: &PowerProfile) -> Result<IpcResponse> {
    let mut state = crate::STATE.lock().unwrap();
    state.settings.power_profile = *profile;

    ec::apply_power_profile(ec, &profile)?;
    Ok(IpcResponse::Success)
}

pub fn set_led_mode(ec: &EcDevice, mode: &PowerLedMode) -> Result<IpcResponse> {
    let mut state = crate::STATE.lock().unwrap();
    state.settings.led_mode = *mode;

    ec::apply_led_mode(ec, mode)?;
    Ok(IpcResponse::Success)
}
