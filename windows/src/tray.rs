use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use std::sync::mpsc as std_mpsc;

use eframe::egui;
use tokio::sync::{mpsc, watch};
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};
use tracing::{error, info, warn};

use crate::auth::LocalServerEvent;
use crate::config::Config;
use crate::login_window::LoginState;
use crate::test_window::TestState;
use crate::ws::{ConnectionStatus, WsCommand};

/// Result of an async registration operation.
/// Ok(true) = registered, Ok(false) = unregistered, Err = error message.
type RegistrationResult = Option<Result<bool, String>>;

/// IDs for menu items.
struct MenuItems {
    status: MenuItem,
    sign_in: MenuItem,
    sign_out: MenuItem,
    test_connection: MenuItem,
    register_device: MenuItem,
    unregister_device: MenuItem,
    test_window: MenuItem,
    about: MenuItem,
    quit: MenuItem,
}

const ICON_GREEN: &[u8] = include_bytes!("../assets/icon-connected.ico");
const ICON_RED: &[u8] = include_bytes!("../assets/icon-disconnected.ico");

fn load_icon_from_ico(data: &[u8]) -> Icon {
    let img = image::load_from_memory(data).expect("failed to decode ico");
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    Icon::from_rgba(rgba.into_raw(), w, h).expect("failed to create icon")
}

fn create_icon(connected: bool) -> Icon {
    load_icon_from_ico(if connected { ICON_GREEN } else { ICON_RED })
}

/// Update sign-in and registration menu items based on current config and registration state.
fn update_menu_labels(items: &MenuItems, config: &Config, is_registered: bool) {
    let signed_in = !config.token.is_empty() || !config.email.is_empty();
    let oss_mode = config.opensource_server_enabled;
    let oss_ready = oss_mode
        && !config.opensource_user_id.is_empty()
        && !config.opensource_api_url.is_empty();
    let is_signed_in = signed_in || oss_ready;

    if is_signed_in {
        let label = if oss_ready {
            format!("Signed in: {}", config.opensource_user_id)
        } else if !config.email.is_empty() {
            format!("Signed in: {}", config.email)
        } else {
            format!("Signed in: {}...", &config.token[..config.token.len().min(11)])
        };
        let _ = items.sign_in.set_text(&label);
        items.sign_in.set_enabled(false);
    } else {
        let _ = items.sign_in.set_text("Sign in");
        items.sign_in.set_enabled(true);
    }

    items.sign_out.set_enabled(is_signed_in);

    // Register enabled when signed in but NOT registered
    items.register_device.set_enabled(is_signed_in && !is_registered);
    // Unregister enabled when signed in AND registered
    items.unregister_device.set_enabled(is_signed_in && is_registered);
    // Test connection only available when registered
    items.test_connection.set_enabled(is_registered);
}

async fn do_register_device(api_url: &str, token: &str, device_id: &str) -> Result<(), String> {
    let hostname = hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "Windows Desktop".to_string());

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{api_url}/api/devices/register"))
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "deviceId": device_id,
            "deviceName": hostname,
            "deviceModel": "Windows Desktop",
            "role": "phone"
        }))
        .send()
        .await
        .map_err(|e| format!("register request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("register failed ({status}): {body}"));
    }

    Ok(())
}

async fn do_unregister_device(api_url: &str, token: &str, device_id: &str) -> Result<(), String> {
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{api_url}/api/devices/unregister"))
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({"deviceId": device_id}))
        .send()
        .await
        .map_err(|e| format!("unregister request failed: {e}"))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(format!("unregister failed ({status}): {body}"));
    }

    Ok(())
}

// ── Main eframe App ──

pub struct TrayApp {
    // Tray icon (kept alive for the duration of the app)
    _tray: Option<TrayIcon>,
    menu_items: Option<MenuItems>,

    // Channels
    ws_cmd_tx: mpsc::Sender<WsCommand>,
    status_rx: watch::Receiver<ConnectionStatus>,
    auth_event_rx: Option<mpsc::Receiver<LocalServerEvent>>,
    menu_event_rx: std_mpsc::Receiver<MenuEvent>,

    // State
    local_port: u16,
    last_status: ConnectionStatus,
    is_registered: Arc<AtomicBool>,

    // Async registration result: Ok(true)=registered, Ok(false)=unregistered, Err=error
    registration_result: Arc<Mutex<RegistrationResult>>,

    // Viewport flags
    show_login: Arc<AtomicBool>,
    show_test: Arc<AtomicBool>,

    // Focus flags — set when menu clicked, consumed by viewport to bring to front
    focus_login: Arc<AtomicBool>,
    focus_test: Arc<AtomicBool>,

    // Shared viewport state
    login_state: Arc<Mutex<LoginState>>,
    test_state: Arc<Mutex<TestState>>,

    // Quit flag
    should_quit: bool,
}

impl TrayApp {
    pub fn new(
        _cc: &eframe::CreationContext<'_>,
        ws_cmd_tx: mpsc::Sender<WsCommand>,
        status_rx: watch::Receiver<ConnectionStatus>,
        local_port: u16,
        auth_event_rx: mpsc::Receiver<LocalServerEvent>,
    ) -> Self {
        let config = Config::load();

        // ── Build tray menu ──
        let about = MenuItem::new("About ScreenMCP.com", true, None);
        let status = MenuItem::new("Status: Disconnected", false, None);
        let oss_ready = config.opensource_server_enabled
            && !config.opensource_user_id.is_empty()
            && !config.opensource_api_url.is_empty();
        let has_token = !config.token.is_empty() || !config.email.is_empty();
        let is_signed_in = has_token || oss_ready;

        let sign_in_label = if is_signed_in {
            if oss_ready {
                format!("Signed in: {}", config.opensource_user_id)
            } else if !config.email.is_empty() {
                format!("Signed in: {}", config.email)
            } else {
                format!("Signed in: {}...", &config.token[..config.token.len().min(11)])
            }
        } else {
            "Sign in".to_string()
        };
        let sign_in = MenuItem::new(&sign_in_label, !is_signed_in, None);
        let sign_out = MenuItem::new("Sign Out", is_signed_in, None);

        // Initially not registered: test connection disabled, register enabled, unregister disabled
        let test_connection = MenuItem::new("Test Connection", false, None);
        let register_device = MenuItem::new("Register Device", is_signed_in, None);
        let unregister_device = MenuItem::new("Unregister Device", false, None);
        let test_window = MenuItem::new("Test Window", true, None);
        let quit = MenuItem::new("Quit", true, None);
        let quit_id = quit.id().clone();

        let menu = Menu::new();
        let _ = menu.append(&about);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&status);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&sign_in);
        let _ = menu.append(&sign_out);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&test_connection);
        let _ = menu.append(&register_device);
        let _ = menu.append(&unregister_device);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&test_window);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&quit);

        let tray = match TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("ScreenMCP Windows - Disconnected")
            .with_icon(create_icon(false))
            .with_title("ScreenMCP")
            .build()
        {
            Ok(tray) => {
                info!("tray icon created");
                Some(tray)
            }
            Err(e) => {
                error!("failed to create tray icon: {e}");
                None
            }
        };

        let menu_items = MenuItems {
            status,
            sign_in,
            sign_out,
            test_connection,
            register_device,
            unregister_device,
            test_window,
            about,
            quit,
        };

        let (menu_event_tx, menu_event_rx) = std_mpsc::channel::<MenuEvent>();
        let ws_cmd_tx_for_handler = ws_cmd_tx.clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            if event.id == quit_id {
                info!("menu: quit clicked");
                let _ = ws_cmd_tx_for_handler.try_send(WsCommand::Shutdown);
                std::process::exit(0);
            }
            let _ = menu_event_tx.send(event);
        }));

        // Auto-open login window if not signed in
        let cloud_signed_in = !config.token.is_empty();
        let oss_signed_in = config.opensource_server_enabled
            && !config.opensource_user_id.is_empty()
            && !config.opensource_api_url.is_empty();
        let show_login = Arc::new(AtomicBool::new(!cloud_signed_in && !oss_signed_in));

        let login_state = Arc::new(Mutex::new(LoginState::new(local_port, config.api_url.clone())));

        let focus_login = Arc::new(AtomicBool::new(!cloud_signed_in && !oss_signed_in));

        Self {
            _tray: tray,
            menu_items: Some(menu_items),
            ws_cmd_tx,
            status_rx,
            auth_event_rx: Some(auth_event_rx),
            menu_event_rx,
            local_port,
            last_status: ConnectionStatus::Disconnected,
            is_registered: Arc::new(AtomicBool::new(false)),
            registration_result: Arc::new(Mutex::new(None)),
            show_login,
            show_test: Arc::new(AtomicBool::new(false)),
            focus_login,
            focus_test: Arc::new(AtomicBool::new(false)),
            login_state,
            test_state: Arc::new(Mutex::new(TestState::new())),
            should_quit: false,
        }
    }

    fn update_tray_for_status(&self, status: &ConnectionStatus) {
        if let Some(ref tray) = self._tray {
            let connected = matches!(status, ConnectionStatus::Connected);
            let _ = tray.set_icon(Some(create_icon(connected)));
            let _ = tray.set_tooltip(Some(&format!("ScreenMCP Windows - {status}")));
        }
        if let Some(ref items) = self.menu_items {
            let _ = items.status.set_text(&format!("Status: {status}"));
        }
    }

    fn refresh_labels(&self) {
        let config = Config::load();
        if let Some(ref items) = self.menu_items {
            update_menu_labels(items, &config, self.is_registered.load(Ordering::SeqCst));
        }
    }

    fn handle_menu_events(&mut self) {
        while let Ok(event) = self.menu_event_rx.try_recv() {
            let items = match &self.menu_items {
                Some(items) => items,
                None => continue,
            };

            if event.id() == items.sign_in.id() {
                info!("menu: sign in clicked");
                self.show_login.store(true, Ordering::SeqCst);
                self.focus_login.store(true, Ordering::SeqCst);
                // Re-initialize login state
                let config = Config::load();
                *self.login_state.lock().unwrap() =
                    LoginState::new(self.local_port, config.api_url.clone());
            } else if event.id() == items.sign_out.id() {
                info!("menu: sign out clicked");
                let mut cfg = Config::load();
                cfg.token.clear();
                cfg.email.clear();
                if let Err(e) = cfg.save() {
                    error!("failed to save config: {e}");
                }
                // Reset registration state on sign out
                self.is_registered.store(false, Ordering::SeqCst);
                self.refresh_labels();
            } else if event.id() == items.test_window.id() {
                info!("menu: test window clicked");
                self.show_test.store(true, Ordering::SeqCst);
                self.focus_test.store(true, Ordering::SeqCst);
            } else if event.id() == items.test_connection.id() {
                info!("menu: test connection clicked");
                let new_config = Config::load();
                let _ = self.ws_cmd_tx.try_send(WsCommand::UpdateConfig(new_config));
                let _ = self.ws_cmd_tx.try_send(WsCommand::Connect);
            } else if event.id() == items.register_device.id() {
                info!("menu: register device clicked");
                let cfg = Config::load();
                let api_url = cfg.effective_api_url().to_string();
                let token = cfg.effective_token().to_string();
                let did = cfg.device_id.clone();
                let result = self.registration_result.clone();
                std::thread::spawn(move || {
                    let rt = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .unwrap();
                    rt.block_on(async {
                        let res = match do_register_device(&api_url, &token, &did).await {
                            Ok(()) => {
                                info!("device registered successfully");
                                Ok(true)
                            }
                            Err(e) => {
                                error!("device registration failed: {e}");
                                Err(e)
                            }
                        };
                        *result.lock().unwrap() = Some(res);
                    });
                });
            } else if event.id() == items.unregister_device.id() {
                info!("menu: unregister device clicked");
                let cfg = Config::load();
                let api_url = cfg.effective_api_url().to_string();
                let token = cfg.effective_token().to_string();
                let did = cfg.device_id.clone();
                let result = self.registration_result.clone();
                std::thread::spawn(move || {
                    let rt = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .unwrap();
                    rt.block_on(async {
                        let res = match do_unregister_device(&api_url, &token, &did).await {
                            Ok(()) => {
                                info!("device unregistered successfully");
                                Ok(false)
                            }
                            Err(e) => {
                                error!("device unregistration failed: {e}");
                                Err(e)
                            }
                        };
                        *result.lock().unwrap() = Some(res);
                    });
                });
            } else if event.id() == items.about.id() {
                info!("menu: about clicked");
                let _ = open::that("https://screenmcp.com");
            } else if event.id() == items.quit.id() {
                info!("menu: quit clicked");
                let _ = self.ws_cmd_tx.try_send(WsCommand::Shutdown);
                self.should_quit = true;
            }
        }
    }

    fn handle_status_changes(&mut self) {
        if self.status_rx.has_changed().unwrap_or(false) {
            let status = self.status_rx.borrow_and_update().clone();
            if status != self.last_status {
                info!("status changed: {} -> {}", self.last_status, status);
                self.update_tray_for_status(&status);
                self.last_status = status;
            }
        }
    }

    fn handle_registration_results(&mut self) {
        let result = self.registration_result.lock().unwrap().take();
        if let Some(result) = result {
            match result {
                Ok(registered) => {
                    self.is_registered.store(registered, Ordering::SeqCst);
                    if registered {
                        info!("device registered — updating menu");
                    } else {
                        info!("device unregistered — updating menu");
                    }
                    self.refresh_labels();
                    // Update status text
                    if let Some(ref items) = self.menu_items {
                        let msg = if registered {
                            "Status: Registered"
                        } else {
                            "Status: Unregistered"
                        };
                        let _ = items.status.set_text(msg);
                    }
                }
                Err(e) => {
                    warn!("registration operation failed: {e}");
                    if let Some(ref items) = self.menu_items {
                        let _ = items.status.set_text(&format!("Error: {e}"));
                    }
                }
            }
        }
    }

    fn handle_auth_events(&mut self) {
        // Collect events first to avoid borrow conflicts
        let mut events = Vec::new();
        if let Some(ref mut rx) = self.auth_event_rx {
            while let Ok(event) = rx.try_recv() {
                events.push(event);
            }
        }

        for event in events {
            match event {
                LocalServerEvent::TokenReceived { token, email } => {
                    info!("auth token received, email: {email}");
                    self.refresh_labels();

                    // Update login viewport to show signed-in state
                    {
                        let mut state = self.login_state.lock().unwrap();
                        state.signed_in = true;
                        state.signed_in_email = if !email.is_empty() {
                            email.clone()
                        } else {
                            format!("{}...", &token[..token.len().min(11)])
                        };
                        state.status = "Signed in".to_string();
                    }

                    // Push updated config to WS manager
                    let new_config = Config::load();
                    let _ = self.ws_cmd_tx.try_send(WsCommand::UpdateConfig(new_config));

                    let _ = token;
                }
            }
        }
    }
}

impl eframe::App for TrayApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Poll periodically for tray events, status changes, auth events
        ctx.request_repaint_after(Duration::from_millis(200));

        // Handle events
        self.handle_menu_events();
        self.handle_status_changes();
        self.handle_auth_events();
        self.handle_registration_results();

        if self.should_quit {
            self._tray.take();
            self.menu_items.take();
            std::process::exit(0);
        }

        // ── Login viewport ──
        if self.show_login.load(Ordering::SeqCst) {
            let state = self.login_state.clone();
            let show = self.show_login.clone();
            let focus = self.focus_login.clone();
            ctx.show_viewport_deferred(
                egui::ViewportId::from_hash_of("login_window"),
                egui::ViewportBuilder::default()
                    .with_title("ScreenMCP")
                    .with_inner_size([420.0, 500.0]),
                move |ctx, _class| {
                    if ctx.input(|i| i.viewport().close_requested()) {
                        show.store(false, Ordering::SeqCst);
                        return;
                    }
                    // Bring to front when requested
                    if focus.swap(false, Ordering::SeqCst) {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                    }
                    let mut s = state.lock().unwrap();
                    let should_close = s.render(ctx);
                    if should_close {
                        show.store(false, Ordering::SeqCst);
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                },
            );
        }

        // ── Test viewport ──
        if self.show_test.load(Ordering::SeqCst) {
            let state = self.test_state.clone();
            let show = self.show_test.clone();
            let focus = self.focus_test.clone();
            ctx.show_viewport_deferred(
                egui::ViewportId::from_hash_of("test_window"),
                egui::ViewportBuilder::default()
                    .with_title("ScreenMCP Test Window")
                    .with_inner_size([640.0, 800.0]),
                move |ctx, _class| {
                    if ctx.input(|i| i.viewport().close_requested()) {
                        show.store(false, Ordering::SeqCst);
                        return;
                    }
                    // Bring to front when requested
                    if focus.swap(false, Ordering::SeqCst) {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                    }
                    let mut s = state.lock().unwrap();
                    s.render(ctx);
                },
            );
        }

        // Main viewport is hidden — no UI to draw
    }
}

/// Build and run the system tray event loop. This blocks the main thread.
pub fn run_tray(
    ws_cmd_tx: mpsc::Sender<WsCommand>,
    status_rx: watch::Receiver<ConnectionStatus>,
    local_port: u16,
    auth_event_rx: mpsc::Receiver<LocalServerEvent>,
) {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_visible(false)
            .with_inner_size([1.0, 1.0])
            .with_taskbar(false),
        ..Default::default()
    };

    if let Err(e) = eframe::run_native(
        "ScreenMCP",
        options,
        Box::new(move |cc| {
            // Set font styles for all viewports
            let mut style = (*cc.egui_ctx.style()).clone();
            style
                .text_styles
                .insert(egui::TextStyle::Body, egui::FontId::proportional(15.0));
            style
                .text_styles
                .insert(egui::TextStyle::Button, egui::FontId::proportional(15.0));
            style
                .text_styles
                .insert(egui::TextStyle::Monospace, egui::FontId::monospace(14.0));
            style
                .text_styles
                .insert(egui::TextStyle::Small, egui::FontId::proportional(12.0));
            style
                .text_styles
                .insert(egui::TextStyle::Heading, egui::FontId::proportional(24.0));
            cc.egui_ctx.set_style(style);

            Ok(Box::new(TrayApp::new(
                cc,
                ws_cmd_tx,
                status_rx,
                local_port,
                auth_event_rx,
            )))
        }),
    ) {
        error!("eframe error: {e}");
    }
}
