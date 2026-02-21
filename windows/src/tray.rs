use tokio::sync::{mpsc, watch};
use tray_icon::menu::{
    CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu,
};
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
    RefreshConfig,
    Quit,
}

/// IDs for menu items.
struct MenuItems {
    connect: MenuItem,
    disconnect: MenuItem,
    status: MenuItem,
    settings: MenuItem,
    quit: MenuItem,
    // Open source server items
    opensource_toggle: CheckMenuItem,
    opensource_user_id: MenuItem,
    opensource_api_url: MenuItem,
    opensource_edit: MenuItem,
}

/// Create a simple colored icon (green or red).
fn create_icon(connected: bool) -> Icon {
    let size = 32u32;
    let rgba = create_icon_rgba(connected);
    Icon::from_rgba(rgba, size, size).expect("failed to create icon")
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
            let _ = tray.set_tooltip(Some(&format!("ScreenMCP Windows - {status}")));
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

/// Truncate a string for display in menu items.
fn truncate_for_display(s: &str, max_len: usize) -> String {
    if s.is_empty() {
        "(not set)".to_string()
    } else if s.len() > max_len {
        format!("{}...", &s[..max_len])
    } else {
        s.to_string()
    }
}

/// Update the opensource submenu item labels from config.
fn update_opensource_labels(items: &MenuItems, config: &Config) {
    let enabled = config.opensource_server_enabled;
    items.opensource_toggle.set_checked(enabled);
    let _ = items.opensource_user_id.set_text(&format!(
        "User ID: {}",
        truncate_for_display(&config.opensource_user_id, 30)
    ));
    items.opensource_user_id.set_enabled(enabled);
    let _ = items.opensource_api_url.set_text(&format!(
        "API URL: {}",
        truncate_for_display(&config.opensource_api_url, 40)
    ));
    items.opensource_api_url.set_enabled(enabled);
    items.opensource_edit.set_enabled(enabled);
}

impl ApplicationHandler<TrayEvent> for App {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {
        if self.tray.is_some() {
            return; // Already initialized
        }

        // Load current config for initial state
        let config = Config::load();

        // Build menu
        let connect = MenuItem::new("Connect", true, None);
        let disconnect = MenuItem::new("Disconnect", false, None);
        let status = MenuItem::new("Status: Disconnected", false, None);
        let settings = MenuItem::new("Open Config File", true, None);
        let quit = MenuItem::new("Quit", true, None);

        // Open Source Server submenu
        let opensource_toggle =
            CheckMenuItem::new("Open Source Server", true, config.opensource_server_enabled, None);
        let opensource_user_id = MenuItem::new(
            &format!(
                "User ID: {}",
                truncate_for_display(&config.opensource_user_id, 30)
            ),
            config.opensource_server_enabled,
            None,
        );
        let opensource_api_url = MenuItem::new(
            &format!(
                "API URL: {}",
                truncate_for_display(&config.opensource_api_url, 40)
            ),
            config.opensource_server_enabled,
            None,
        );
        let opensource_edit = MenuItem::new(
            "Edit Open Source Settings...",
            config.opensource_server_enabled,
            None,
        );

        let opensource_submenu = Submenu::new("Open Source Server", true);
        let _ = opensource_submenu.append(&opensource_toggle);
        let _ = opensource_submenu.append(&PredefinedMenuItem::separator());
        let _ = opensource_submenu.append(&opensource_user_id);
        let _ = opensource_submenu.append(&opensource_api_url);
        let _ = opensource_submenu.append(&PredefinedMenuItem::separator());
        let _ = opensource_submenu.append(&opensource_edit);

        let menu = Menu::new();
        let _ = menu.append(&connect);
        let _ = menu.append(&disconnect);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&status);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&opensource_submenu);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&settings);
        let _ = menu.append(&PredefinedMenuItem::separator());
        let _ = menu.append(&quit);

        let icon = create_icon(false);

        match TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_tooltip("ScreenMCP Windows - Disconnected")
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
            quit,
            opensource_toggle,
            opensource_user_id,
            opensource_api_url,
            opensource_edit,
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
        let quit_id = self.menu_items.as_ref().unwrap().quit.id().clone();
        let opensource_toggle_id = self
            .menu_items
            .as_ref()
            .unwrap()
            .opensource_toggle
            .id()
            .clone();
        let opensource_user_id_id = self
            .menu_items
            .as_ref()
            .unwrap()
            .opensource_user_id
            .id()
            .clone();
        let opensource_api_url_id = self
            .menu_items
            .as_ref()
            .unwrap()
            .opensource_api_url
            .id()
            .clone();
        let opensource_edit_id = self
            .menu_items
            .as_ref()
            .unwrap()
            .opensource_edit
            .id()
            .clone();

        // Channel to request tray label refresh when opensource settings change
        let (label_update_tx, mut label_update_rx) =
            tokio::sync::mpsc::channel::<Config>(4);

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
                        open_config_file();
                    } else if event.id() == &quit_id {
                        info!("menu: quit clicked");
                        let _ = rt.block_on(ws_tx.send(WsCommand::Shutdown));
                        let _ = proxy2.send_event(TrayEvent::Quit);
                    } else if event.id() == &opensource_toggle_id {
                        info!("menu: opensource toggle clicked");
                        // Toggle the setting and save
                        let mut cfg = Config::load();
                        cfg.opensource_server_enabled = !cfg.opensource_server_enabled;
                        if let Err(e) = cfg.save() {
                            error!("failed to save config: {e}");
                        }
                        // Request label update
                        let _ = rt.block_on(label_update_tx.send(cfg));
                    } else if event.id() == &opensource_user_id_id
                        || event.id() == &opensource_api_url_id
                        || event.id() == &opensource_edit_id
                    {
                        info!("menu: open source settings edit clicked");
                        open_config_file();
                    }
                }
            }
        });

        // Forward label update requests to the event loop as RefreshConfig events
        let proxy_for_labels = self.proxy.clone();
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(async {
                while let Some(_cfg) = label_update_rx.recv().await {
                    // Just signal the event loop to refresh. We'll re-load config there.
                    let _ = proxy_for_labels.send_event(TrayEvent::RefreshConfig);
                }
            });
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
            TrayEvent::RefreshConfig => {
                let config = Config::load();
                if let Some(ref items) = self.menu_items {
                    update_opensource_labels(items, &config);
                }
                info!("opensource settings refreshed in tray");
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

fn open_config_file() {
    let config_path = Config::config_path();
    // Ensure config file exists
    if !config_path.exists() {
        let config = Config::load();
        if let Err(e) = config.save() {
            error!("failed to save default config: {e}");
        }
    }
    // Open config file in default editor
    if let Err(e) = open::that(&config_path) {
        error!("failed to open config: {e}");
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
