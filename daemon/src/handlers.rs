use ipc::{ChargeLimit, CurrentSettings, FanIndex, FanMode, IpcRequest, IpcResponse, KeyboardBacklightLevel, PowerLedMode, PowerProfile};
use anyhow::{Result, anyhow};
use crate::ec::{self, EcDevice};

#[cfg(windows)]
const STATE_PATH: &str = "C:\\ProgramData\\LecooControl\\daemon_state.bin";
#[cfg(not(windows))]
const STATE_PATH: &str = "/var/lib/lecoo-control/daemon_state.bin";

pub trait DaemonState: Sized {
    fn load() -> Option<Self>;
    fn load_or_default() -> Self;
    fn save(&self) -> Result<()> ;
    fn restore_state(&self, ec: &EcDevice) -> Result<()>;
}

impl DaemonState for CurrentSettings {
    fn save(&self) -> Result<()> {
        let dir = std::path::Path::new(STATE_PATH).parent().unwrap();
        std::fs::create_dir_all(dir)?;

        let file = std::fs::File::create(STATE_PATH)?;
        let mut writer = std::io::BufWriter::new(file);

        bincode::encode_into_std_write(self, &mut writer, bincode::config::standard())?;
        Ok(())
    }

    fn load() -> Option<Self> {
        let file = std::fs::File::open(STATE_PATH).ok()?;
        let mut reader = std::io::BufReader::new(file);

        // bincode::decode_from_std_read(&mut reader, bincode::config::standard()).ok()
        Some(bincode::decode_from_std_read::<CurrentSettings, _, _>(&mut reader, bincode::config::standard()).expect("cannot decode!"))
    }

    fn load_or_default() -> Self {
        Self::load().unwrap_or_default()
    }

    fn restore_state(&self, ec: &EcDevice) -> Result<()> {
        ec::apply_keyboard_backlight(ec, &self.keyboard_backlight)?;
        ec::apply_led_mode(ec, &self.led_mode)?;
        ec::apply_power_profile(ec, &self.power_profile)?;
        ec::apply_fan_mode(ec, &ipc::FanIndex::Cpu, &self.fan_mode_cpu)?;
        ec::apply_fan_mode(ec, &ipc::FanIndex::Gpu, &self.fan_mode_gpu)?;
        Ok(())
    }
}

pub fn do_work(req: &IpcRequest) -> IpcResponse {
    let ec = crate::EC.get().unwrap();

    let result = match req {
        // GETTERS:
        IpcRequest::GetSystemState => get_system_state(ec),

        IpcRequest::GetFansRPM => get_fans_rpm(ec),

        IpcRequest::GetTemperatures => get_temperatures(ec),

        IpcRequest::GetChargeLimit => get_charge_limit(ec),

        IpcRequest::GetPowerProfile => get_power_profile(ec),

        IpcRequest::GetKeyboardBacklight => get_keyboard_backlight(ec),

        // SETTERS:
        IpcRequest::SetPowerProfile(profile) => set_power_profile(ec, profile),

        IpcRequest::SetFanMode { fan, mode } => set_fan_mode(ec, fan, mode),

        IpcRequest::SetKeyboardBacklight(level) => set_keyboard_backlight(ec, level),

        IpcRequest::SetChargeLimit(limit) => set_charge_limit(ec, limit),

        IpcRequest::SetLedMode(mode) => set_led_mode(ec, mode),
    };

    match result {
        Ok(success) => success,
        Err(err) => IpcResponse::Error(format!("Processing request failed: {}", err)),
    }
}

// Getters

fn get_charge_limit(ec: &EcDevice) -> Result<IpcResponse> {
    let (min, max) = ec::read_charge_limit(ec)?;
    Ok(IpcResponse::ChargeLimit(min, max))
}

fn get_power_profile(ec: &EcDevice) -> Result<IpcResponse> {
    let profile = ec::read_power_profile(ec)?;
    Ok(IpcResponse::PowerLimit(profile))
}

fn get_keyboard_backlight(ec: &EcDevice) -> Result<IpcResponse> {
    let level = ec::read_keyboard_backlight(ec)?;
    Ok(IpcResponse::KeyboardBacklight(level))
}

fn get_system_state(ec: &EcDevice) -> Result<IpcResponse> {
    let (chip_id1, chip_id2, chip_ver) = ec::read_system_info(ec)?;

    let chip_name = format!("IT{:02X}{:02X}", chip_id1, chip_id2);
    let revision = format!("{:02X}", chip_ver);

    let sys_info = format!("Controller: {} (Rev {})", chip_name, revision);

    Ok(IpcResponse::Message(sys_info))
}

fn get_fans_rpm(ec: &EcDevice) -> Result<IpcResponse> {
    let (cpu_rpm, gpu_rpm) = ec::read_fans_rpm(ec)?;
    Ok(IpcResponse::FanRPM(cpu_rpm, gpu_rpm))
}

fn get_temperatures(ec: &EcDevice) -> Result<IpcResponse> {
    let (cpu_temp, sys_temp) = ec::read_temperatures(ec)?;
    Ok(IpcResponse::Temp(cpu_temp, sys_temp))
}

// Setters

#[inline]
pub fn get_state() -> Result<std::sync::MutexGuard<'static, CurrentSettings>> {
    crate::STATE
        .get()
        .ok_or_else(|| anyhow!("State not initialized. How did you get here?"))?
        .try_lock()
        .map_err(|_| anyhow!("State locked, cannot acquire lock"))
}

fn set_charge_limit(ec: &EcDevice, profile: &ChargeLimit) -> Result<IpcResponse> {
    let mut state = get_state()?;
    state.charge_limit = profile.clone();

    ec::apply_charge_limit(ec, &profile)?;
    Ok(IpcResponse::Success)
}

fn set_keyboard_backlight(ec: &EcDevice, level: &KeyboardBacklightLevel) -> Result<IpcResponse> {
    let mut state = get_state()?;
    state.keyboard_backlight = *level;

    ec::apply_keyboard_backlight(ec, level)?;
    Ok(IpcResponse::Success)
}

fn set_fan_mode(ec: &EcDevice, fan: &FanIndex, mode: &FanMode) -> Result<IpcResponse> {
    let mut state = get_state()?;
    match fan {
        FanIndex::Cpu => state.fan_mode_cpu = *mode,
        FanIndex::Gpu => state.fan_mode_gpu = *mode,
    }

    ec::apply_fan_mode(ec, fan, mode)?;
    Ok(IpcResponse::Success)
}

fn set_power_profile(ec: &EcDevice, profile: &PowerProfile) -> Result<IpcResponse> {
    let mut state = get_state()?;
    state.power_profile = *profile;

    ec::apply_power_profile(ec, &profile)?;
    Ok(IpcResponse::Success)
}

fn set_led_mode(ec: &EcDevice, mode: &PowerLedMode) -> Result<IpcResponse> {
    let mut state = get_state()?;
    state.led_mode = *mode;

    ec::apply_led_mode(ec, mode)?;
    Ok(IpcResponse::Success)
}
