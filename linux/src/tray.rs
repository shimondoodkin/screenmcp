use tokio::sync::{mpsc, watch};
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};
use tracing::{error, info, warn};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::window::WindowId;

use crate::auth::{self, LocalServerEvent};
use crate::config::Config;
use crate::ws::{ConnectionStatus, WsCommand};

/// Custom events sent to the winit event loop.
#[derive(Debug)]
pub enum TrayEvent {
    StatusChanged(ConnectionStatus),
    AuthTokenReceived { token: String, email: String },
    RegistrationResult(Result<bool, String>),
    Quit,
}

/// IDs for menu items.
struct MenuItems {
    about: MenuItem,
    status: MenuItem,
    sign_in: MenuItem,
    sign_out: MenuItem,
    test_connection: MenuItem,
    register_device: MenuItem,
    unregister_device: MenuItem,
    quit: MenuItem,
}

fn create_icon_rgba(connected: bool) -> Vec<u8> {
    let size = 32u32;
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);

    let (r, g, b) = if connected {
        (0x00u8, 0xC8u8, 0x00u8)
    } else {
        (0xC8u8, 0x00u8, 0x00u8)
    };

    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 - size as f32 / 2.0;
            let dy = y as f32 - size as f32 / 2.0;
            let dist = (dx * dx + dy * dy).sqrt();
            let radius = size as f32 / 2.0 - 1.0;

            if dist <= radius {
                rgba.push(r);
                rgba.push(g);
                rgba.push(b);
                rgba.push(0xFF);
            } else {
                rgba.push(0);
                rgba.push(0);
                rgba.push(0);
                rgba.push(0);
            }
        }
    }

    rgba
}

fn create_icon(connected: bool) -> Icon {
    Icon::from_rgba(create_icon_rgba(connected), 32, 32).expect("failed to create icon")
}

fn hex_to_uuid(hex: &str) -> String {
    if hex.len() == 32 && !hex.contains('-') {
        format!(
            "{}-{}-{}-{}-{}",
            &hex[0..8],
            &hex[8..12],
            &hex[12..16],
            &hex[16..20],
            &hex[20..32]
        )
    } else {
        hex.to_string()
    }
}

async fn do_register_device(api_url: &str, token: &str, device_id: &str) -> Result<(), String> {
    let hostname = hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "Linux Desktop".to_string());
    let uuid_id = hex_to_uuid(device_id);

    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{api_url}/api/devices/register"))
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "deviceId": uuid_id,
            "deviceName": hostname,
            "deviceModel": "Linux Desktop",
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
    let uuid_id = hex_to_uuid(device_id);
    let client = reqwest::Client::new();
    let resp = client
        .post(format!("{api_url}/api/devices/delete"))
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({"deviceId": uuid_id}))
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

/// Update menu items based on current config and registration state.
fn update_menu_labels(items: &MenuItems, config: &Config, is_registered: bool) {
    let signed_in = !config.token.is_empty() || !config.email.is_empty();
    let oss_ready = config.opensource_server_enabled
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
    items.register_device.set_enabled(is_signed_in && !is_registered);
    items.unregister_device.set_enabled(is_signed_in && is_registered);
    items.test_connection.set_enabled(is_registered);
}

struct App {
    tray: Option<TrayIcon>,
    menu_items: Option<MenuItems>,
    ws_cmd_tx: mpsc::Sender<WsCommand>,
    status_rx: watch::Receiver<ConnectionStatus>,
    auth_event_rx: Option<mpsc::Receiver<LocalServerEvent>>,
    last_status: ConnectionStatus,
    proxy: EventLoopProxy<TrayEvent>,
    local_port: u16,
    is_registered: bool,
}

impl App {
    fn update_tray_for_status(&mut self, status: &ConnectionStatus) {
        if let Some(ref tray) = self.tray {
            let connected = matches!(status, ConnectionStatus::Connected);
            if let Ok(icon) = Icon::from_rgba(create_icon_rgba(connected), 32, 32) {
                let _ = tray.set_icon(Some(icon));
            }
            let _ = tray.set_tooltip(Some(&format!("ScreenMCP Linux - {status}")));
        }

        if let Some(ref items) = self.menu_items {
            let _ = items.status.set_text(&format!("Status: {status}"));
        }
    }

    fn refresh_labels(&self) {
        let config = Config::load();
        if let Some(ref items) = self.menu_items {
            update_menu_labels(items, &config, self.is_registered);
        }
    }
}

impl ApplicationHandler<TrayEvent> for App {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {
        if self.tray.is_some() {
            return;
        }

        let config = Config::load();
        let oss_ready = config.opensource_server_enabled
            && !config.opensource_user_id.is_empty()
            && !config.opensource_api_url.is_empty();
        let has_token = !config.token.is_empty() || !config.email.is_empty();
        let is_signed_in = has_token || oss_ready;

        // Build menu
        let about = MenuItem::new("About ScreenMCP.com", true, None);
        let status = MenuItem::new("Status: Disconnected", false, None);

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
        let test_connection = MenuItem::new("Test Connection", false, None);
        let register_device = MenuItem::new("Register Device", is_signed_in, None);
        let unregister_device = MenuItem::new("Unregister Device", false, None);
        let quit = MenuItem::new("Quit", true, None);

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
        let _ = menu.append(&quit);

        match TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("ScreenMCP Linux - Disconnected")
            .with_icon(create_icon(false))
            .with_title("ScreenMCP")
            .build()
        {
            Ok(tray) => {
                self.tray = Some(tray);
                info!("tray icon created");
            }
            Err(e) => {
                error!("failed to create tray icon: {e}");
            }
        }

        self.menu_items = Some(MenuItems {
            about,
            status,
            sign_in,
            sign_out,
            test_connection,
            register_device,
            unregister_device,
            quit,
        });

        // Spawn status watcher
        let mut status_rx = self.status_rx.clone();
        let proxy = self.proxy.clone();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(async {
                loop {
                    if status_rx.changed().await.is_err() {
                        break;
                    }
                    let status = status_rx.borrow().clone();
                    let _ = proxy.send_event(TrayEvent::StatusChanged(status));
                }
            });
        });

        // Spawn auth event listener
        if let Some(auth_rx) = self.auth_event_rx.take() {
            let proxy = self.proxy.clone();
            std::thread::spawn(move || {
                let rt = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .unwrap();
                rt.block_on(async move {
                    let mut rx = auth_rx;
                    while let Some(event) = rx.recv().await {
                        match event {
                            LocalServerEvent::TokenReceived { token, email } => {
                                let _ = proxy.send_event(TrayEvent::AuthTokenReceived { token, email });
                            }
                        }
                    }
                });
            });
        }

        // Spawn menu event handler
        let ws_tx = self.ws_cmd_tx.clone();
        let proxy2 = self.proxy.clone();
        let local_port = self.local_port;
        let menu_ref = self.menu_items.as_ref().unwrap();
        let about_id = menu_ref.about.id().clone();
        let sign_in_id = menu_ref.sign_in.id().clone();
        let sign_out_id = menu_ref.sign_out.id().clone();
        let test_connection_id = menu_ref.test_connection.id().clone();
        let register_id = menu_ref.register_device.id().clone();
        let unregister_id = menu_ref.unregister_device.id().clone();
        let quit_id = menu_ref.quit.id().clone();

        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            loop {
                if let Ok(event) = MenuEvent::receiver().recv() {
                    if event.id() == &about_id {
                        info!("menu: about clicked");
                        let _ = open::that("https://screenmcp.com");
                    } else if event.id() == &sign_in_id {
                        info!("menu: sign in clicked");
                        let config = Config::load();
                        auth::open_google_sign_in(local_port, &config.api_url);
                    } else if event.id() == &sign_out_id {
                        info!("menu: sign out clicked");
                        let mut cfg = Config::load();
                        cfg.token.clear();
                        cfg.email.clear();
                        if let Err(e) = cfg.save() {
                            error!("failed to save config: {e}");
                        }
                        // Send a registration result to reset is_registered and refresh labels
                        let _ = proxy2.send_event(TrayEvent::RegistrationResult(Ok(false)));
                    } else if event.id() == &test_connection_id {
                        info!("menu: test connection clicked");
                        let new_config = Config::load();
                        let _ = rt.block_on(ws_tx.send(WsCommand::UpdateConfig(new_config)));
                        let _ = rt.block_on(ws_tx.send(WsCommand::Connect));
                    } else if event.id() == &register_id {
                        info!("menu: register device clicked");
                        let cfg = Config::load();
                        let api_url = cfg.effective_api_url().to_string();
                        let token = cfg.effective_token().to_string();
                        let did = cfg.device_id.clone();
                        let proxy3 = proxy2.clone();
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
                                let _ = proxy3.send_event(TrayEvent::RegistrationResult(res));
                            });
                        });
                    } else if event.id() == &unregister_id {
                        info!("menu: unregister device clicked");
                        let cfg = Config::load();
                        let api_url = cfg.effective_api_url().to_string();
                        let token = cfg.effective_token().to_string();
                        let did = cfg.device_id.clone();
                        let proxy3 = proxy2.clone();
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
                                let _ = proxy3.send_event(TrayEvent::RegistrationResult(res));
                            });
                        });
                    } else if event.id() == &quit_id {
                        info!("menu: quit clicked");
                        let _ = rt.block_on(ws_tx.send(WsCommand::Shutdown));
                        let _ = proxy2.send_event(TrayEvent::Quit);
                    }
                }
            }
        });
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: TrayEvent) {
        match event {
            TrayEvent::StatusChanged(status) => {
                if status != self.last_status {
                    info!("status changed: {} -> {}", self.last_status, status);
                    self.update_tray_for_status(&status);
                    self.last_status = status;
                }
            }
            TrayEvent::AuthTokenReceived { token, email } => {
                info!("auth token received, email: {email}");
                self.refresh_labels();

                // Push updated config to WS manager
                let new_config = Config::load();
                let _ = self.ws_cmd_tx.try_send(WsCommand::UpdateConfig(new_config));

                let _ = token;
            }
            TrayEvent::RegistrationResult(result) => {
                match result {
                    Ok(registered) => {
                        self.is_registered = registered;
                        if registered {
                            info!("device registered — updating menu");
                        } else {
                            info!("device unregistered — updating menu");
                        }
                        self.refresh_labels();
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
            TrayEvent::Quit => {
                info!("quitting application");
                event_loop.exit();
            }
        }
    }

    fn window_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        _event: WindowEvent,
    ) {
        // No windows, just tray
    }
}

/// Build and run the system tray event loop. This blocks the main thread.
pub fn run_tray(
    ws_cmd_tx: mpsc::Sender<WsCommand>,
    status_rx: watch::Receiver<ConnectionStatus>,
    local_port: u16,
    auth_event_rx: mpsc::Receiver<LocalServerEvent>,
) {
    let event_loop: EventLoop<TrayEvent> = EventLoop::with_user_event()
        .build()
        .expect("failed to build event loop");

    let proxy = event_loop.create_proxy();

    let mut app = App {
        tray: None,
        menu_items: None,
        ws_cmd_tx,
        status_rx,
        auth_event_rx: Some(auth_event_rx),
        last_status: ConnectionStatus::Disconnected,
        proxy,
        local_port,
        is_registered: false,
    };

    event_loop.run_app(&mut app).expect("event loop failed");
}
