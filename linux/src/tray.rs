use tokio::sync::{mpsc, watch};
use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};
use tracing::{error, info};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::window::WindowId;

use crate::config::Config;
use crate::ws::{ConnectionStatus, WsCommand};

/// Custom events sent to the winit event loop.
#[derive(Debug)]
pub enum TrayEvent {
    StatusChanged(ConnectionStatus),
    /// Update opensource menu labels after a config change (enabled, user_id, api_url)
    UpdateOpensourceMenu(bool, String, String),
    Quit,
}

/// IDs for menu items.
struct MenuItems {
    connect: MenuItem,
    disconnect: MenuItem,
    status: MenuItem,
    #[allow(dead_code)]
    settings: MenuItem,
    opensource_toggle: MenuItem,
    opensource_user_id: MenuItem,
    opensource_api_url: MenuItem,
    #[allow(dead_code)]
    quit: MenuItem,
}

/// Create RGBA bytes for a simple colored circle icon (green or red).
fn create_icon_rgba(connected: bool) -> Vec<u8> {
    let size = 32u32;
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);

    let (r, g, b) = if connected {
        (0x00u8, 0xC8u8, 0x00u8) // green
    } else {
        (0xC8u8, 0x00u8, 0x00u8) // red
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

/// Create an Icon from a connected/disconnected state.
fn create_icon(connected: bool) -> Icon {
    Icon::from_rgba(create_icon_rgba(connected), 32, 32).expect("failed to create icon")
}

struct App {
    tray: Option<TrayIcon>,
    menu_items: Option<MenuItems>,
    ws_cmd_tx: mpsc::Sender<WsCommand>,
    status_rx: watch::Receiver<ConnectionStatus>,
    last_status: ConnectionStatus,
    proxy: EventLoopProxy<TrayEvent>,
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
            match status {
                ConnectionStatus::Connected => {
                    items.connect.set_enabled(false);
                    items.disconnect.set_enabled(true);
                }
                ConnectionStatus::Disconnected | ConnectionStatus::Error(_) => {
                    items.connect.set_enabled(true);
                    items.disconnect.set_enabled(false);
                }
                ConnectionStatus::Connecting | ConnectionStatus::Reconnecting => {
                    items.connect.set_enabled(false);
                    items.disconnect.set_enabled(true);
                }
            }
        }
    }

    fn update_opensource_menu(&mut self, enabled: bool, user_id: &str, api_url: &str) {
        if let Some(ref items) = self.menu_items {
            if enabled {
                let _ = items.opensource_toggle.set_text("Open Source Server [ON]");
            } else {
                let _ = items.opensource_toggle.set_text("Open Source Server [OFF]");
            }
            items.opensource_user_id.set_enabled(enabled);
            items.opensource_api_url.set_enabled(enabled);

            let user_label = if user_id.is_empty() {
                "  User ID: (not set)".to_string()
            } else {
                format!("  User ID: {user_id}")
            };
            let url_label = if api_url.is_empty() {
                "  API URL: (not set)".to_string()
            } else {
                format!("  API URL: {api_url}")
            };
            let _ = items.opensource_user_id.set_text(&user_label);
            let _ = items.opensource_api_url.set_text(&url_label);
        }
    }
}

impl ApplicationHandler<TrayEvent> for App {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {
        if self.tray.is_some() {
            return; // Already initialized
        }

        // Build menu
        let connect = MenuItem::new("Connect", true, None);
        let disconnect = MenuItem::new("Disconnect", false, None);
        let status = MenuItem::new("Status: Disconnected", false, None);
        let settings = MenuItem::new("Open Config File", true, None);

        // Load current config to display opensource settings state
        let current_config = Config::load();
        let os_enabled = current_config.opensource_server_enabled;
        let os_toggle_label = if os_enabled {
            "Open Source Server [ON]"
        } else {
            "Open Source Server [OFF]"
        };
        let opensource_toggle = MenuItem::new(os_toggle_label, true, None);

        let os_user_label = if current_config.opensource_user_id.is_empty() {
            "  User ID: (not set)".to_string()
        } else {
            format!("  User ID: {}", current_config.opensource_user_id)
        };
        let os_url_label = if current_config.opensource_api_url.is_empty() {
            "  API URL: (not set)".to_string()
        } else {
            format!("  API URL: {}", current_config.opensource_api_url)
        };
        let opensource_user_id = MenuItem::new(&os_user_label, os_enabled, None);
        let opensource_api_url = MenuItem::new(&os_url_label, os_enabled, None);

        let quit = MenuItem::new("Quit", true, None);

        let opensource_submenu = Submenu::new("Open Source Server", true);
        let _ = opensource_submenu.append(&opensource_toggle);
        let _ = opensource_submenu.append(&opensource_user_id);
        let _ = opensource_submenu.append(&opensource_api_url);

        let menu = Menu::new();
        let _ = menu.append(&connect);
        let _ = menu.append(&disconnect);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&status);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&settings);
        let _ = menu.append(&opensource_submenu);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&quit);

        let icon = create_icon(false);

        match TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("ScreenMCP Linux - Disconnected")
            .with_icon(icon)
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

        // Store menu item references for later updates
        let menu_items = MenuItems {
            connect,
            disconnect,
            status,
            settings,
            opensource_toggle,
            opensource_user_id,
            opensource_api_url,
            quit,
        };
        self.menu_items = Some(menu_items);

        // Spawn a task to watch for status changes and forward them to the event loop
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

        // Spawn a thread to handle menu events
        let ws_tx = self.ws_cmd_tx.clone();
        let proxy2 = self.proxy.clone();
        let menu_ref = self.menu_items.as_ref().unwrap();
        let connect_id = menu_ref.connect.id().clone();
        let disconnect_id = menu_ref.disconnect.id().clone();
        let settings_id = menu_ref.settings.id().clone();
        let opensource_toggle_id = menu_ref.opensource_toggle.id().clone();
        let opensource_user_id_id = menu_ref.opensource_user_id.id().clone();
        let opensource_api_url_id = menu_ref.opensource_api_url.id().clone();
        let quit_id = menu_ref.quit.id().clone();

        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            loop {
                if let Ok(event) = MenuEvent::receiver().recv() {
                    if event.id() == &connect_id {
                        info!("menu: connect clicked");
                        // Reload config before connecting
                        let new_config = Config::load();
                        let _ = rt.block_on(ws_tx.send(WsCommand::UpdateConfig(new_config)));
                        let _ = rt.block_on(ws_tx.send(WsCommand::Connect));
                    } else if event.id() == &disconnect_id {
                        info!("menu: disconnect clicked");
                        let _ = rt.block_on(ws_tx.send(WsCommand::Disconnect));
                    } else if event.id() == &settings_id {
                        info!("menu: settings clicked");
                        let config_path = Config::config_path();
                        // Ensure config file exists
                        if !config_path.exists() {
                            let config = Config::load();
                            if let Err(e) = config.save() {
                                error!("failed to save default config: {e}");
                            }
                        }
                        // Open config file in default editor (uses xdg-open on Linux)
                        if let Err(e) = open::that(&config_path) {
                            error!("failed to open config: {e}");
                        }
                    } else if event.id() == &opensource_toggle_id {
                        info!("menu: opensource toggle clicked");
                        let mut config = Config::load();
                        config.opensource_server_enabled = !config.opensource_server_enabled;
                        let enabled = config.opensource_server_enabled;
                        if let Err(e) = config.save() {
                            error!("failed to save config: {e}");
                        }

                        // Send update to main thread to update menu items
                        let config = Config::load();
                        let _ = proxy2.send_event(TrayEvent::UpdateOpensourceMenu(
                            config.opensource_server_enabled,
                            config.opensource_user_id.clone(),
                            config.opensource_api_url.clone(),
                        ));

                        // Update WS manager config
                        let _ = rt.block_on(ws_tx.send(WsCommand::UpdateConfig(config)));

                        info!(
                            "opensource server mode: {}",
                            if enabled { "enabled" } else { "disabled" }
                        );
                    } else if event.id() == &opensource_user_id_id {
                        info!("menu: opensource user_id clicked");
                        // Use zenity for text input on Linux
                        let config = Config::load();
                        let current = config.opensource_user_id.clone();
                        let result = std::process::Command::new("zenity")
                            .args([
                                "--entry",
                                "--title=Open Source Server - User ID",
                                "--text=Enter User ID (used as Bearer token):",
                                &format!("--entry-text={current}"),
                            ])
                            .output();

                        match result {
                            Ok(output) if output.status.success() => {
                                let new_value =
                                    String::from_utf8_lossy(&output.stdout).trim().to_string();
                                let mut config = Config::load();
                                config.opensource_user_id = new_value;
                                if let Err(e) = config.save() {
                                    error!("failed to save config: {e}");
                                }

                                // Send update to main thread
                                let config = Config::load();
                                let _ = proxy2.send_event(TrayEvent::UpdateOpensourceMenu(
                                    config.opensource_server_enabled,
                                    config.opensource_user_id.clone(),
                                    config.opensource_api_url.clone(),
                                ));

                                // Update WS manager config
                                let _ = rt.block_on(ws_tx.send(WsCommand::UpdateConfig(config)));
                            }
                            Ok(_) => {
                                info!("user cancelled user_id input");
                            }
                            Err(e) => {
                                error!("failed to run zenity for user_id input: {e}");
                                // Fallback: open the config file
                                let config_path = Config::config_path();
                                let _ = open::that(&config_path);
                            }
                        }
                    } else if event.id() == &opensource_api_url_id {
                        info!("menu: opensource api_url clicked");
                        // Use zenity for text input on Linux
                        let config = Config::load();
                        let current = config.opensource_api_url.clone();
                        let result = std::process::Command::new("zenity")
                            .args([
                                "--entry",
                                "--title=Open Source Server - API URL",
                                "--text=Enter API Server URL:",
                                &format!("--entry-text={current}"),
                            ])
                            .output();

                        match result {
                            Ok(output) if output.status.success() => {
                                let new_value =
                                    String::from_utf8_lossy(&output.stdout).trim().to_string();
                                let mut config = Config::load();
                                config.opensource_api_url = new_value;
                                if let Err(e) = config.save() {
                                    error!("failed to save config: {e}");
                                }

                                // Send update to main thread
                                let config = Config::load();
                                let _ = proxy2.send_event(TrayEvent::UpdateOpensourceMenu(
                                    config.opensource_server_enabled,
                                    config.opensource_user_id.clone(),
                                    config.opensource_api_url.clone(),
                                ));

                                // Update WS manager config
                                let _ = rt.block_on(ws_tx.send(WsCommand::UpdateConfig(config)));
                            }
                            Ok(_) => {
                                info!("user cancelled api_url input");
                            }
                            Err(e) => {
                                error!("failed to run zenity for api_url input: {e}");
                                // Fallback: open the config file
                                let config_path = Config::config_path();
                                let _ = open::that(&config_path);
                            }
                        }
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
            TrayEvent::UpdateOpensourceMenu(enabled, user_id, api_url) => {
                self.update_opensource_menu(enabled, &user_id, &api_url);
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
        last_status: ConnectionStatus::Disconnected,
        proxy,
    };

    event_loop.run_app(&mut app).expect("event loop failed");
}
