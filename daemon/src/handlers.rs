use crate::{
    ec::{self, EcDevice},
    telemetry,
};
use anyhow::{Context, Result, anyhow, bail};
use ipc::{DaemonCommand, ErrorCode, IpcError, IpcRequest, IpcResponse, SystemInfo};
use lecoo_types::{caps::ChargeStatus, ec_types::*, settings::CurrentSettings};

#[cfg(windows)]
const STATE_PATH: &str = "C:\\ProgramData\\LecooControl\\daemon_state.bin";
#[cfg(not(windows))]
const STATE_PATH: &str = "/var/lib/lecoo-control/daemon_state.bin";

pub trait DaemonState: Sized {
    fn load() -> Result<Self>;
    fn load_or_default() -> Self;
    fn save(&self) -> Result<()>;
    fn restore_state(&self, ec: &EcDevice) -> Result<()>;
}

impl DaemonState for CurrentSettings {
    fn save(&self) -> Result<()> {
        let dir = std::path::Path::new(STATE_PATH).parent().context("Invalid state path")?;
        std::fs::create_dir_all(dir)?;

        let tmp = format!("{STATE_PATH}.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        std::fs::rename(&tmp, STATE_PATH)?;

        Ok(())
    }

    fn load() -> Result<Self> {
        match std::fs::read(STATE_PATH) {
            Ok(bytes) => serde_json::from_slice(&bytes).context("Failed to parse state file"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e).context("Failed to read state file"),
        }
    }

    fn load_or_default() -> Self {
        Self::load()
            .map_err(|err| log::error!("Load state error: {}", err))
            .unwrap_or_default()
    }

    fn restore_state(&self, ec: &EcDevice) -> Result<()> {
        let mut errors: Vec<String> = Vec::new();

        let step = |name: &str, result: Result<()>, errors: &mut Vec<String>| {
            if let Err(e) = result {
                log::warn!("restore {name}: {e}");
                errors.push(format!("{name}: {e}"));
            }
        };

        if !matches!(ec.profile.kbd, ec::KbdOps::None) {
            step("kbd", ec::apply_keyboard_backlight(ec, &self.keyboard_backlight), &mut errors);
        }
        if !matches!(ec.profile.led, ec::LedOps::None) {
            step("led", ec::apply_led_mode(ec, &self.led_mode), &mut errors);
        }
        if ec.profile.power.is_some() {
            step("power", ec::apply_power_profile(ec, &self.power_profile).map(|_| ()), &mut errors);
        }
        for (index, mode) in [(FanIndex::Cpu, &self.fan_mode_cpu), (FanIndex::Gpu, &self.fan_mode_gpu)] {
            if ec.profile.fan(index).is_some() {
                step("fan", ec::apply_fan_mode(ec, &index, mode), &mut errors);
            }
        }
        if !matches!(ec.profile.charge, ec::ChargeOps::None) {
            match ec::reconcile(ec, &self.charge) {
                Ok(Some(reason)) => log::info!("charge pending: {reason}"),
                Ok(None) => {}
                Err(e) => {
                    log::warn!("restore charge: {e}");
                    errors.push(format!("charge: {e}"));
                }
            }
        }

        if errors.is_empty() { Ok(()) } else { bail!("Partial restore: {}", errors.join("; ")) }
    }
}

pub fn do_work(req: &IpcRequest) -> IpcResponse {
    if let Some(info) = crate::UNSUPPORTED.get() {
        return IpcResponse::Error(IpcError::new(ErrorCode::UnsupportedHardware, "", Some(info.clone())));
    }
    let ec = crate::EC.get().unwrap();

    let result = match req {
        // GETTERS:
        IpcRequest::GetSystemState => get_system_state(ec),
        IpcRequest::GetFansRPM => get_fans_rpm(ec),
        IpcRequest::GetTemperatures => get_temperatures(ec),
        IpcRequest::GetChargeLimit => get_charge_limit(ec),
        IpcRequest::GetPowerProfile => get_power_profile(ec),
        IpcRequest::GetKeyboardBacklight => get_keyboard_backlight(ec),
        IpcRequest::GetChargeStatus => get_charge_status(ec),

        // SETTERS:
        IpcRequest::SetPowerProfile(profile) => set_power_profile(ec, profile),
        IpcRequest::SetFanMode { fan, mode } => set_fan_mode(ec, fan, mode),
        IpcRequest::SetKeyboardBacklight(level) => set_keyboard_backlight(ec, level),
        IpcRequest::SetLedMode(mode) => set_led_mode(ec, mode),
        IpcRequest::SetChargeIntent(intent) => set_charge_intent(ec, intent),
        IpcRequest::SetChargeLimit(limit) => set_charge_limit_legacy(ec, limit),

        // Daemon command
        IpcRequest::DaemonCommand(command) => process_daemon_command(ec, command),

        IpcRequest::Unknown => Ok(IpcResponse::Error(IpcError::new(ErrorCode::UnsupportedRequest, "", None))),
    };

    match result {
        Ok(success) => success,
        Err(err) => IpcResponse::Error(IpcError::new(ErrorCode::Internal, err.to_string(), None)),
    }
}

fn process_daemon_command(ec: &EcDevice, command: &DaemonCommand) -> Result<IpcResponse> {
    match command {
        DaemonCommand::RestoreDefaults => {
            let mut state = get_state()?;
            *state = CurrentSettings::default();
            state.save()?;
            state.restore_state(ec)?;
            Ok(IpcResponse::Success)
        }

        DaemonCommand::ActivateTelemetry(is_enabled) => {
            let mut state = get_state()?;
            state.telemetry_enabled = *is_enabled;
            state.save()?;

            if *is_enabled {
                telemetry::enable();
                Ok(IpcResponse::Success)
            } else {
                telemetry::disable();
                Ok(IpcResponse::TelemetryDisabledInfo)
            }
        }

        DaemonCommand::ApplySettings => {
            let state = get_state()?;
            state.restore_state(ec)?;
            Ok(IpcResponse::Success)
        }

        DaemonCommand::GetSettings => Ok(IpcResponse::Settings(Box::new(get_state()?.clone()))),
        DaemonCommand::GetTelemetryId => Ok(IpcResponse::TelemetryId(get_state()?.telemetry_client_id)),
        DaemonCommand::GetCapabilities => {
            Ok(IpcResponse::Capabilities(Box::new(ec.profile.caps(crate::VERSION))))
        }
        DaemonCommand::Read(addr) => Ok(IpcResponse::Byte(ec.read_abs(*addr)?)),
        #[cfg(feature = "unsafe_write_using_ipc")]
        DaemonCommand::Write(addr, val) => {
            log::warn!("Raw IPC write: {addr:#06X} = {val:#04X}");
            ec.write(ec::Addr::Reg(*addr), *val)?;
            Ok(IpcResponse::Success)
        }
        #[cfg(not(feature = "unsafe_write_using_ipc"))]
        DaemonCommand::Write(..) => Ok(IpcResponse::Error(IpcError::new(
            ErrorCode::UnsupportedRequest,
            "Daemon was built without unsafe_write_using_ipc",
            None,
        ))),

        DaemonCommand::ActivateProcessSuspend(_) => todo!(),
        DaemonCommand::RunPrepareShutdown => todo!(),
        DaemonCommand::RunPrepareSuspend => todo!(),
        DaemonCommand::RunPrepareResume => todo!(),
        DaemonCommand::Unknown => Ok(IpcResponse::Error(IpcError::new(ErrorCode::UnsupportedRequest, "", None))),
    }
}

// Getters

fn get_charge_status(ec: &EcDevice) -> Result<IpcResponse> {
    let desired = get_state()?.charge;
    let (effective, thresholds) = ec::effective_charge(ec)?;

    let pending = if effective == desired {
        None
    } else {
        match ec::charge_availability(ec, &desired)? {
            ec::Availability::Blocked(reason) => Some(reason),
            _ => None,
        }
    };

    Ok(IpcResponse::ChargeStatus(ChargeStatus {
        desired,
        effective,
        soc: ec::read_battery_rsoc(ec)?,
        pending,
        thresholds,
    }))
}

fn set_charge_intent(ec: &EcDevice, intent: &ChargeIntent) -> Result<IpcResponse> {
    match ec::charge_availability(ec, intent)? {
        ec::Availability::Unsupported(reason) => {
            return Ok(IpcResponse::Error(IpcError::new(ErrorCode::UnsupportedHardware, reason, None)));
        }
        ec::Availability::Blocked(reason) => {
            get_state()?.charge = *intent;
            let _ = get_state()?.save();
            return Ok(IpcResponse::Error(IpcError::new(ErrorCode::Precondition, reason, None)));
        }
        ec::Availability::Ready => {}
    }

    ec::apply_charge(ec, intent)?;
    get_state()?.charge = *intent;
    let _ = get_state()?.save();
    Ok(IpcResponse::Success)
}

// todo: legacy, remove!
fn set_charge_limit_legacy(ec: &EcDevice, limit: &ChargeLimit) -> Result<IpcResponse> {
    let (min, max) = limit.as_percent();
    let intent = if max >= 100 || (min == 0 && max == 0) {
        ChargeIntent::Full
    } else {
        ChargeIntent::Preserve(Some(ChargeRange { min, max }))
    };
    set_charge_intent(ec, &intent)
}

fn get_charge_limit(ec: &EcDevice) -> Result<IpcResponse> {
    let (min, max) = ec::read_charge_limit(ec)?;
    let current = ec::read_battery_rsoc(ec)?;
    Ok(IpcResponse::ChargeLimit { min, max, current })
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

    let chip = format!("IT{:02X}{:02X}", chip_id1, chip_id2);
    let revision = format!("{:02X}", chip_ver);

    let info = SystemInfo {
        chip,
        revision,
        hram_offset: ec.hram_offset(),
        daemon_version: crate::VERSION.to_string(),
    };
    Ok(IpcResponse::SystemInfo(info))
}

fn get_fans_rpm(ec: &EcDevice) -> Result<IpcResponse> {
    let (cpu, gpu) = ec::read_fans_rpm(ec)?;
    Ok(IpcResponse::FanRpm { cpu, gpu })
}

fn get_temperatures(ec: &EcDevice) -> Result<IpcResponse> {
    let (cpu_c, sys_c) = ec::read_temperatures(ec)?;
    Ok(IpcResponse::Temps { cpu_c, sys_c })
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

fn set_keyboard_backlight(ec: &EcDevice, level: &KeyboardBacklightLevel) -> Result<IpcResponse> {
    ec::apply_keyboard_backlight(ec, level)?;
    let mut state = get_state()?;
    state.keyboard_backlight = *level;
    Ok(IpcResponse::Success)
}

fn set_fan_mode(ec: &EcDevice, fan: &FanIndex, mode: &FanMode) -> Result<IpcResponse> {
    ec::apply_fan_mode(ec, fan, mode)?;
    let mut state = get_state()?;
    match fan {
        FanIndex::Cpu => state.fan_mode_cpu = *mode,
        FanIndex::Gpu => state.fan_mode_gpu = *mode,
    }
    Ok(IpcResponse::Success)
}

fn set_power_profile(ec: &EcDevice, profile: &PowerProfile) -> Result<IpcResponse> {
    ec::apply_power_profile(ec, profile)?;
    let mut state = get_state()?;
    state.power_profile = *profile;
    Ok(IpcResponse::Success)
}

fn set_led_mode(ec: &EcDevice, mode: &PowerLedMode) -> Result<IpcResponse> {
    ec::apply_led_mode(ec, mode)?;
    let mut state = get_state()?;
    state.led_mode = *mode;
    Ok(IpcResponse::Success)
}
