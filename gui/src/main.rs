// This is a desktop application, not a command-line tool. On Windows this
// prevents a separate console window from appearing when users launch the GUI.
#![cfg_attr(windows, windows_subsystem = "windows")]

use std::{
    sync::mpsc::{self, Receiver, RecvTimeoutError, Sender},
    thread,
    time::{Duration, Instant},
};

use eframe::egui::{self, Color32, RichText};
use ipc::{DaemonCommand, ErrorCode, IpcClient, IpcError, IpcRequest, IpcResponse, SystemInfo};
use lecoo_types::{
    caps::{Capabilities, ChargeCaps, ChargeStatus, FanCaps},
    ec_types::{
        BreathConfig, ChargeIntent, ChargeRange, FanIndex, FanMode, KeyboardBacklightLevel, PowerLedMode,
        PowerProfile,
    },
    settings::CurrentSettings,
};

const APP_NAME: &str = "Lecoo Control Center";
const REFRESH_INTERVAL: Duration = Duration::from_secs(2);

#[cfg(windows)]
mod windows_integration {
    use super::APP_NAME;
    use std::{
        collections::VecDeque,
        env,
        io::ErrorKind,
        path::Path,
        sync::{Arc, Mutex},
    };

    use eframe::egui;
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use tray_icon::{
        Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent,
        menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem},
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{SW_HIDE, SW_SHOW, SetForegroundWindow, ShowWindow};
    use winreg::{
        RegKey,
        enums::{HKEY_CURRENT_USER, KEY_WRITE},
    };

    const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const STARTUP_VALUE: &str = "LecooControlCenter";
    const PREFERENCES_KEY: &str = r"Software\LecooControlCenter";
    const MINIMIZE_TO_TRAY_VALUE: &str = "MinimizeToTray";
    const TRAY_SIZE: u32 = 32;

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum TrayAction {
        Show,
        Hide,
        Exit,
    }

    pub(super) fn tray_action_for_menu_id(menu_id: &str) -> Option<TrayAction> {
        match menu_id {
            "show-window" => Some(TrayAction::Show),
            "hide-window" => Some(TrayAction::Hide),
            "exit-app" => Some(TrayAction::Exit),
            _ => None,
        }
    }

    pub struct TrayController {
        _icon: TrayIcon,
        actions: Arc<Mutex<VecDeque<TrayAction>>>,
    }

    pub fn primary_window_handle(creation_context: &eframe::CreationContext<'_>) -> Option<isize> {
        let handle = creation_context.window_handle().ok()?.as_raw();
        match handle {
            RawWindowHandle::Win32(handle) => Some(handle.hwnd.get()),
            _ => None,
        }
    }

    pub fn set_primary_window_visible(window_handle: isize, visible: bool) {
        unsafe {
            ShowWindow(window_handle as _, if visible { SW_SHOW } else { SW_HIDE });
            if visible {
                SetForegroundWindow(window_handle as _);
            }
        }
    }

    /// eframe shows the main viewport just after its first paint, even when
    /// the viewport was initially configured as hidden. Once the tray icon is
    /// available, hide it shortly afterwards so an autostarted application
    /// lands in the notification area rather than leaving a window onscreen.
    pub fn hide_primary_window_after_startup(window_handle: Option<isize>) {
        let Some(window_handle) = window_handle else {
            return;
        };

        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(250));
            set_primary_window_visible(window_handle, false);
        });
    }

    impl TrayController {
        pub fn new(repaint_context: egui::Context) -> Result<Self, String> {
            let actions = Arc::new(Mutex::new(VecDeque::new()));
            let menu = Menu::new();
            let show = MenuItem::with_id("show-window", "显示窗口", true, None);
            let hide = MenuItem::with_id("hide-window", "隐藏到通知区域", true, None);
            let separator = PredefinedMenuItem::separator();
            let exit = MenuItem::with_id("exit-app", "退出 Lecoo 控制中心", true, None);
            menu.append_items(&[&show, &hide, &separator, &exit])
                .map_err(|error| format!("创建托盘菜单失败：{error}"))?;

            let icon = TrayIconBuilder::new()
                .with_tooltip(APP_NAME)
                .with_icon(tray_icon())
                .with_menu(Box::new(menu))
                .with_menu_on_left_click(false)
                .with_menu_on_right_click(true)
                .build()
                .map_err(|error| format!("创建托盘图标失败：{error}"))?;

            let menu_actions = Arc::clone(&actions);
            let menu_context = repaint_context.clone();
            MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
                let action = tray_action_for_menu_id(event.id().as_ref());
                if let Some(action) = action {
                    queue_action(&menu_actions, action);
                    menu_context.request_repaint();
                }
            }));

            let tray_actions = Arc::clone(&actions);
            TrayIconEvent::set_event_handler(Some(move |event: TrayIconEvent| {
                let is_left_click = matches!(
                    event,
                    TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } | TrayIconEvent::DoubleClick { button: MouseButton::Left, .. }
                );
                if is_left_click {
                    queue_action(&tray_actions, TrayAction::Show);
                    repaint_context.request_repaint();
                }
            }));

            Ok(Self { _icon: icon, actions })
        }

        pub fn drain_actions(&self) -> Vec<TrayAction> {
            let Ok(mut actions) = self.actions.lock() else {
                return Vec::new();
            };
            actions.drain(..).collect()
        }
    }

    pub fn startup_enabled() -> Result<bool, String> {
        let root = RegKey::predef(HKEY_CURRENT_USER);
        let run = match root.open_subkey(RUN_KEY) {
            Ok(run) => run,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(format!("读取 Windows 启动项失败：{error}")),
        };

        match run.get_value::<String, _>(STARTUP_VALUE) {
            Ok(_) => Ok(true),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
            Err(error) => Err(format!("读取 Windows 启动项失败：{error}")),
        }
    }

    pub fn set_startup_enabled(enabled: bool) -> Result<(), String> {
        let root = RegKey::predef(HKEY_CURRENT_USER);
        if enabled {
            let (run, _) = root
                .create_subkey(RUN_KEY)
                .map_err(|error| format!("打开 Windows 启动项失败：{error}"))?;
            let executable = env::current_exe().map_err(|error| format!("无法确定当前程序路径：{error}"))?;
            run.set_value(STARTUP_VALUE, &startup_command(&executable))
                .map_err(|error| format!("写入 Windows 启动项失败：{error}"))?;
        } else {
            let run = match root.open_subkey_with_flags(RUN_KEY, KEY_WRITE) {
                Ok(run) => run,
                Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
                Err(error) => return Err(format!("打开 Windows 启动项失败：{error}")),
            };
            match run.delete_value(STARTUP_VALUE) {
                Ok(()) => {}
                Err(error) if error.kind() == ErrorKind::NotFound => {}
                Err(error) => return Err(format!("移除 Windows 启动项失败：{error}")),
            }
        }
        Ok(())
    }

    pub fn minimize_to_tray_enabled() -> bool {
        let root = RegKey::predef(HKEY_CURRENT_USER);
        let Ok(preferences) = root.open_subkey(PREFERENCES_KEY) else {
            return false;
        };
        preferences.get_value::<u32, _>(MINIMIZE_TO_TRAY_VALUE).unwrap_or(0) != 0
    }

    pub fn set_minimize_to_tray_enabled(enabled: bool) -> Result<(), String> {
        let root = RegKey::predef(HKEY_CURRENT_USER);
        let (preferences, _) = root
            .create_subkey(PREFERENCES_KEY)
            .map_err(|error| format!("打开应用偏好设置失败：{error}"))?;
        preferences
            .set_value(MINIMIZE_TO_TRAY_VALUE, &(u32::from(enabled)))
            .map_err(|error| format!("保存通知区域偏好失败：{error}"))
    }

    pub fn startup_command(executable: &Path) -> String {
        format!("\"{}\" --minimized", executable.display())
    }

    pub fn tray_icon_rgba() -> Vec<u8> {
        let mut pixels = vec![0_u8; (TRAY_SIZE * TRAY_SIZE * 4) as usize];
        let center = (TRAY_SIZE as i32 - 1) / 2;
        let radius = 14_i32;

        for y in 0..TRAY_SIZE as i32 {
            for x in 0..TRAY_SIZE as i32 {
                let index = ((y * TRAY_SIZE as i32 + x) * 4) as usize;
                let dx = x - center;
                let dy = y - center;
                if dx * dx + dy * dy <= radius * radius {
                    pixels[index..index + 4].copy_from_slice(&[25, 110, 190, 255]);
                }
                if (11..=15).contains(&x) && (8..=23).contains(&y)
                    || (11..=22).contains(&x) && (19..=23).contains(&y)
                {
                    pixels[index..index + 4].copy_from_slice(&[255, 255, 255, 255]);
                }
            }
        }
        pixels
    }

    fn queue_action(actions: &Arc<Mutex<VecDeque<TrayAction>>>, action: TrayAction) {
        if let Ok(mut actions) = actions.lock() {
            actions.push_back(action);
        }
    }

    fn tray_icon() -> Icon {
        Icon::from_rgba(tray_icon_rgba(), TRAY_SIZE, TRAY_SIZE)
            .expect("the built-in tray icon has valid RGBA dimensions")
    }
}

#[cfg(windows)]
fn starts_minimized() -> bool {
    std::env::args_os().any(|argument| argument == std::ffi::OsStr::new("--minimized"))
}

#[cfg(not(windows))]
fn starts_minimized() -> bool {
    false
}

/// Egui's bundled fonts do not cover Chinese. On Windows, use the existing
/// system font at runtime instead of embedding a multi-megabyte font file.
#[cfg(windows)]
fn configure_platform_fonts(context: &egui::Context) {
    let Ok(system_font) = std::fs::read(r"C:\Windows\Fonts\simhei.ttf") else {
        return;
    };

    let font_name = "windows-simhei".to_owned();
    let mut fonts = egui::FontDefinitions::empty();
    fonts.font_data.insert(font_name.clone(), egui::FontData::from_owned(system_font));
    fonts
        .families
        .get_mut(&egui::FontFamily::Proportional)
        .expect("egui has a proportional font family")
        .insert(0, font_name.clone());
    fonts
        .families
        .get_mut(&egui::FontFamily::Monospace)
        .expect("egui has a monospace font family")
        .insert(0, font_name);
    context.set_fonts(fonts);
}

#[cfg(not(windows))]
fn configure_platform_fonts(_: &egui::Context) {}

fn main() -> eframe::Result<()> {
    let launch_minimized = starts_minimized();
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1080.0, 720.0])
            .with_min_inner_size([820.0, 560.0])
            .with_visible(!launch_minimized),
        // Glow keeps the native renderer small and avoids bringing in wgpu.
        renderer: eframe::Renderer::Glow,
        vsync: true,
        multisampling: 0,
        // Keep Winit in its native event loop. On this machine `run_on_demand`
        // can spin while the window is idle, whereas the regular loop sleeps
        // until input or the scheduled telemetry refresh.
        run_and_return: false,
        ..Default::default()
    };

    eframe::run_native(
        APP_NAME,
        native_options,
        Box::new(move |creation_context| Box::new(LecooApp::new(creation_context, launch_minimized))),
    )
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Dashboard,
    Thermal,
    PowerAndBattery,
    Lighting,
    Settings,
}

enum ConnectionState {
    Connecting,
    Online,
    Offline(String),
}

#[derive(Clone, Copy)]
enum NoticeKind {
    Info,
    Success,
    Error,
}

#[derive(Clone)]
struct Notice {
    kind: NoticeKind,
    text: String,
}

#[derive(Clone, Copy)]
struct TemperatureReading {
    cpu_c: u8,
    system_c: u8,
}

#[derive(Clone, Copy)]
struct FanReading {
    cpu_rpm: u16,
    gpu_rpm: u16,
}

#[derive(Clone, Default)]
struct Snapshot {
    capabilities: Option<Capabilities>,
    system: Option<SystemInfo>,
    settings: Option<CurrentSettings>,
    temperatures: Option<TemperatureReading>,
    fans: Option<FanReading>,
    charge: Option<ChargeStatus>,
    power_profile: Option<PowerProfile>,
    keyboard_backlight: Option<KeyboardBacklightLevel>,
    updated_at: Option<Instant>,
}

struct ActionOutcome {
    message: String,
    telemetry_id: Option<u64>,
}

struct Confirmation {
    title: String,
    message: String,
    label: String,
    request: IpcRequest,
    refresh_after: bool,
}

enum WorkerCommand {
    Refresh,
    Request {
        label: String,
        request: IpcRequest,
        refresh_after: bool,
    },
    Quit,
}

enum WorkerEvent {
    Snapshot(Result<Snapshot, String>),
    Action {
        label: String,
        result: Result<ActionOutcome, String>,
    },
}

/// The UI never performs blocking IPC. The worker waits on a channel when idle,
/// so it consumes no CPU between a user action or the low-frequency refresh.
struct IpcWorker {
    command_tx: Sender<WorkerCommand>,
    event_rx: Receiver<WorkerEvent>,
}

impl IpcWorker {
    fn spawn(repaint_context: egui::Context) -> Self {
        let (command_tx, command_rx) = mpsc::channel();
        let (event_tx, event_rx) = mpsc::channel();

        thread::Builder::new()
            .name("lecoo-ipc".to_owned())
            .spawn(move || ipc_worker_loop(command_rx, event_tx, repaint_context))
            .expect("failed to start the Lecoo IPC worker");

        Self { command_tx, event_rx }
    }

    fn send(&self, command: WorkerCommand) -> Result<(), mpsc::SendError<WorkerCommand>> {
        self.command_tx.send(command)
    }

    fn try_recv(&self) -> Option<WorkerEvent> {
        self.event_rx.try_recv().ok()
    }
}

impl Drop for IpcWorker {
    fn drop(&mut self) {
        let _ = self.command_tx.send(WorkerCommand::Quit);
    }
}

fn ipc_worker_loop(
    commands: Receiver<WorkerCommand>,
    events: Sender<WorkerEvent>,
    repaint_context: egui::Context,
) {
    let mut client: Option<IpcClient> = None;

    loop {
        // Polling belongs on the blocking IPC thread rather than in egui's
        // repaint loop. This keeps the window event-driven while still
        // refreshing live telemetry at a predictable, low rate.
        let command = match commands.recv_timeout(REFRESH_INTERVAL) {
            Ok(command) => command,
            Err(RecvTimeoutError::Timeout) => {
                emit(&events, &repaint_context, WorkerEvent::Snapshot(fetch_snapshot(&mut client)));
                continue;
            }
            Err(RecvTimeoutError::Disconnected) => break,
        };

        match command {
            WorkerCommand::Refresh => {
                emit(&events, &repaint_context, WorkerEvent::Snapshot(fetch_snapshot(&mut client)));
            }
            WorkerCommand::Request { label, request, refresh_after } => {
                let result = daemon_request(&mut client, &request).and_then(action_outcome);
                emit(&events, &repaint_context, WorkerEvent::Action { label, result });

                // A charge request can deliberately return a precondition error
                // after saving its desired state, so refresh after every action.
                if refresh_after {
                    emit(&events, &repaint_context, WorkerEvent::Snapshot(fetch_snapshot(&mut client)));
                }
            }
            WorkerCommand::Quit => break,
        }
    }
}

fn emit(events: &Sender<WorkerEvent>, repaint_context: &egui::Context, event: WorkerEvent) {
    if events.send(event).is_ok() {
        repaint_context.request_repaint();
    }
}

fn daemon_request(client: &mut Option<IpcClient>, request: &IpcRequest) -> Result<IpcResponse, String> {
    if client.is_none() {
        *client = Some(IpcClient::connect().map_err(|error| format!("无法连接 Lecoo 后台服务：{error}"))?);
    }

    match client.as_mut().expect("IPC client was initialized above").request(request) {
        Ok(response) => Ok(response),
        Err(error) => {
            *client = None;
            Err(format!("与 Lecoo 后台服务通信失败：{error}"))
        }
    }
}

fn fetch_snapshot(client: &mut Option<IpcClient>) -> Result<Snapshot, String> {
    let capabilities = match ipc_success(daemon_request(
        client,
        &IpcRequest::DaemonCommand(DaemonCommand::GetCapabilities),
    )?)? {
        IpcResponse::Capabilities(value) => *value,
        response => return Err(unexpected_response("硬件能力", &response)),
    };

    let system = match ipc_success(daemon_request(client, &IpcRequest::GetSystemState)?)? {
        IpcResponse::SystemInfo(value) => value,
        response => return Err(unexpected_response("系统信息", &response)),
    };

    let settings =
        match ipc_success(daemon_request(client, &IpcRequest::DaemonCommand(DaemonCommand::GetSettings))?)? {
            IpcResponse::Settings(value) => *value,
            response => return Err(unexpected_response("当前设置", &response)),
        };

    let temperatures = if capabilities.sensors.is_empty() {
        None
    } else {
        Some(match ipc_success(daemon_request(client, &IpcRequest::GetTemperatures)?)? {
            IpcResponse::Temps { cpu_c, sys_c } => TemperatureReading { cpu_c, system_c: sys_c },
            response => return Err(unexpected_response("温度数据", &response)),
        })
    };

    let fans = if capabilities.fans.is_empty() {
        None
    } else {
        Some(match ipc_success(daemon_request(client, &IpcRequest::GetFansRPM)?)? {
            IpcResponse::FanRpm { cpu, gpu } => FanReading { cpu_rpm: cpu, gpu_rpm: gpu },
            response => return Err(unexpected_response("风扇转速", &response)),
        })
    };

    let charge = if capabilities.charge.supported {
        Some(match ipc_success(daemon_request(client, &IpcRequest::GetChargeStatus)?)? {
            IpcResponse::ChargeStatus(value) => value,
            response => return Err(unexpected_response("电池状态", &response)),
        })
    } else {
        None
    };

    let power_profile = if capabilities.power_profiles.is_empty() {
        None
    } else {
        Some(match ipc_success(daemon_request(client, &IpcRequest::GetPowerProfile)?)? {
            IpcResponse::PowerLimit(value) => value,
            response => return Err(unexpected_response("性能模式", &response)),
        })
    };

    let keyboard_backlight = if capabilities.kbd.on_off || capabilities.kbd.levels || capabilities.kbd.custom
    {
        Some(match ipc_success(daemon_request(client, &IpcRequest::GetKeyboardBacklight)?)? {
            IpcResponse::KeyboardBacklight(value) => value,
            response => return Err(unexpected_response("键盘背光", &response)),
        })
    } else {
        None
    };

    Ok(Snapshot {
        capabilities: Some(capabilities),
        system: Some(system),
        settings: Some(settings),
        temperatures,
        fans,
        charge,
        power_profile,
        keyboard_backlight,
        updated_at: Some(Instant::now()),
    })
}

fn ipc_success(response: IpcResponse) -> Result<IpcResponse, String> {
    match response {
        IpcResponse::Error(error) => Err(format_ipc_error(&error)),
        value => Ok(value),
    }
}

fn action_outcome(response: IpcResponse) -> Result<ActionOutcome, String> {
    match response {
        IpcResponse::Error(error) => Err(format_ipc_error(&error)),
        IpcResponse::Success => Ok(ActionOutcome { message: "已应用。".to_owned(), telemetry_id: None }),
        IpcResponse::TelemetryDisabledInfo => {
            Ok(ActionOutcome { message: "匿名遥测已关闭。".to_owned(), telemetry_id: None })
        }
        IpcResponse::TelemetryId(id) => {
            Ok(ActionOutcome { message: format!("遥测 ID：{id:016X}"), telemetry_id: Some(id) })
        }
        response => Ok(ActionOutcome { message: format!("已完成：{response:?}"), telemetry_id: None }),
    }
}

fn format_ipc_error(error: &IpcError) -> String {
    let mut result = if let Some(info) = &error.unsupported {
        match info.chip.as_deref() {
            Some(chip) => format!("此硬件暂不受支持：{}（{}）", info.board, chip),
            None => format!("此硬件暂不受支持：{}", info.board),
        }
    } else {
        match error.code {
            ErrorCode::Internal => "后台服务返回内部错误".to_owned(),
            ErrorCode::UnsupportedRequest => "当前后台服务不支持该操作，请更新组件".to_owned(),
            ErrorCode::UnsupportedHardware => "本机硬件不支持该操作".to_owned(),
            ErrorCode::Precondition => "操作已保存，但当前条件不满足".to_owned(),
        }
    };

    if !error.message.trim().is_empty() {
        result.push_str("：");
        result.push_str(&error.message);
    }
    result
}

fn unexpected_response(name: &str, response: &IpcResponse) -> String {
    format!("读取{name}时收到意外响应：{response:?}")
}

#[cfg(windows)]
enum TrayStatus {
    Pending,
    Ready(windows_integration::TrayController),
    Unavailable(String),
}

struct LecooApp {
    worker: IpcWorker,
    snapshot: Snapshot,
    connection: ConnectionState,
    page: Page,
    notice: Option<Notice>,
    confirmation: Option<Confirmation>,
    refresh_in_flight: bool,
    action_in_flight: bool,
    cpu_pwm: u8,
    gpu_pwm: u8,
    keyboard_custom: u8,
    led_brightness: u8,
    charge_min: u8,
    charge_max: u8,
    charge_range_initialized: bool,
    telemetry_id: Option<u64>,
    #[cfg(windows)]
    tray: TrayStatus,
    #[cfg(windows)]
    startup_enabled: bool,
    #[cfg(windows)]
    minimize_to_tray: bool,
    #[cfg(windows)]
    allow_exit: bool,
    #[cfg(windows)]
    launched_minimized: bool,
    #[cfg(windows)]
    main_window_handle: Option<isize>,
}

impl LecooApp {
    fn new(creation_context: &eframe::CreationContext<'_>, launch_minimized: bool) -> Self {
        configure_platform_fonts(&creation_context.egui_ctx);
        #[cfg(not(windows))]
        let _ = launch_minimized;
        let mut app = Self {
            worker: IpcWorker::spawn(creation_context.egui_ctx.clone()),
            snapshot: Snapshot::default(),
            connection: ConnectionState::Connecting,
            page: Page::Dashboard,
            notice: None,
            confirmation: None,
            refresh_in_flight: false,
            action_in_flight: false,
            cpu_pwm: 128,
            gpu_pwm: 128,
            keyboard_custom: 128,
            led_brightness: 128,
            charge_min: 60,
            charge_max: 80,
            charge_range_initialized: false,
            telemetry_id: None,
            #[cfg(windows)]
            tray: TrayStatus::Pending,
            #[cfg(windows)]
            startup_enabled: windows_integration::startup_enabled().unwrap_or(false),
            #[cfg(windows)]
            minimize_to_tray: windows_integration::minimize_to_tray_enabled(),
            #[cfg(windows)]
            allow_exit: false,
            #[cfg(windows)]
            launched_minimized: launch_minimized,
            #[cfg(windows)]
            main_window_handle: windows_integration::primary_window_handle(creation_context),
        };
        app.refresh_now();
        app
    }

    fn is_busy(&self) -> bool {
        self.refresh_in_flight || self.action_in_flight
    }

    #[cfg(windows)]
    fn tray_is_ready(&self) -> bool {
        matches!(&self.tray, TrayStatus::Ready(_))
    }

    #[cfg(windows)]
    fn ensure_tray(&mut self, context: &egui::Context) {
        if !matches!(&self.tray, TrayStatus::Pending) {
            return;
        }

        match windows_integration::TrayController::new(context.clone()) {
            Ok(tray) => {
                self.tray = TrayStatus::Ready(tray);
                if self.launched_minimized {
                    windows_integration::hide_primary_window_after_startup(self.main_window_handle);
                    self.launched_minimized = false;
                }
            }
            Err(error) => {
                let show_window = self.launched_minimized;
                self.launched_minimized = false;
                self.tray = TrayStatus::Unavailable(error.clone());
                if show_window {
                    self.show_main_window(context);
                }
                self.set_notice(NoticeKind::Error, format!("Windows 通知区域不可用：{error}"));
            }
        }
    }

    #[cfg(windows)]
    fn show_main_window(&self, context: &egui::Context) {
        if let Some(window_handle) = self.main_window_handle {
            windows_integration::set_primary_window_visible(window_handle, true);
        }
        context.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        context.send_viewport_cmd(egui::ViewportCommand::Focus);
    }

    #[cfg(windows)]
    fn hide_main_window(&self, context: &egui::Context) {
        if let Some(window_handle) = self.main_window_handle {
            windows_integration::set_primary_window_visible(window_handle, false);
        }
        context.send_viewport_cmd(egui::ViewportCommand::Visible(false));
    }

    #[cfg(windows)]
    fn process_tray_actions(&mut self, context: &egui::Context) {
        let actions = match &self.tray {
            TrayStatus::Ready(tray) => tray.drain_actions(),
            TrayStatus::Pending | TrayStatus::Unavailable(_) => return,
        };

        for action in actions {
            match action {
                windows_integration::TrayAction::Show => {
                    self.launched_minimized = false;
                    self.show_main_window(context);
                }
                windows_integration::TrayAction::Hide => self.hide_main_window(context),
                windows_integration::TrayAction::Exit => {
                    self.allow_exit = true;
                    context.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }

    #[cfg(windows)]
    fn keep_window_in_tray_when_requested(&mut self, context: &egui::Context) {
        let close_requested = context.input(|input| input.viewport().close_requested());
        if close_requested && self.minimize_to_tray && self.tray_is_ready() && !self.allow_exit {
            context.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.hide_main_window(context);
        }
    }

    fn refresh_now(&mut self) {
        if self.is_busy() {
            return;
        }

        self.refresh_in_flight = true;
        if self.worker.send(WorkerCommand::Refresh).is_err() {
            self.refresh_in_flight = false;
            self.connection = ConnectionState::Offline("IPC 工作线程已停止".to_owned());
            self.set_notice(NoticeKind::Error, "无法启动后台通信。");
        }
    }

    fn submit(&mut self, label: impl Into<String>, request: IpcRequest, refresh_after: bool) {
        if self.is_busy() {
            return;
        }

        let label = label.into();
        let command = WorkerCommand::Request { label: label.clone(), request, refresh_after };
        if self.worker.send(command).is_ok() {
            self.action_in_flight = true;
            self.set_notice(NoticeKind::Info, format!("{label}：正在处理…"));
        } else {
            self.connection = ConnectionState::Offline("IPC 工作线程已停止".to_owned());
            self.set_notice(NoticeKind::Error, "无法向后台服务发送操作。");
        }
    }

    fn set_notice(&mut self, kind: NoticeKind, text: impl Into<String>) {
        self.notice = Some(Notice { kind, text: text.into() });
    }

    fn pump_events(&mut self) {
        while let Some(event) = self.worker.try_recv() {
            match event {
                WorkerEvent::Snapshot(result) => {
                    self.refresh_in_flight = false;
                    match result {
                        Ok(snapshot) => {
                            self.absorb_snapshot(snapshot);
                            self.connection = ConnectionState::Online;
                        }
                        Err(error) => {
                            self.connection = ConnectionState::Offline(error.clone());
                            self.set_notice(NoticeKind::Error, error);
                        }
                    }
                }
                WorkerEvent::Action { label, result } => {
                    self.action_in_flight = false;
                    match result {
                        Ok(outcome) => {
                            if let Some(id) = outcome.telemetry_id {
                                self.telemetry_id = Some(id);
                            }
                            self.set_notice(NoticeKind::Success, format!("{label}：{}", outcome.message));
                        }
                        Err(error) => self.set_notice(NoticeKind::Error, format!("{label}：{error}")),
                    }
                }
            }
        }
    }

    fn absorb_snapshot(&mut self, snapshot: Snapshot) {
        if let Some(settings) = &snapshot.settings {
            if let FanMode::Custom(value) = settings.fan_mode_cpu {
                self.cpu_pwm = value;
            }
            if let FanMode::Custom(value) = settings.fan_mode_gpu {
                self.gpu_pwm = value;
            }
            if let KeyboardBacklightLevel::Custom(value) = settings.keyboard_backlight {
                self.keyboard_custom = value;
            }
            if let PowerLedMode::Custom(value) = settings.led_mode {
                self.led_brightness = value;
            }
        }

        if !self.charge_range_initialized {
            if let Some((min, max)) = snapshot
                .capabilities
                .as_ref()
                .and_then(|capabilities| capabilities.charge.custom_range)
            {
                self.charge_min = min;
                self.charge_max = max;
                self.charge_range_initialized = true;
            }
        }

        self.snapshot = snapshot;
    }

    fn fan_mode(&self, index: FanIndex) -> FanMode {
        self.snapshot
            .settings
            .as_ref()
            .map(|settings| match index {
                FanIndex::Cpu => settings.fan_mode_cpu,
                FanIndex::Gpu => settings.fan_mode_gpu,
            })
            .unwrap_or_default()
    }

    fn fan_pwm(&self, index: FanIndex) -> u8 {
        match index {
            FanIndex::Cpu => self.cpu_pwm,
            FanIndex::Gpu => self.gpu_pwm,
        }
    }

    fn set_fan_pwm(&mut self, index: FanIndex, value: u8) {
        match index {
            FanIndex::Cpu => self.cpu_pwm = value,
            FanIndex::Gpu => self.gpu_pwm = value,
        }
    }

    fn current_power_profile(&self) -> PowerProfile {
        self.snapshot.power_profile.unwrap_or_else(|| {
            self.snapshot
                .settings
                .as_ref()
                .map(|settings| settings.power_profile)
                .unwrap_or_default()
        })
    }

    fn current_keyboard_backlight(&self) -> KeyboardBacklightLevel {
        self.snapshot.keyboard_backlight.unwrap_or_else(|| {
            self.snapshot
                .settings
                .as_ref()
                .map(|settings| settings.keyboard_backlight)
                .unwrap_or_default()
        })
    }

    fn choose_fan_mode(&mut self, index: FanIndex, mode: FanMode) {
        let fan = fan_name(index);
        let label = format!("设置 {fan} 风扇");
        let request = IpcRequest::SetFanMode { fan: index, mode };

        if fan_change_needs_confirmation(mode) {
            let message = match mode {
                FanMode::Turbo => "Turbo 模式会绕过常规安全保护。仅在您了解散热风险时继续。",
                FanMode::Custom(0) => "将风扇 PWM 设为 0 可能导致过热。仅在您确定设备处于安全状态时继续。",
                _ => unreachable!("only risky fan modes require confirmation"),
            };
            self.confirmation = Some(Confirmation {
                title: "确认风扇设置".to_owned(),
                message: message.to_owned(),
                label,
                request,
                refresh_after: true,
            });
        } else {
            self.submit(label, request, true);
        }
    }

    fn ask_confirmation(
        &mut self,
        title: impl Into<String>,
        message: impl Into<String>,
        label: impl Into<String>,
        request: IpcRequest,
        refresh_after: bool,
    ) {
        self.confirmation = Some(Confirmation {
            title: title.into(),
            message: message.into(),
            label: label.into(),
            request,
            refresh_after,
        });
    }

    fn connection_badge(&self) -> (String, Color32) {
        match &self.connection {
            ConnectionState::Connecting => ("正在连接".to_owned(), Color32::from_rgb(220, 170, 40)),
            ConnectionState::Online => ("后台服务已连接".to_owned(), Color32::from_rgb(70, 180, 110)),
            ConnectionState::Offline(_) => ("后台服务未连接".to_owned(), Color32::from_rgb(210, 80, 80)),
        }
    }

    fn show_header(&mut self, context: &egui::Context) {
        let (connection_text, connection_color) = self.connection_badge();
        let busy = self.is_busy();
        let mut refresh = false;
        #[cfg(windows)]
        let mut hide_to_tray = false;

        egui::TopBottomPanel::top("header").show(context, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.heading("Lecoo 控制中心");
                ui.separator();
                ui.colored_label(connection_color, RichText::new(connection_text).strong());
                if busy {
                    ui.spinner();
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add_enabled(!busy, egui::Button::new("刷新状态")).clicked() {
                        refresh = true;
                    }
                    #[cfg(windows)]
                    if self.tray_is_ready() && ui.button("最小化到托盘").clicked() {
                        hide_to_tray = true;
                    }
                });
            });
            ui.add_space(4.0);
        });

        if refresh {
            self.refresh_now();
        }
        #[cfg(windows)]
        if hide_to_tray {
            self.hide_main_window(context);
        }
    }

    fn show_navigation(&mut self, context: &egui::Context) {
        let mut page = self.page;
        egui::SidePanel::left("navigation")
            .resizable(false)
            .default_width(154.0)
            .show(context, |ui| {
                ui.add_space(8.0);
                ui.label(RichText::new("导航").small().weak());
                ui.add_space(4.0);
                for (candidate, title) in [
                    (Page::Dashboard, "概览"),
                    (Page::Thermal, "散热"),
                    (Page::PowerAndBattery, "性能与电池"),
                    (Page::Lighting, "灯光"),
                    (Page::Settings, "设置"),
                ] {
                    if ui.selectable_label(page == candidate, title).clicked() {
                        page = candidate;
                    }
                }

                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    ui.separator();
                    ui.label(RichText::new("低频 IPC 刷新 · 单工作线程").small().weak());
                });
            });
        self.page = page;
    }

    fn show_notice(&mut self, ui: &mut egui::Ui) {
        let Some(notice) = self.notice.clone() else {
            return;
        };

        let color = match notice.kind {
            NoticeKind::Info => Color32::from_rgb(80, 145, 210),
            NoticeKind::Success => Color32::from_rgb(70, 175, 105),
            NoticeKind::Error => Color32::from_rgb(215, 80, 80),
        };
        let mut dismiss = false;
        ui.horizontal(|ui| {
            ui.colored_label(color, notice.text);
            if ui.small_button("清除").clicked() {
                dismiss = true;
            }
        });
        ui.add_space(6.0);
        if dismiss {
            self.notice = None;
        }
    }

    fn show_dashboard(&mut self, ui: &mut egui::Ui) {
        ui.heading("概览");
        ui.label("所有控制均通过已安装的 Lecoo 后台服务生效。");
        let synced = self
            .snapshot
            .updated_at
            .map(|time| format!("最近同步：{} 秒前", time.elapsed().as_secs()))
            .unwrap_or_else(|| "最近同步：等待后台服务".to_owned());
        ui.label(RichText::new(synced).small().weak());
        ui.add_space(8.0);

        let mut retry = false;
        if let ConnectionState::Offline(error) = &self.connection {
            ui.group(|ui| {
                ui.colored_label(Color32::from_rgb(215, 80, 80), "无法读取后台服务状态");
                ui.label(error);
                if ui.button("重新连接").clicked() {
                    retry = true;
                }
            });
            ui.add_space(8.0);
        }
        if retry {
            self.refresh_now();
        }

        let cpu_temperature = self
            .snapshot
            .temperatures
            .map(|value| format!("{} °C", value.cpu_c))
            .unwrap_or_else(|| "—".to_owned());
        let system_temperature = self
            .snapshot
            .temperatures
            .map(|value| format!("{} °C", value.system_c))
            .unwrap_or_else(|| "—".to_owned());
        let cpu_fan = self
            .snapshot
            .fans
            .map(|value| format!("{} RPM", value.cpu_rpm))
            .unwrap_or_else(|| "—".to_owned());
        let battery = self
            .snapshot
            .charge
            .as_ref()
            .map(|value| format!("{}%", value.soc))
            .unwrap_or_else(|| "—".to_owned());

        ui.columns(4, |columns| {
            metric_card(&mut columns[0], "CPU 温度", &cpu_temperature, "实时读取");
            metric_card(&mut columns[1], "系统温度", &system_temperature, "实时读取");
            metric_card(&mut columns[2], "CPU 风扇", &cpu_fan, "当前转速");
            metric_card(&mut columns[3], "电池电量", &battery, "当前状态");
        });

        ui.add_space(14.0);
        ui.heading("设备信息");
        ui.add_space(4.0);
        egui::Grid::new("dashboard_system_info").num_columns(2).striped(true).show(ui, |ui| {
            let capabilities = self.snapshot.capabilities.as_ref();
            let system = self.snapshot.system.as_ref();
            grid_row(ui, "主板", capabilities.map(|value| value.board.as_str()).unwrap_or("等待连接"));
            grid_row(
                ui,
                "后台版本",
                system
                    .map(|value| value.daemon_version.as_str())
                    .or_else(|| capabilities.map(|value| value.daemon_version.as_str()))
                    .unwrap_or("—"),
            );
            grid_row(ui, "EC 芯片", system.map(|value| value.chip.as_str()).unwrap_or("—"));
            grid_row(ui, "当前性能模式", power_profile_label(self.current_power_profile()));
        });

        if let Some(status) = &self.snapshot.charge {
            ui.add_space(14.0);
            ui.heading("电池保护");
            ui.label(format!(
                "目标：{} · 生效：{}{}",
                charge_intent_label(status.desired),
                charge_intent_label(status.effective),
                status
                    .thresholds
                    .map(|(min, max)| format!(" · 阈值 {min}%–{max}%"))
                    .unwrap_or_default()
            ));
            if let Some(reason) = &status.pending {
                ui.colored_label(Color32::from_rgb(220, 170, 40), format!("等待应用：{reason}"));
            }
        }
    }

    fn show_thermal(&mut self, ui: &mut egui::Ui) {
        ui.heading("散热控制");
        ui.label("自动模式由 EC 温控表管理；Turbo 和 PWM 0 会要求二次确认。");
        ui.add_space(8.0);

        let Some(capabilities) = self.snapshot.capabilities.clone() else {
            ui.label("正在读取硬件能力…");
            return;
        };
        if capabilities.fans.is_empty() {
            ui.label("本机未声明可控制的风扇。");
            return;
        }

        let fan_count = capabilities.fans.len().min(2);
        ui.columns(fan_count, |columns| {
            for (position, fan) in capabilities.fans.into_iter().enumerate() {
                self.show_fan_card(&mut columns[position % fan_count], fan);
            }
        });
    }

    fn show_fan_card(&mut self, ui: &mut egui::Ui, fan: FanCaps) {
        let busy = self.is_busy();
        let current_mode = self.fan_mode(fan.index);
        let rpm = match (fan.index, self.snapshot.fans) {
            (FanIndex::Cpu, Some(value)) => format!("{} RPM", value.cpu_rpm),
            (FanIndex::Gpu, Some(value)) => format!("{} RPM", value.gpu_rpm),
            (_, None) => "—".to_owned(),
        };
        let mut pwm = self.fan_pwm(fan.index).min(fan.duty_max);
        let mut requested_mode = None;

        ui.group(|ui| {
            ui.heading(format!("{} 风扇", fan_name(fan.index)));
            ui.label(RichText::new(rpm).size(24.0).strong());
            ui.label(format!("当前：{}", fan_mode_label(current_mode)));
            ui.add_space(6.0);
            ui.add_enabled_ui(!busy, |ui| {
                ui.horizontal(|ui| {
                    if ui.selectable_label(matches!(current_mode, FanMode::Auto), "自动").clicked() {
                        requested_mode = Some(FanMode::Auto);
                    }
                    if ui.selectable_label(matches!(current_mode, FanMode::Full), "全速").clicked() {
                        requested_mode = Some(FanMode::Full);
                    }
                    if ui.selectable_label(matches!(current_mode, FanMode::Turbo), "Turbo").clicked() {
                        requested_mode = Some(FanMode::Turbo);
                    }
                });
                ui.add_space(4.0);
                ui.add(
                    egui::Slider::new(&mut pwm, 0..=fan.duty_max)
                        .text(format!("自定义 PWM（最大 {}）", fan.duty_max)),
                );
                if ui.button("应用自定义 PWM").clicked() {
                    requested_mode = Some(FanMode::Custom(pwm));
                }
            });
        });

        self.set_fan_pwm(fan.index, pwm);
        if let Some(mode) = requested_mode {
            self.choose_fan_mode(fan.index, mode);
        }
    }

    fn show_power_and_battery(&mut self, ui: &mut egui::Ui) {
        ui.heading("性能与电池");
        let Some(capabilities) = self.snapshot.capabilities.clone() else {
            ui.label("正在读取硬件能力…");
            return;
        };

        ui.group(|ui| {
            ui.heading("性能模式");
            let current = self.current_power_profile();
            let busy = self.is_busy();
            let mut requested = None;
            if capabilities.power_profiles.is_empty() {
                ui.label("本机未声明可切换的性能模式。");
            } else {
                ui.add_enabled_ui(!busy, |ui| {
                    ui.horizontal(|ui| {
                        for profile in &capabilities.power_profiles {
                            if ui
                                .selectable_label(*profile == current, power_profile_label(*profile))
                                .clicked()
                            {
                                requested = Some(*profile);
                            }
                        }
                    });
                });
            }
            if let Some(profile) = requested {
                self.submit("更新性能模式", IpcRequest::SetPowerProfile(profile), true);
            }
        });

        ui.add_space(10.0);
        self.show_battery_controls(ui, &capabilities.charge);
    }

    fn show_battery_controls(&mut self, ui: &mut egui::Ui, capabilities: &ChargeCaps) {
        ui.group(|ui| {
            ui.heading("电池保护");
            if !capabilities.supported {
                ui.label("此主板未声明电池充电阈值控制。");
                return;
            }

            if let Some(status) = &self.snapshot.charge {
                ui.label(format!("当前电量：{}%", status.soc));
                ui.label(format!("目标策略：{}", charge_intent_label(status.desired)));
                ui.label(format!("当前生效：{}", charge_intent_label(status.effective)));
                if let Some((min, max)) = status.thresholds {
                    ui.label(format!("当前阈值：{min}%–{max}%"));
                }
                if let Some(reason) = &status.pending {
                    ui.colored_label(Color32::from_rgb(220, 170, 40), format!("等待应用：{reason}"));
                }
            } else {
                ui.label("正在读取电池状态…");
            }

            let mut options = capabilities.presets.clone();
            append_charge_option(&mut options, "完全充电", ChargeIntent::Full);
            if capabilities.preserve_fixed.is_some() {
                append_charge_option(&mut options, "固件电池保护", ChargeIntent::Preserve(None));
            }
            if capabilities.freeze_soc.is_some() {
                append_charge_option(&mut options, "冻结充电", ChargeIntent::Freeze);
            }

            let desired = self.snapshot.charge.as_ref().map(|status| status.desired);
            let busy = self.is_busy();
            let mut requested = None;
            ui.add_space(6.0);
            ui.label("预设策略");
            ui.add_enabled_ui(!busy, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (name, intent) in &options {
                        let title =
                            if name.trim().is_empty() { charge_intent_label(*intent) } else { name.clone() };
                        if ui.selectable_label(desired == Some(*intent), title).clicked() {
                            requested = Some(*intent);
                        }
                    }
                });
            });

            if let Some((bound_min, bound_max)) = capabilities.custom_range {
                let mut min = self.charge_min.clamp(bound_min, bound_max);
                let mut max = self.charge_max.clamp(min, bound_max);
                let mut custom_requested = false;
                ui.add_space(6.0);
                ui.label(format!("自定义阈值（允许范围 {bound_min}%–{bound_max}%）"));
                ui.add_enabled_ui(!busy, |ui| {
                    ui.horizontal(|ui| {
                        ui.add(egui::Slider::new(&mut min, bound_min..=bound_max).text("开始"));
                        ui.add(egui::Slider::new(&mut max, min..=bound_max).text("停止"));
                        if ui.button("应用自定义阈值").clicked() {
                            custom_requested = true;
                        }
                    });
                });
                self.charge_min = min;
                self.charge_max = max;
                if custom_requested {
                    requested = Some(ChargeIntent::Preserve(Some(ChargeRange { min, max })));
                }
            }

            if let Some(intent) = requested {
                self.submit("更新电池保护", IpcRequest::SetChargeIntent(intent), true);
            }
        });
    }

    fn show_lighting(&mut self, ui: &mut egui::Ui) {
        ui.heading("灯光");
        let Some(capabilities) = self.snapshot.capabilities.clone() else {
            ui.label("正在读取硬件能力…");
            return;
        };

        self.show_keyboard_controls(ui, &capabilities);
        ui.add_space(10.0);
        self.show_led_controls(ui, &capabilities);
    }

    fn show_keyboard_controls(&mut self, ui: &mut egui::Ui, capabilities: &Capabilities) {
        ui.group(|ui| {
            ui.heading("键盘背光");
            let keyboard = &capabilities.kbd;
            if !(keyboard.on_off || keyboard.levels || keyboard.custom) {
                ui.label("本机未声明键盘背光控制。");
                return;
            }

            let current = self.current_keyboard_backlight();
            let busy = self.is_busy();
            let mut custom = self.keyboard_custom;
            let mut requested = None;
            ui.label(format!("当前：{}", keyboard_backlight_label(current)));
            ui.add_enabled_ui(!busy, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if keyboard.on_off
                        && ui
                            .selectable_label(matches!(current, KeyboardBacklightLevel::Off), "关闭")
                            .clicked()
                    {
                        requested = Some(KeyboardBacklightLevel::Off);
                    }
                    if keyboard.levels {
                        for (title, level) in [
                            ("低", KeyboardBacklightLevel::Low),
                            ("中", KeyboardBacklightLevel::Medium),
                            ("高", KeyboardBacklightLevel::High),
                        ] {
                            if ui.selectable_label(current == level, title).clicked() {
                                requested = Some(level);
                            }
                        }
                    }
                });
                if keyboard.custom {
                    ui.add(egui::Slider::new(&mut custom, 0..=255).text("自定义亮度"));
                    if ui.button("应用自定义亮度").clicked() {
                        requested = Some(KeyboardBacklightLevel::Custom(custom));
                    }
                }
            });

            self.keyboard_custom = custom;
            if let Some(level) = requested {
                self.submit("更新键盘背光", IpcRequest::SetKeyboardBacklight(level), true);
            }
        });
    }

    fn show_led_controls(&mut self, ui: &mut egui::Ui, capabilities: &Capabilities) {
        ui.group(|ui| {
            ui.heading("后部 LED 环");
            let led = &capabilities.led;
            if !(led.on_off || led.brightness || led.animation) {
                ui.label("本机未声明后部 LED 环控制。");
                return;
            }

            let current =
                self.snapshot.settings.as_ref().map(|settings| settings.led_mode).unwrap_or_default();
            let busy = self.is_busy();
            let mut static_brightness = self.led_brightness;
            let mut requested = None;

            ui.label(format!("当前：{}", led_mode_label(current)));
            ui.add_enabled_ui(!busy, |ui| {
                if ui
                    .selectable_label(matches!(current, PowerLedMode::Auto), "自动（保留系统电量指示）")
                    .clicked()
                {
                    requested = Some(PowerLedMode::Auto);
                }

                if led.brightness {
                    ui.add_space(4.0);
                    ui.add(egui::Slider::new(&mut static_brightness, 0..=255).text("静态亮度"));
                    if ui.button("应用静态亮度").clicked() {
                        requested = Some(PowerLedMode::Custom(static_brightness));
                    }
                }

                if led.animation {
                    ui.add_space(6.0);
                    ui.label("硬件灯效");
                    ui.horizontal_wrapped(|ui| {
                        for (title, configuration) in led_effects() {
                            if ui
                                .selectable_label(
                                    matches!(
                                        current,
                                        PowerLedMode::Animation(active) if active == configuration
                                    ),
                                    title,
                                )
                                .clicked()
                            {
                                requested = Some(PowerLedMode::Animation(configuration));
                            }
                        }
                    });
                }
            });

            self.led_brightness = static_brightness;
            ui.colored_label(
                Color32::from_rgb(220, 170, 40),
                "提示：静态自定义 LED 模式可能会取代标准电池充电指示。",
            );
            if let Some(mode) = requested {
                self.submit("更新后部 LED", IpcRequest::SetLedMode(mode), true);
            }
        });
    }

    #[cfg(windows)]
    fn show_windows_desktop_options(&mut self, ui: &mut egui::Ui) {
        let tray_ready = self.tray_is_ready();
        let tray_error = match &self.tray {
            TrayStatus::Unavailable(error) => Some(error.clone()),
            TrayStatus::Pending | TrayStatus::Ready(_) => None,
        };

        ui.group(|ui| {
            ui.heading("Windows 启动与通知区域");

            let mut startup_enabled = self.startup_enabled;
            let startup_changed = ui
                .add_enabled(
                    tray_ready || self.startup_enabled,
                    egui::Checkbox::new(&mut startup_enabled, "登录后启动（启动时最小化到通知区域）"),
                )
                .changed();
            ui.label(RichText::new("默认关闭；仅写入当前用户的启动项，不需要管理员权限。").small().weak());

            let mut minimize_to_tray = self.minimize_to_tray;
            let minimize_changed = ui
                .add_enabled(
                    tray_ready || self.minimize_to_tray,
                    egui::Checkbox::new(&mut minimize_to_tray, "关闭窗口时最小化到通知区域"),
                )
                .changed();
            ui.label(RichText::new("托盘菜单可显示窗口、隐藏到通知区域或完全退出程序。").small().weak());

            if tray_ready {
                ui.colored_label(Color32::from_rgb(70, 175, 105), "通知区域已就绪。");
            } else if let Some(error) = tray_error {
                ui.colored_label(Color32::from_rgb(215, 80, 80), format!("通知区域不可用：{error}"));
            } else {
                ui.label(RichText::new("正在初始化通知区域…").small().weak());
            }

            if startup_changed {
                match windows_integration::set_startup_enabled(startup_enabled) {
                    Ok(()) => {
                        self.startup_enabled = startup_enabled;
                        let message = if startup_enabled {
                            "已设置为登录后最小化启动。"
                        } else {
                            "已关闭登录后启动。"
                        };
                        self.set_notice(NoticeKind::Success, message);
                    }
                    Err(error) => self.set_notice(NoticeKind::Error, error),
                }
            }

            if minimize_changed {
                match windows_integration::set_minimize_to_tray_enabled(minimize_to_tray) {
                    Ok(()) => {
                        self.minimize_to_tray = minimize_to_tray;
                        let message = if minimize_to_tray {
                            "关闭窗口时将保留在通知区域。"
                        } else {
                            "关闭窗口时将直接退出程序。"
                        };
                        self.set_notice(NoticeKind::Success, message);
                    }
                    Err(error) => self.set_notice(NoticeKind::Error, error),
                }
            }
        });
    }

    fn show_settings(&mut self, ui: &mut egui::Ui) {
        ui.heading("设置");
        #[cfg(windows)]
        {
            self.show_windows_desktop_options(ui);
            ui.add_space(10.0);
        }
        let Some(settings) = self.snapshot.settings.clone() else {
            ui.label("正在读取后台设置…");
            return;
        };

        ui.group(|ui| {
            ui.heading("后台服务");
            let busy = self.is_busy();
            let mut telemetry_enabled = settings.telemetry_enabled;
            let telemetry_changed = ui
                .add_enabled(!busy, egui::Checkbox::new(&mut telemetry_enabled, "允许匿名遥测"))
                .changed();
            ui.label(RichText::new("遥测仅用于改善 EC 识别与故障诊断；可随时关闭。").small().weak());

            let mut request_id = false;
            let mut apply_settings = false;
            let mut restore_defaults = false;
            ui.add_enabled_ui(!busy, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("读取遥测 ID").clicked() {
                        request_id = true;
                    }
                    if ui.button("重新应用保存的设置").clicked() {
                        apply_settings = true;
                    }
                    if ui.button("恢复默认设置…").clicked() {
                        restore_defaults = true;
                    }
                });
            });
            if let Some(id) = self.telemetry_id {
                ui.label(format!("最近读取的遥测 ID：{id:016X}"));
            }

            if telemetry_changed {
                self.submit(
                    "更新遥测设置",
                    IpcRequest::DaemonCommand(DaemonCommand::ActivateTelemetry(telemetry_enabled)),
                    true,
                );
            }
            if request_id {
                self.submit("读取遥测 ID", IpcRequest::DaemonCommand(DaemonCommand::GetTelemetryId), false);
            }
            if apply_settings {
                self.submit(
                    "重新应用保存的设置",
                    IpcRequest::DaemonCommand(DaemonCommand::ApplySettings),
                    true,
                );
            }
            if restore_defaults {
                self.ask_confirmation(
                    "恢复默认设置",
                    "这会将后台保存的风扇、性能、电池和灯光设置恢复为默认值，并立即应用到硬件。",
                    "恢复默认设置",
                    IpcRequest::DaemonCommand(DaemonCommand::RestoreDefaults),
                    true,
                );
            }
        });

        ui.add_space(10.0);
        ui.group(|ui| {
            ui.heading("兼容性信息");
            egui::Grid::new("settings_hardware_info")
                .num_columns(2)
                .striped(true)
                .show(ui, |ui| {
                    let capabilities = self.snapshot.capabilities.as_ref();
                    let system = self.snapshot.system.as_ref();
                    grid_row(ui, "主板", capabilities.map(|value| value.board.as_str()).unwrap_or("—"));
                    grid_row(ui, "EC 芯片", system.map(|value| value.chip.as_str()).unwrap_or("—"));
                    grid_row(ui, "EC 修订", system.map(|value| value.revision.as_str()).unwrap_or("—"));
                    grid_row(
                        ui,
                        "HRAM 偏移",
                        system.map(|value| format!("{:04X}", value.hram_offset)).as_deref().unwrap_or("—"),
                    );
                });
        });
    }

    fn show_confirmation(&mut self, context: &egui::Context) {
        let mut confirm = None;
        if let Some(pending) = &self.confirmation {
            let title = pending.title.clone();
            let message = pending.message.clone();
            egui::Window::new(title).collapsible(false).resizable(false).show(context, |ui| {
                ui.label(message);
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.button("取消").clicked() {
                        confirm = Some(false);
                    }
                    if ui.button(RichText::new("确认执行").color(Color32::from_rgb(215, 80, 80))).clicked()
                    {
                        confirm = Some(true);
                    }
                });
            });
        }

        if let Some(approved) = confirm {
            let pending =
                self.confirmation.take().expect("confirmation exists while processing its decision");
            if approved {
                self.submit(pending.label, pending.request, pending.refresh_after);
            } else {
                self.set_notice(NoticeKind::Info, "已取消操作。");
            }
        }
    }
}

impl eframe::App for LecooApp {
    fn update(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        #[cfg(windows)]
        self.ensure_tray(context);
        #[cfg(windows)]
        self.process_tray_actions(context);
        #[cfg(windows)]
        self.keep_window_in_tray_when_requested(context);

        self.pump_events();

        self.show_header(context);
        self.show_navigation(context);
        let page = self.page;
        egui::CentralPanel::default().show(context, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                self.show_notice(ui);
                match page {
                    Page::Dashboard => self.show_dashboard(ui),
                    Page::Thermal => self.show_thermal(ui),
                    Page::PowerAndBattery => self.show_power_and_battery(ui),
                    Page::Lighting => self.show_lighting(ui),
                    Page::Settings => self.show_settings(ui),
                }
            });
        });
        self.show_confirmation(context);
    }
}

fn metric_card(ui: &mut egui::Ui, title: &str, value: &str, detail: &str) {
    ui.group(|ui| {
        ui.label(RichText::new(title).small().weak());
        ui.label(RichText::new(value).size(24.0).strong());
        ui.label(RichText::new(detail).small().weak());
    });
}

fn grid_row(ui: &mut egui::Ui, label: &str, value: &str) {
    ui.label(label);
    ui.label(value);
    ui.end_row();
}

fn fan_name(index: FanIndex) -> &'static str {
    match index {
        FanIndex::Cpu => "CPU",
        FanIndex::Gpu => "GPU / 辅助",
    }
}

fn fan_mode_label(mode: FanMode) -> String {
    match mode {
        FanMode::Auto => "自动".to_owned(),
        FanMode::Full => "全速".to_owned(),
        FanMode::Turbo => "Turbo（高风险）".to_owned(),
        FanMode::Custom(value) => format!("自定义（{value}/255）"),
    }
}

fn fan_change_needs_confirmation(mode: FanMode) -> bool {
    matches!(mode, FanMode::Turbo | FanMode::Custom(0))
}

fn power_profile_label(profile: PowerProfile) -> &'static str {
    match profile {
        PowerProfile::Silent => "安静",
        PowerProfile::Default => "默认",
        PowerProfile::Performance => "性能",
    }
}

fn keyboard_backlight_label(level: KeyboardBacklightLevel) -> String {
    match level {
        KeyboardBacklightLevel::Off => "关闭".to_owned(),
        KeyboardBacklightLevel::Low => "低".to_owned(),
        KeyboardBacklightLevel::Medium => "中".to_owned(),
        KeyboardBacklightLevel::High => "高".to_owned(),
        KeyboardBacklightLevel::Custom(value) => format!("自定义（{value}/255）"),
    }
}

fn led_mode_label(mode: PowerLedMode) -> String {
    match mode {
        PowerLedMode::Auto => "自动".to_owned(),
        PowerLedMode::Custom(value) => format!("静态亮度（{value}/255）"),
        PowerLedMode::Animation(_) => "硬件灯效".to_owned(),
    }
}

fn charge_intent_label(intent: ChargeIntent) -> String {
    match intent {
        ChargeIntent::Full => "完全充电".to_owned(),
        ChargeIntent::Preserve(None) => "固件电池保护".to_owned(),
        ChargeIntent::Preserve(Some(range)) => format!("保护阈值 {}%–{}%", range.min, range.max),
        ChargeIntent::Freeze => "冻结充电".to_owned(),
    }
}

fn append_charge_option(options: &mut Vec<(String, ChargeIntent)>, title: &str, intent: ChargeIntent) {
    if !options.iter().any(|(_, existing)| *existing == intent) {
        options.push((title.to_owned(), intent));
    }
}

fn led_effects() -> [(&'static str, BreathConfig); 10] {
    [
        ("柔和", BreathConfig::smooth()),
        ("睡眠", BreathConfig::sleep()),
        ("提醒", BreathConfig::alert()),
        ("禅意", BreathConfig::zen()),
        ("脉冲", BreathConfig::ping()),
        ("活力", BreathConfig::energetic()),
        ("警告", BreathConfig::warning()),
        ("真空", BreathConfig::vacuum()),
        ("紧急", BreathConfig::panic()),
        ("声呐", BreathConfig::sonar()),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use ipc::IpcServer;
    use lecoo_types::caps::{KbdCaps, LedCaps, SensorRole};

    fn n155a_capabilities() -> Capabilities {
        Capabilities {
            board: "N155A".to_owned(),
            daemon_version: "test-daemon".to_owned(),
            fans: vec![
                FanCaps { index: FanIndex::Cpu, duty_max: 220 },
                FanCaps { index: FanIndex::Gpu, duty_max: 220 },
            ],
            sensors: vec![SensorRole::Cpu, SensorRole::Sys],
            power_profiles: vec![PowerProfile::Silent, PowerProfile::Default, PowerProfile::Performance],
            kbd: KbdCaps { on_off: true, levels: true, custom: true },
            led: LedCaps { on_off: true, brightness: true, animation: true },
            battery_leds: true,
            charge: ChargeCaps {
                supported: true,
                custom_range: Some((20, 100)),
                presets: vec![(
                    "balanced".to_owned(),
                    ChargeIntent::Preserve(Some(ChargeRange { min: 70, max: 80 })),
                )],
                ..Default::default()
            },
        }
    }

    #[test]
    fn only_risky_fan_modes_need_confirmation() {
        assert!(!fan_change_needs_confirmation(FanMode::Auto));
        assert!(!fan_change_needs_confirmation(FanMode::Full));
        assert!(!fan_change_needs_confirmation(FanMode::Custom(1)));
        assert!(fan_change_needs_confirmation(FanMode::Custom(0)));
        assert!(fan_change_needs_confirmation(FanMode::Turbo));
    }

    #[test]
    fn charge_labels_preserve_thresholds() {
        assert_eq!(charge_intent_label(ChargeIntent::Full), "完全充电");
        assert_eq!(
            charge_intent_label(ChargeIntent::Preserve(Some(ChargeRange { min: 60, max: 80 }))),
            "保护阈值 60%–80%"
        );
    }

    #[test]
    fn custom_charge_option_is_not_duplicated() {
        let mut options = vec![("满电".to_owned(), ChargeIntent::Full)];
        append_charge_option(&mut options, "完全充电", ChargeIntent::Full);
        append_charge_option(&mut options, "冻结充电", ChargeIntent::Freeze);
        assert_eq!(options.len(), 2);
    }

    #[cfg(windows)]
    #[test]
    fn system_font_covers_chinese_ui_labels() {
        let context = egui::Context::default();
        configure_platform_fonts(&context);
        let font = egui::FontId::proportional(14.0);
        let mut covers_labels = false;
        let _ = context.run(egui::RawInput::default(), |context| {
            covers_labels = context.fonts(|fonts| fonts.has_glyphs(&font, "概览 散热 性能 电池 灯光 设置"));
        });
        assert!(covers_labels);
    }

    #[cfg(windows)]
    #[test]
    fn startup_command_quotes_program_path_and_starts_minimized() {
        let executable =
            std::path::Path::new(r"C:\Program Files\LecooControlCenter\lecoo-control-center.exe");
        assert_eq!(
            windows_integration::startup_command(executable),
            r#""C:\Program Files\LecooControlCenter\lecoo-control-center.exe" --minimized"#
        );
    }

    #[cfg(windows)]
    #[test]
    fn tray_icon_rgba_is_a_complete_square_image() {
        let pixels = windows_integration::tray_icon_rgba();
        assert_eq!(pixels.len(), 32 * 32 * 4);
        assert!(pixels.chunks_exact(4).any(|pixel| pixel == [255, 255, 255, 255]));
        assert!(pixels.chunks_exact(4).any(|pixel| pixel == [25, 110, 190, 255]));
    }

    #[cfg(windows)]
    #[test]
    fn tray_menu_ids_map_to_the_expected_actions() {
        assert_eq!(
            windows_integration::tray_action_for_menu_id("show-window"),
            Some(windows_integration::TrayAction::Show)
        );
        assert_eq!(
            windows_integration::tray_action_for_menu_id("hide-window"),
            Some(windows_integration::TrayAction::Hide)
        );
        assert_eq!(
            windows_integration::tray_action_for_menu_id("exit-app"),
            Some(windows_integration::TrayAction::Exit)
        );
        assert_eq!(windows_integration::tray_action_for_menu_id("unknown"), None);
    }

    #[test]
    fn snapshot_reads_every_supported_n155a_value_over_ipc() {
        let capabilities = n155a_capabilities();
        let settings = CurrentSettings::default();
        let (ready_tx, ready_rx) = mpsc::channel();
        let socket_name = format!("lecoo_gui_snapshot_test_{}", std::process::id());
        let server_socket_name = socket_name.clone();

        let server = thread::spawn(move || {
            let mut server = IpcServer::bind_to(&server_socket_name).expect("bind test IPC server");
            ready_tx.send(()).expect("report ready test IPC server");
            let mut connection = server.accept().expect("accept GUI IPC connection");
            connection.accept_handshake().expect("accept IPC handshake");

            for expected in [
                IpcRequest::DaemonCommand(DaemonCommand::GetCapabilities),
                IpcRequest::GetSystemState,
                IpcRequest::DaemonCommand(DaemonCommand::GetSettings),
                IpcRequest::GetTemperatures,
                IpcRequest::GetFansRPM,
                IpcRequest::GetChargeStatus,
                IpcRequest::GetPowerProfile,
                IpcRequest::GetKeyboardBacklight,
            ] {
                let request: IpcRequest = connection.recv().expect("read GUI IPC request");
                assert_eq!(request, expected);
                let response = match request {
                    IpcRequest::DaemonCommand(DaemonCommand::GetCapabilities) => {
                        IpcResponse::Capabilities(Box::new(capabilities.clone()))
                    }
                    IpcRequest::GetSystemState => IpcResponse::SystemInfo(SystemInfo {
                        chip: "ITE".to_owned(),
                        revision: "1.0".to_owned(),
                        hram_offset: 0xC400,
                        daemon_version: "test-daemon".to_owned(),
                    }),
                    IpcRequest::DaemonCommand(DaemonCommand::GetSettings) => {
                        IpcResponse::Settings(Box::new(settings.clone()))
                    }
                    IpcRequest::GetTemperatures => IpcResponse::Temps { cpu_c: 63, sys_c: 41 },
                    IpcRequest::GetFansRPM => IpcResponse::FanRpm { cpu: 2345, gpu: 1987 },
                    IpcRequest::GetChargeStatus => IpcResponse::ChargeStatus(ChargeStatus {
                        desired: ChargeIntent::Preserve(Some(ChargeRange { min: 70, max: 80 })),
                        effective: ChargeIntent::Preserve(Some(ChargeRange { min: 70, max: 80 })),
                        soc: 76,
                        pending: None,
                        thresholds: Some((70, 80)),
                    }),
                    IpcRequest::GetPowerProfile => IpcResponse::PowerLimit(PowerProfile::Performance),
                    IpcRequest::GetKeyboardBacklight => {
                        IpcResponse::KeyboardBacklight(KeyboardBacklightLevel::High)
                    }
                    other => panic!("unexpected request: {other:?}"),
                };
                connection.send(&response).expect("write GUI IPC response");
            }
        });

        ready_rx.recv().expect("wait for test IPC server");
        let mut client = Some(IpcClient::connect_to(&socket_name).expect("connect test IPC client"));
        let snapshot = fetch_snapshot(&mut client).expect("read N155A snapshot");
        server.join().expect("finish test IPC server");

        assert_eq!(snapshot.capabilities.as_ref(), Some(&n155a_capabilities()));
        assert_eq!(snapshot.temperatures.map(|value| value.cpu_c), Some(63));
        assert_eq!(snapshot.temperatures.map(|value| value.system_c), Some(41));
        assert_eq!(snapshot.fans.map(|value| value.cpu_rpm), Some(2345));
        assert_eq!(snapshot.fans.map(|value| value.gpu_rpm), Some(1987));
        assert_eq!(snapshot.charge.as_ref().map(|value| value.soc), Some(76));
        assert_eq!(snapshot.power_profile, Some(PowerProfile::Performance));
        assert_eq!(snapshot.keyboard_backlight, Some(KeyboardBacklightLevel::High));
    }
}
