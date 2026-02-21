use tokio::sync::{mpsc, watch};
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};
use tracing::{error, info};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop, EventLoopProxy};
use winit::window::WindowId;

use crate::config::Config;
use crate::ws::{ConnectionStatus, WsCommand};

/// Wrapper to move non-Send types into a single background thread.
/// Safety: the wrapped value must only be accessed from that thread.
struct SendWrapper<T>(T);
unsafe impl<T> Send for SendWrapper<T> {}

/// Truncate a string for display, appending "..." if it exceeds max_len.
fn truncate_display(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len])
    }
}

/// Custom events sent to the winit event loop.
#[derive(Debug)]
pub enum TrayEvent {
    StatusChanged(ConnectionStatus),
    Quit,
}

/// IDs for menu items.
struct MenuItems {
    connect: MenuItem,
    disconnect: MenuItem,
    status: MenuItem,
    settings: MenuItem,
    opensource_enabled: CheckMenuItem,
    opensource_user_id: MenuItem,
    opensource_api_url: MenuItem,
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
            let _ = tray.set_tooltip(Some(&format!("ScreenMCP Mac - {status}")));
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
}

impl ApplicationHandler<TrayEvent> for App {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {
        if self.tray.is_some() {
            return; // Already initialized
        }

        // Load current config to set initial state of opensource items
        let current_config = Config::load();
        let oss_enabled = current_config.opensource_server_enabled;

        // Build menu
        let connect = MenuItem::new("Connect", true, None);
        let disconnect = MenuItem::new("Disconnect", false, None);
        let status = MenuItem::new("Status: Disconnected", false, None);
        let settings = MenuItem::new("Open Config File", true, None);

        // Open Source Server submenu items
        let opensource_enabled = CheckMenuItem::new("Open Source Server", true, oss_enabled, None);
        let user_id_label = if oss_enabled && !current_config.opensource_user_id.is_empty() {
            format!("User ID: {}", truncate_display(&current_config.opensource_user_id, 30))
        } else {
            "User ID: (not set)".to_string()
        };
        let api_url_label = if oss_enabled && !current_config.opensource_api_url.is_empty() {
            format!("API URL: {}", truncate_display(&current_config.opensource_api_url, 40))
        } else {
            "API URL: (not set)".to_string()
        };
        let opensource_user_id = MenuItem::new(&user_id_label, oss_enabled, None);
        let opensource_api_url = MenuItem::new(&api_url_label, oss_enabled, None);

        let opensource_submenu = Submenu::new("Open Source Server", true);
        let _ = opensource_submenu.append(&opensource_enabled);
        let _ = opensource_submenu.append(&opensource_user_id);
        let _ = opensource_submenu.append(&opensource_api_url);

        let quit = MenuItem::new("Quit", true, None);

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
            .with_tooltip("ScreenMCP Mac - Disconnected")
            .with_icon(icon)
            .with_title("ScreenMCP")
            .build()
        {
            Ok(tray) => {
                self.tray = Some(tray);
                info!("menu bar icon created");
            }
            Err(e) => {
                error!("failed to create menu bar icon: {e}");
            }
        }

        // Store menu item references for later updates
        let menu_items = MenuItems {
            connect,
            disconnect,
            status,
            settings,
            opensource_enabled,
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
        let connect_id = self.menu_items.as_ref().unwrap().connect.id().clone();
        let disconnect_id = self.menu_items.as_ref().unwrap().disconnect.id().clone();
        let settings_id = self.menu_items.as_ref().unwrap().settings.id().clone();
        let opensource_enabled_id = self.menu_items.as_ref().unwrap().opensource_enabled.id().clone();
        let opensource_user_id_id = self.menu_items.as_ref().unwrap().opensource_user_id.id().clone();
        let opensource_api_url_id = self.menu_items.as_ref().unwrap().opensource_api_url.id().clone();
        let quit_id = self.menu_items.as_ref().unwrap().quit.id().clone();

        // Clone the menu items so the background thread can read/update state.
        // Wrapped in SendWrapper because muda's menu items use Rc internally.
        let oss_check_ref = SendWrapper(self.menu_items.as_ref().unwrap().opensource_enabled.clone());
        let oss_userid_ref = SendWrapper(self.menu_items.as_ref().unwrap().opensource_user_id.clone());
        let oss_apiurl_ref = SendWrapper(self.menu_items.as_ref().unwrap().opensource_api_url.clone());

        std::thread::spawn(move || {
            // Force the closure to capture the SendWrappers as whole values
            // (Rust 2021 captures fields individually, which would bypass SendWrapper).
            let oss_check_ref = oss_check_ref;
            let oss_userid_ref = oss_userid_ref;
            let oss_apiurl_ref = oss_apiurl_ref;

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
                        // Open config file in default editor (uses `open` on macOS)
                        if let Err(e) = open::that(&config_path) {
                            error!("failed to open config: {e}");
                        }
                    } else if event.id() == &opensource_enabled_id {
                        // CheckMenuItem auto-toggles its checked state on click
                        let is_checked = oss_check_ref.0.is_checked();
                        info!("menu: open source server toggled to {is_checked}");

                        // Update config and save
                        let mut config = Config::load();
                        config.opensource_server_enabled = is_checked;
                        if let Err(e) = config.save() {
                            error!("failed to save config: {e}");
                        }

                        // Enable/disable the User ID and API URL items
                        oss_userid_ref.0.set_enabled(is_checked);
                        oss_apiurl_ref.0.set_enabled(is_checked);

                        // Update display labels
                        if is_checked {
                            let uid = if config.opensource_user_id.is_empty() {
                                "(not set)".to_string()
                            } else {
                                truncate_display(&config.opensource_user_id, 30)
                            };
                            let aurl = if config.opensource_api_url.is_empty() {
                                "(not set)".to_string()
                            } else {
                                truncate_display(&config.opensource_api_url, 40)
                            };
                            let _ = oss_userid_ref.0.set_text(&format!("User ID: {uid}"));
                            let _ = oss_apiurl_ref.0.set_text(&format!("API URL: {aurl}"));
                        } else {
                            let _ = oss_userid_ref.0.set_text("User ID: (disabled)");
                            let _ = oss_apiurl_ref.0.set_text("API URL: (disabled)");
                        }

                        // Notify WS manager of config change
                        let _ = rt.block_on(ws_tx.send(WsCommand::UpdateConfig(config)));
                    } else if event.id() == &opensource_user_id_id || event.id() == &opensource_api_url_id {
                        // Clicking User ID or API URL opens config file for editing
                        info!("menu: opensource setting clicked, opening config file");
                        let config_path = Config::config_path();
                        if !config_path.exists() {
                            let config = Config::load();
                            if let Err(e) = config.save() {
                                error!("failed to save default config: {e}");
                            }
                        }
                        if let Err(e) = open::that(&config_path) {
                            error!("failed to open config: {e}");
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
        // No windows, just menu bar icon
    }
}

/// Build and run the menu bar icon event loop. This blocks the main thread.
/// On macOS, the main thread must run the event loop for AppKit compatibility.
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
