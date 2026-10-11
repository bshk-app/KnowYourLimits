pub mod alerts;
pub mod claude;
pub mod codex;
pub mod i18n;
pub mod live;
pub mod model;
#[cfg(target_os = "macos")]
mod native_notifications;
#[cfg(test)]
mod permissions_tests;
mod process;
mod storage;
#[cfg(target_os = "macos")]
pub mod tray_render;
#[cfg(all(test, windows))]
mod windows_tests;
pub mod windows_tray;

use i18n::t;
use live::Coordinator;
use model::{now_ms, AppState, ProviderId, ProviderStatus, Settings};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU8, Ordering},
    time::Duration,
};
use tauri::{
    image::Image,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, LogicalSize, Manager, PhysicalPosition, WebviewUrl, WebviewWindowBuilder,
    WindowEvent,
};

struct RuntimeState {
    coordinator: Coordinator,
    settings_path: PathBuf,
    tray: TrayItems,
    panel_click: Mutex<Option<bool>>,
    panel_tray: Mutex<String>,
    #[cfg(windows)]
    windows_icons: Mutex<[Option<(bool, windows_tray::Indicator)>; 2]>,
    #[cfg(windows)]
    settings_transition: tauri::async_runtime::Mutex<()>,
    exit: ExitGate,
    #[cfg(unix)]
    signals: Mutex<Option<SignalTask>>,
    background: Mutex<Option<SignalTask>>,
    #[cfg(target_os = "macos")]
    rendering: Mutex<Option<String>>,
}
struct TrayItems {
    open: MenuItem<tauri::Wry>,
    quit: MenuItem<tauri::Wry>,
    claude: MenuItem<tauri::Wry>,
    codex: MenuItem<tauri::Wry>,
    cancel: MenuItem<tauri::Wry>,
    status: MenuItem<tauri::Wry>,
}
#[derive(Default)]
struct ExitGate(AtomicU8);
impl ExitGate {
    fn ready(&self) -> bool {
        self.0.load(Ordering::SeqCst) == 2
    }
    fn begin(&self) -> bool {
        self.0
            .compare_exchange(0, 1, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }
    fn finish(&self) {
        self.0.store(2, Ordering::SeqCst);
    }
}
fn snapshot(app: &AppHandle) -> Result<AppState, String> {
    Ok(app.state::<RuntimeState>().coordinator.snapshot())
}
fn update_tray(app: &AppHandle) -> Result<(), String> {
    // Queued event callbacks must read authoritative state, not captured DTOs.
    let state = snapshot(app)?;
    let title = state.title(now_ms());
    let language = &state.settings.language;
    #[cfg(windows)]
    update_windows_trays(app, &state)?;
    if let Some(tray) = app.tray_by_id("main-tray") {
        #[cfg(target_os = "macos")]
        {
            let dark = app.get_webview_window("main").and_then(|w| w.theme().ok())
                == Some(tauri::Theme::Dark);
            let values: Vec<_> = state
                .settings
                .bar_providers
                .iter()
                .map(|id| {
                    state.providers.iter().find(|p| p.id == *id).map(|p| {
                        (
                            p.display_remaining(now_ms()),
                            state.refreshing == Some(*id) || p.remaining(now_ms()).is_none(),
                        )
                    })
                })
                .collect();
            let key = format!(
                "{}|{values:?}|{dark}",
                serde_json::to_string(&state.settings).map_err(|_| "Настройки недоступны")?
            );
            let runtime = app.state::<RuntimeState>();
            if runtime.rendering.lock().expect("tray rendering").as_ref() != Some(&key) {
                let frame = tray_render::render(&state, now_ms(), dark)?;
                tray.set_title(None::<&str>).map_err(|e| e.to_string())?;
                tray.set_icon_with_as_template(
                    Some(Image::new_owned(frame.rgba, frame.width, frame.height)),
                    frame.template,
                )
                .map_err(|e| e.to_string())?;
                let width = frame.logical_width;
                tray.with_inner_tray_icon(move |inner| {
                    if let (Some(item), Some(mtm)) =
                        (inner.ns_status_item(), objc2::MainThreadMarker::new())
                    {
                        if let Some(image) = item.button(mtm).and_then(|button| button.image()) {
                            image.setSize(objc2_foundation::NSSize::new(width, 22.0));
                        }
                    }
                })
                .map_err(|e| e.to_string())?;
                *runtime.rendering.lock().expect("tray rendering") = Some(key);
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            #[cfg(not(windows))]
            tray.set_title(Some(&title)).map_err(|e| e.to_string())?;
        }
        tray.set_tooltip(Some(format!("KnowYourLimits\n{title}")))
            .map_err(|e| e.to_string())?;
    }
    let items = &app.state::<RuntimeState>().tray;
    for (id, item) in [
        (ProviderId::Claude, &items.claude),
        (ProviderId::Codex, &items.codex),
    ] {
        let signed_out = state
            .providers
            .iter()
            .any(|p| p.id == id && p.status == ProviderStatus::SignedOut);
        let label = if state.refreshing == Some(id) {
            format!(
                "{} · {}",
                id.label(),
                t(
                    language,
                    if state.signing_in {
                        "signingIn"
                    } else {
                        "refreshing"
                    },
                    &[]
                )
            )
        } else if signed_out {
            t(language, "signInClaude", &[])
        } else {
            t(
                language,
                if state.connected.contains(&id) {
                    "refreshProvider"
                } else {
                    "connect"
                },
                &[("provider", id.label())],
            )
        };
        item.set_text(label).map_err(|e| e.to_string())?;
        item.set_enabled(
            state.refreshing.is_none()
                && state.retry_at.get(id).is_none_or(|until| until <= now_ms()),
        )
        .map_err(|e| e.to_string())?;
    }
    items
        .open
        .set_text(t(language, "openPanel", &[]))
        .map_err(|e| e.to_string())?;
    items
        .quit
        .set_text(t(language, "quit", &[]))
        .map_err(|e| e.to_string())?;
    items
        .cancel
        .set_text(t(
            language,
            if state.signing_in {
                "cancelSignIn"
            } else {
                "cancelRefresh"
            },
            &[],
        ))
        .map_err(|e| e.to_string())?;
    if let Some(window) = app.get_webview_window("settings") {
        let _ = window.set_title(&t(language, "settings", &[]));
    }
    items
        .cancel
        .set_enabled(state.refreshing.is_some())
        .map_err(|e| e.to_string())?;
    items
        .status
        .set_text(if state.connected.is_empty() {
            t(language, "noAccounts", &[])
        } else {
            "KnowYourLimits".into()
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(windows)]
fn update_windows_trays(app: &AppHandle, state: &AppState) -> Result<(), String> {
    let runtime = app.state::<RuntimeState>();
    let mut cache = runtime
        .windows_icons
        .lock()
        .expect("Windows tray rendering");
    for id in [ProviderId::Claude, ProviderId::Codex] {
        let Some(tray) = app.tray_by_id(windows_tray::tray_id(id)) else {
            continue;
        };
        let indicator = windows_tray::Indicator::new(state, id, now_ms());
        let visible = state.settings.bar_providers.contains(&id);
        let previous = &cache[id.index()];
        if previous.as_ref().is_none_or(|(_, old)| old != &indicator) {
            tray.set_icon(Some(Image::new_owned(
                indicator.render(windows_tray::ICON_SIZE),
                windows_tray::ICON_SIZE,
                windows_tray::ICON_SIZE,
            )))
            .map_err(|error| error.to_string())?;
            tray.set_tooltip(Some(indicator.tooltip()))
                .map_err(|error| error.to_string())?;
        }
        if previous.as_ref().is_none_or(|(old, _)| *old != visible) {
            tray.set_visible(visible)
                .map_err(|error| error.to_string())?;
        }
        cache[id.index()] = Some((visible, indicator));
    }
    Ok(())
}
#[tauri::command]
fn get_state(app: AppHandle) -> Result<AppState, String> {
    snapshot(&app)
}
#[tauri::command]
fn save_settings(app: AppHandle, settings: Settings) -> Result<AppState, String> {
    let runtime = app.state::<RuntimeState>();
    runtime
        .coordinator
        .save_settings(settings, &runtime.settings_path)
}
#[tauri::command]
fn set_refresh_minutes(
    app: AppHandle,
    provider: ProviderId,
    minutes: u16,
) -> Result<AppState, String> {
    let runtime = app.state::<RuntimeState>();
    runtime
        .coordinator
        .set_refresh_minutes(provider, minutes, &runtime.settings_path)
}
#[tauri::command]
async fn read_quota(app: AppHandle, provider: ProviderId) -> Result<AppState, String> {
    let coordinator = app.state::<RuntimeState>().coordinator.clone();
    coordinator.read_quota(provider).await
}
#[tauri::command]
async fn sign_in_claude(app: AppHandle) -> Result<AppState, String> {
    let coordinator = app.state::<RuntimeState>().coordinator.clone();
    coordinator.sign_in_claude().await
}
#[tauri::command]
async fn cancel_read(app: AppHandle) -> Result<AppState, String> {
    let coordinator = app.state::<RuntimeState>().coordinator.clone();
    coordinator.cancel_read().await
}
#[tauri::command]
async fn request_notification_permission(app: AppHandle) -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        app.state::<native_notifications::NotificationService>()
            .request_permission()
            .await
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        Err("Уведомления доступны в приложении macOS.".into())
    }
}
#[tauri::command]
async fn test_notification(
    app: AppHandle,
    provider: ProviderId,
    sound: bool,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let service = app.state::<native_notifications::NotificationService>();
        if !service.request_permission().await? {
            return Err("Разрешите уведомления для KnowYourLimits в настройках macOS.".into());
        }
        service
            .send_confirmed(alerts::Alert {
                provider,
                title: t(
                    &snapshot(&app)?.settings.language,
                    "notificationTitle",
                    &[("provider", provider.label())],
                ),
                body: t(&snapshot(&app)?.settings.language, "notificationTest", &[]),
                sound,
            })
            .await
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (app, provider, sound);
        Err("Уведомления доступны в приложении macOS.".into())
    }
}
struct SignalTask {
    cancel: tauri::async_runtime::Sender<()>,
    handle: tauri::async_runtime::JoinHandle<()>,
}
#[cfg(unix)]
fn listen_for_shutdown(app: AppHandle) -> std::io::Result<SignalTask> {
    use tokio::signal::unix::{signal, SignalKind};
    let (mut terminate, mut interrupt) = tauri::async_runtime::block_on(async {
        Ok::<_, std::io::Error>((
            signal(SignalKind::terminate())?,
            signal(SignalKind::interrupt())?,
        ))
    })?;
    let (cancel, receiver) = tauri::async_runtime::channel(1);
    let handle = tauri::async_runtime::spawn(async move {
        relay_shutdown_signals(receiver, terminate.recv(), interrupt.recv(), move |code| {
            app.state::<RuntimeState>().coordinator.request_stop();
            app.exit(code);
        })
        .await;
    });
    Ok(SignalTask { cancel, handle })
}

#[cfg(unix)]
async fn relay_shutdown_signals(
    mut cancel: tauri::async_runtime::Receiver<()>,
    terminate: impl std::future::Future<Output = Option<()>>,
    interrupt: impl std::future::Future<Output = Option<()>>,
    request_exit: impl FnOnce(i32),
) {
    let code = tokio::select! {
        biased;
        _ = cancel.recv() => return,
        signal = terminate => signal.map(|()| 143),
        signal = interrupt => signal.map(|()| 130),
    };
    if let Some(code) = code {
        request_exit(code);
        // ExitRequested owns cleanup and cancels/joins this listener before
        // setting exit-ready. No detached signal or process-drain task.
        let _ = cancel.recv().await;
    }
}

#[tauri::command]
fn hide_panel(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        window.hide().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
enum SettingsTab {
    Menubar,
    Claude,
    Codex,
}

// Sync on purpose: Tauri runs sync commands on the main thread, so a double click
// cannot interleave lookup and creation and build two settings windows.
#[cfg(not(windows))]
#[tauri::command]
fn open_settings(app: AppHandle, tab: SettingsTab) -> Result<(), String> {
    show_settings(&app, tab).map_err(|e| e.to_string())
}

#[cfg(windows)]
#[tauri::command]
async fn open_settings(app: AppHandle, tab: SettingsTab) -> Result<(), String> {
    let runtime = app.state::<RuntimeState>();
    let _guard = runtime.settings_transition.lock().await;
    show_settings(&app, tab).map_err(|e| e.to_string())
}

// An open settings window keeps its unsaved draft; the event only switches the tab.
fn show_settings(app: &AppHandle, tab: SettingsTab) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window("settings") {
        app.emit_to("settings", "settings-tab", tab)?;
        window.unminimize()?;
        window.show()?;
        return window.set_focus();
    }
    let page = match tab {
        SettingsTab::Menubar => "settings.html#menubar",
        SettingsTab::Claude => "settings.html#claude",
        SettingsTab::Codex => "settings.html#codex",
    };
    let builder = WebviewWindowBuilder::new(app, "settings", WebviewUrl::App(page.into()))
        .title(t(
            &app.state::<RuntimeState>()
                .coordinator
                .snapshot()
                .settings
                .language,
            "settings",
            &[],
        ))
        .inner_size(420.0, 520.0)
        .resizable(false)
        .maximizable(false)
        .center();
    #[cfg(target_os = "macos")]
    let builder = builder
        .title_bar_style(tauri::TitleBarStyle::Overlay)
        .hidden_title(true);
    builder.build()?.set_focus()
}

const PANEL_WIDTH: f64 = 360.0;

// The panel follows its content like a popover but never grows past the screen.
fn panel_height(content: f64, work_area: f64) -> f64 {
    content.ceil().clamp(160.0, (work_area - 16.0).max(160.0))
}

#[tauri::command]
fn fit_panel(app: AppHandle, height: f64) -> Result<(), String> {
    fit_panel_window(&app, height).map_err(|e| e.to_string())
}

fn fit_panel_window(app: &AppHandle, height: f64) -> tauri::Result<()> {
    let Some(window) = app.get_webview_window("main") else {
        return Ok(());
    };
    let work_area = window.current_monitor()?.map_or(620.0, |monitor| {
        f64::from(monitor.work_area().size.height) / monitor.scale_factor()
    });
    // Restoring the position keeps the top edge under the tray whatever edge set_size pins.
    let position = window.outer_position()?;
    window.set_size(LogicalSize::new(
        PANEL_WIDTH,
        panel_height(height, work_area),
    ))?;
    window.set_position(position)
}

fn show_panel(app: &AppHandle) -> tauri::Result<()> {
    let Some(window) = app.get_webview_window("main") else {
        return Ok(());
    };
    let anchor = app
        .try_state::<RuntimeState>()
        .map(|runtime| {
            runtime
                .panel_tray
                .lock()
                .expect("panel tray anchor")
                .clone()
        })
        .unwrap_or_else(|| "main-tray".into());
    let rect = app
        .tray_by_id(&anchor)
        .and_then(|tray| tray.rect().ok().flatten());
    #[cfg(windows)]
    let rect = rect.or_else(|| {
        [ProviderId::Claude, ProviderId::Codex]
            .into_iter()
            .filter_map(|id| app.tray_by_id(windows_tray::tray_id(id)))
            .find_map(|tray| tray.rect().ok().flatten())
    });
    if let Some(rect) = rect {
        // Tray rectangles returned by Tauri are physical; conversion is a no-op for them.
        let point = rect.position.to_physical::<f64>(1.0);
        let monitor = window
            .monitor_from_point(point.x, point.y)?
            .or(window.primary_monitor()?);
        if let Some(monitor) = monitor {
            let scale = monitor.scale_factor();
            let size = rect.size.to_physical::<f64>(scale);
            let area = monitor.work_area();
            let width = PANEL_WIDTH * scale;
            let height = f64::from(window.outer_size()?.height);
            let left = f64::from(area.position.x);
            let top = f64::from(area.position.y);
            let right = left + f64::from(area.size.width);
            let bottom = top + f64::from(area.size.height);
            let x =
                (point.x + size.width / 2.0 - width / 2.0).clamp(left, (right - width).max(left));
            let y = (point.y + size.height + 4.0 * scale).clamp(top, (bottom - height).max(top));
            window.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32))?;
        }
    } else {
        window.center()?;
    }
    window.show()?;
    window.set_focus()?;
    if let Some(runtime) = app.try_state::<RuntimeState>() {
        runtime.coordinator.note_activity();
    }
    Ok(())
}

fn finish_panel_click(app: &AppHandle) -> tauri::Result<()> {
    let Some(window) = app.get_webview_window("main") else {
        return Ok(());
    };
    // Use visibility at mouse-down: AppKit may blur/hide the panel before mouse-up.
    let pressed_visible = app
        .state::<RuntimeState>()
        .panel_click
        .lock()
        .expect("panel click")
        .take();
    let was_visible = match pressed_visible {
        Some(visible) => visible,
        None => window.is_visible()?,
    };
    if was_visible {
        window.hide()
    } else {
        show_panel(app)
    }
}

fn left_click_on_tray(app: &AppHandle) -> bool {
    #[cfg(windows)]
    {
        use windows_sys::Win32::{
            Foundation::POINT,
            UI::{
                Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON},
                WindowsAndMessaging::GetCursorPos,
            },
        };
        let mut point = POINT { x: 0, y: 0 };
        // Both native cursor positions and Tauri tray rectangles are physical pixels.
        if unsafe { GetAsyncKeyState(i32::from(VK_LBUTTON)) } < 0
            && unsafe { GetCursorPos(&mut point) } != 0
        {
            for id in [ProviderId::Claude, ProviderId::Codex] {
                if let Some(rect) = app
                    .tray_by_id(windows_tray::tray_id(id))
                    .and_then(|tray| tray.rect().ok().flatten())
                {
                    let origin = rect.position.to_physical::<f64>(1.0);
                    let size = rect.size.to_physical::<f64>(1.0);
                    if f64::from(point.x) >= origin.x
                        && f64::from(point.x) < origin.x + size.width
                        && f64::from(point.y) >= origin.y
                        && f64::from(point.y) < origin.y + size.height
                    {
                        return true;
                    }
                }
            }
        }
    }
    #[cfg(target_os = "macos")]
    if objc2_app_kit::NSEvent::pressedMouseButtons() & 1 != 0 {
        if let Some(tray) = app.tray_by_id("main-tray") {
            return tray
                .with_inner_tray_icon(|inner| {
                    let Some(mtm) = objc2::MainThreadMarker::new() else {
                        return false;
                    };
                    let Some(window) = inner
                        .ns_status_item()
                        .and_then(|item| item.button(mtm))
                        .and_then(|button| button.window())
                    else {
                        return false;
                    };
                    // Both values use AppKit screen points, including mixed-scale monitors.
                    let point = objc2_app_kit::NSEvent::mouseLocation();
                    let rect = window.frame();
                    point.x >= rect.origin.x
                        && point.x < rect.origin.x + rect.size.width
                        && point.y >= rect.origin.y
                        && point.y < rect.origin.y + rect.size.height
                })
                .unwrap_or(false);
        }
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    let _ = app;
    false
}

#[cfg(not(windows))]
fn tray_image() -> Image<'static> {
    // Retina alpha-mask; macOS applies its own tint in light and dark menu bars.
    #[cfg(target_os = "macos")]
    {
        tauri::include_image!("icons/tray/44x44.png")
    }
    #[cfg(not(target_os = "macos"))]
    {
        tauri::include_image!("icons/32x32.png")
    }
}

fn app_context() -> tauri::Context<tauri::Wry> {
    tauri::generate_context!()
}

fn startup_state(settings: Settings) -> AppState {
    AppState::disconnected(settings)
}

pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            // Return from the Windows activation message before touching the webview.
            let handle = app.clone();
            let _ = app.run_on_main_thread(move || {
                if let Err(error) = show_panel(&handle) {
                    eprintln!("Could not show panel: {error}");
                }
            });
        }))
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let settings_path = app.path().app_config_dir()?.join("settings.json");
            let (settings, notice) = storage::load(&settings_path);
            let language = settings.language.clone();
            let mut state = startup_state(settings);
            state.notice = notice;
            let publisher_app = app.handle().clone();
            #[cfg(target_os = "macos")]
            app.manage(native_notifications::NotificationService::new());
            let notify_app = app.handle().clone();
            let coordinator = Coordinator::new_with_notifier(
                state,
                move |state| {
                    let _ = publisher_app.emit("state-changed", state);
                    let handle = publisher_app.clone();
                    let _ = publisher_app.run_on_main_thread(move || {
                        if !handle.state::<RuntimeState>().coordinator.is_stopping() {
                            let _ = update_tray(&handle);
                        }
                    });
                },
                move |alert| {
                    #[cfg(target_os = "macos")]
                    notify_app
                        .state::<native_notifications::NotificationService>()
                        .send(alert);
                    #[cfg(not(target_os = "macos"))]
                    let _ = (&notify_app, alert);
                },
            )
            .with_persistence(app.path().app_config_dir()?);

            let open = MenuItem::with_id(
                app,
                "open",
                t(&language, "openPanel", &[]),
                true,
                None::<&str>,
            )?;
            let status = MenuItem::with_id(
                app,
                "status",
                t(&language, "noAccounts", &[]),
                false,
                None::<&str>,
            )?;
            let claude = MenuItem::with_id(
                app,
                "claude",
                t(&language, "connect", &[("provider", "Claude")]),
                true,
                None::<&str>,
            )?;
            let codex = MenuItem::with_id(
                app,
                "codex",
                t(&language, "connect", &[("provider", "Codex")]),
                true,
                None::<&str>,
            )?;
            let cancel = MenuItem::with_id(
                app,
                "cancel",
                t(&language, "cancelRefresh", &[]),
                false,
                None::<&str>,
            )?;
            let separator = PredefinedMenuItem::separator(app)?;
            let quit =
                MenuItem::with_id(app, "quit", t(&language, "quit", &[]), true, None::<&str>)?;
            let menu = Menu::with_items(
                app,
                &[&status, &separator, &open, &claude, &codex, &cancel, &quit],
            )?;
            app.manage(RuntimeState {
                coordinator: coordinator.clone(),
                settings_path,
                tray: TrayItems {
                    open,
                    quit,
                    claude,
                    codex,
                    cancel,
                    status,
                },
                panel_click: Mutex::new(None),
                panel_tray: Mutex::new({
                    #[cfg(windows)]
                    {
                        windows_tray::tray_id(ProviderId::Claude).into()
                    }
                    #[cfg(not(windows))]
                    {
                        "main-tray".into()
                    }
                }),
                #[cfg(windows)]
                windows_icons: Mutex::new([None, None]),
                #[cfg(windows)]
                settings_transition: tauri::async_runtime::Mutex::new(()),
                exit: ExitGate::default(),
                #[cfg(unix)]
                signals: Mutex::new(None),
                background: Mutex::new(None),
                #[cfg(target_os = "macos")]
                rendering: Mutex::new(None),
            });
            // Install handlers only after their callback's managed state exists.
            #[cfg(unix)]
            {
                let signals = listen_for_shutdown(app.handle().clone())?;
                *app.state::<RuntimeState>()
                    .signals
                    .lock()
                    .expect("shutdown signals") = Some(signals);
            }
            // One menu handler serves both Windows icons; actions must fire only once.
            app.on_menu_event(|app, event| match event.id.as_ref() {
                "open" => {
                    if let Err(e) = show_panel(app) {
                        eprintln!("Panel error: {e}");
                    }
                }
                "claude" | "codex" | "cancel" => {
                    let coordinator = app.state::<RuntimeState>().coordinator.clone();
                    let action = event.id.as_ref().to_owned();
                    tauri::async_runtime::spawn(async move {
                        let result = match action.as_str() {
                            "cancel" => coordinator.cancel_read().await,
                            "claude"
                                if coordinator.snapshot().providers.iter().any(|p| {
                                    p.id == ProviderId::Claude
                                        && p.status == ProviderStatus::SignedOut
                                }) =>
                            {
                                coordinator.sign_in_claude().await
                            }
                            "claude" => coordinator.read_quota(ProviderId::Claude).await,
                            _ => coordinator.read_quota(ProviderId::Codex).await,
                        };
                        if result.is_err() {
                            eprintln!("Manual action unavailable.");
                        }
                    });
                }
                "quit" => app.exit(0),
                _ => {}
            });
            #[cfg(windows)]
            let tray_ids = [
                windows_tray::tray_id(ProviderId::Claude),
                windows_tray::tray_id(ProviderId::Codex),
            ];
            #[cfg(not(windows))]
            let tray_ids = ["main-tray"];
            for tray_id in tray_ids {
                #[cfg(windows)]
                let image = {
                    let id = if tray_id == windows_tray::tray_id(ProviderId::Claude) {
                        ProviderId::Claude
                    } else {
                        ProviderId::Codex
                    };
                    Image::new_owned(
                        windows_tray::Indicator::new(&coordinator.snapshot(), id, now_ms())
                            .render(windows_tray::ICON_SIZE),
                        windows_tray::ICON_SIZE,
                        windows_tray::ICON_SIZE,
                    )
                };
                #[cfg(not(windows))]
                let image = tray_image();
                TrayIconBuilder::with_id(tray_id)
                    .icon(image)
                    .icon_as_template(cfg!(target_os = "macos"))
                    .menu(&menu)
                    .show_menu_on_left_click(false)
                    .on_tray_icon_event(|tray, event| {
                        let app = tray.app_handle();
                        match event {
                            TrayIconEvent::Click {
                                button: MouseButton::Left,
                                button_state: MouseButtonState::Down,
                                ..
                            } => {
                                *app.state::<RuntimeState>()
                                    .panel_tray
                                    .lock()
                                    .expect("panel tray anchor") = tray.id().as_ref().to_owned();
                                let visible = app
                                    .get_webview_window("main")
                                    .and_then(|window| window.is_visible().ok())
                                    .unwrap_or(false);
                                *app.state::<RuntimeState>()
                                    .panel_click
                                    .lock()
                                    .expect("panel click") = Some(visible);
                            }
                            TrayIconEvent::Click {
                                button: MouseButton::Left,
                                button_state: MouseButtonState::Up,
                                ..
                            } => {
                                if let Err(e) = finish_panel_click(app) {
                                    eprintln!("Panel error: {e}");
                                }
                            }
                            _ => {}
                        }
                    })
                    .build(app)?;
            }
            update_tray(app.handle()).map_err(std::io::Error::other)?;

            // One owned ticker serves hidden-panel refresh and menu-bar expiry.
            {
                let handle = app.handle().clone();
                let timer_coordinator = coordinator.clone();
                let (cancel, mut receiver) = tauri::async_runtime::channel(1);
                let task = tauri::async_runtime::spawn(async move {
                    let mut ticks = tokio::time::interval(Duration::from_secs(5));
                    ticks.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                    loop {
                        tokio::select! {biased;_=receiver.recv()=>break,_=ticks.tick()=>{}}
                        if timer_coordinator.is_stopping() {
                            break;
                        }
                        let _ = timer_coordinator.refresh_due().await;
                        let update_handle = handle.clone();
                        if handle
                            .run_on_main_thread(move || {
                                if !update_handle
                                    .state::<RuntimeState>()
                                    .coordinator
                                    .is_stopping()
                                {
                                    let _ = update_tray(&update_handle);
                                }
                            })
                            .is_err()
                        {
                            break;
                        }
                    }
                });
                *app.state::<RuntimeState>()
                    .background
                    .lock()
                    .expect("background ticker") = Some(SignalTask {
                    cancel,
                    handle: task,
                });
            }
            Ok(())
        })
        // The panel is reused; the settings window closes for real.
        .on_window_event(|window, event| match event {
            WindowEvent::CloseRequested { api, .. } if window.label() == "main" => {
                api.prevent_close();
                let _ = window.hide();
            }
            WindowEvent::ThemeChanged(_) => {
                if window.app_handle().try_state::<RuntimeState>().is_some() {
                    let _ = update_tray(window.app_handle());
                }
            }
            WindowEvent::Focused(false) if window.label() == "main" => {
                // Preserve visibility if blur arrives even before the tray mouse-down callback.
                // Mouse-up completes this click; other mouse clicks and keyboard blur still hide.
                if !left_click_on_tray(window.app_handle()) {
                    let _ = window.hide();
                }
            }
            _ => {}
        })
        .invoke_handler(tauri::generate_handler![
            get_state,
            save_settings,
            set_refresh_minutes,
            read_quota,
            sign_in_claude,
            cancel_read,
            request_notification_permission,
            test_notification,
            hide_panel,
            open_settings,
            fit_panel
        ])
        .build(app_context())
        .expect("failed to build KnowYourLimits");
    app.run(|app, event| {
        if let tauri::RunEvent::ExitRequested { api, code, .. } = event {
            let runtime = app.state::<RuntimeState>();
            if runtime.exit.ready() {
                return;
            }
            api.prevent_exit();
            runtime.coordinator.request_stop();
            if runtime.exit.begin() {
                let coordinator = runtime.coordinator.clone();
                let handle = app.clone();
                tauri::async_runtime::spawn(async move {
                    {
                        let background = handle
                            .state::<RuntimeState>()
                            .background
                            .lock()
                            .expect("background ticker")
                            .take();
                        if let Some(background) = background {
                            let _ = background.cancel.try_send(());
                            let _ = background.handle.await;
                        }
                    }
                    coordinator.shutdown().await;
                    #[cfg(unix)]
                    {
                        let signals = handle
                            .state::<RuntimeState>()
                            .signals
                            .lock()
                            .expect("shutdown signals")
                            .take();
                        if let Some(signals) = signals {
                            let _ = signals.cancel.try_send(());
                            let _ = signals.handle.await;
                        }
                    }
                    handle.state::<RuntimeState>().exit.finish();
                    handle.exit(code.unwrap_or(0));
                });
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_follows_content_but_stays_on_screen() {
        assert_eq!(panel_height(452.4, 900.0), 453.0);
        assert_eq!(panel_height(1200.0, 900.0), 884.0);
        assert_eq!(panel_height(40.0, 900.0), 160.0);
        assert_eq!(panel_height(400.0, 100.0), 160.0);
    }

    #[test]
    fn exit_gate_prevents_duplicate_shutdown_and_allows_only_final_exit() {
        let gate = ExitGate::default();
        assert!(!gate.ready());
        assert!(gate.begin());
        assert!(!gate.ready());
        assert!(!gate.begin());
        gate.finish();
        assert!(gate.ready());
        assert!(!gate.begin());
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "current_thread")]
    async fn scoped_signal_listener_routes_both_signals_then_awaits_shutdown_cancellation() {
        for code in [130, 143] {
            let (cancel, receiver) = tauri::async_runtime::channel(1);
            let (observed, mut exits) = tauri::async_runtime::channel(1);
            // Synthetic signal futures: never send SIGINT/TERM to the test runner.
            let listener = tokio::spawn(relay_shutdown_signals(
                receiver,
                async move {
                    if code == 143 {
                        Some(())
                    } else {
                        std::future::pending().await
                    }
                },
                async move {
                    if code == 130 {
                        Some(())
                    } else {
                        std::future::pending().await
                    }
                },
                move |code| {
                    observed.try_send(code).unwrap();
                },
            ));
            assert_eq!(exits.recv().await, Some(code));
            assert!(!listener.is_finished());
            cancel.try_send(()).unwrap();
            listener.await.unwrap();
        }
    }
}
