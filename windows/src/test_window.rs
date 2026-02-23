use eframe::egui;
use regex::Regex;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;
use crate::commands;
use crate::config::Config;

/// Post-process pretty-printed JSON for compact display.
fn compact_json(json: &str) -> String {
    // 1. Collapse bounds: [[x, y], [w, h]] onto one line
    let re_bounds = Regex::new(
        r#"(?s)"bounds":\s*\[\s*\[\s*(\d+),\s*(\d+)\s*\],\s*\[\s*(\d+),\s*(\d+)\s*\]\s*\]"#
    ).unwrap();
    let json = re_bounds.replace_all(json, r#""bounds": [[$1, $2], [$3, $4]]"#);

    // 2. Collapse runs of closing-only lines (} ] }, ],) onto one line,
    //    indented at the shallowest (last) closer's level.
    let lines: Vec<&str> = json.lines().collect();
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut closers: Vec<char> = Vec::new();
    let mut min_indent = usize::MAX;
    let mut trailing_comma = false;

    for line in &lines {
        let trimmed = line.trim();
        let is_closer = trimmed == "}" || trimmed == "]"
            || trimmed == "}," || trimmed == "],";
        if is_closer {
            let indent = line.len() - line.trim_start().len();
            min_indent = min_indent.min(indent);
            closers.push(trimmed.chars().next().unwrap());
            trailing_comma = trimmed.ends_with(',');
            continue;
        }
        if !closers.is_empty() {
            let mut combined: String = closers.iter().collect();
            if trailing_comma { combined.push(','); }
            out.push(format!("{}{}", " ".repeat(min_indent), combined));
            closers.clear();
            min_indent = usize::MAX;
            trailing_comma = false;
        }
        out.push(line.to_string());
    }
    if !closers.is_empty() {
        let mut combined: String = closers.iter().collect();
        if trailing_comma { combined.push(','); }
        out.push(format!("{}{}", " ".repeat(min_indent), combined));
    }

    out.join("\n")
}

struct LogEntry {
    time: String,
    message: String,
    is_error: bool,
}

/// State for the test window viewport.
pub struct TestState {
    // Screenshot
    screenshot_texture: Option<egui::TextureHandle>,
    screenshot_size: (f32, f32),

    // Click
    click_x: String,
    click_y: String,

    // Drag
    drag_sx: String,
    drag_sy: String,
    drag_ex: String,
    drag_ey: String,

    // Type
    type_text: String,

    // UI Tree
    ui_tree_text: String,
    show_ui_tree: bool,

    // Log
    log: Vec<LogEntry>,

    // Background command execution
    pending_result: Arc<Mutex<Option<(String, Result<Value, String>)>>>,
}

impl TestState {
    pub fn new() -> Self {
        let mut s = Self {
            screenshot_texture: None,
            screenshot_size: (0.0, 0.0),
            click_x: String::new(),
            click_y: String::new(),
            drag_sx: String::new(),
            drag_sy: String::new(),
            drag_ex: String::new(),
            drag_ey: String::new(),
            type_text: String::new(),
            ui_tree_text: String::new(),
            show_ui_tree: false,
            log: Vec::new(),
            pending_result: Arc::new(Mutex::new(None)),
        };
        s.add_log("Test window ready", false);
        s
    }

    fn add_log(&mut self, message: &str, is_error: bool) {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default();
        let secs = now.as_secs();
        let hours = (secs / 3600) % 24;
        let minutes = (secs / 60) % 60;
        let seconds = secs % 60;
        let time = format!("{hours:02}:{minutes:02}:{seconds:02}");
        self.log.push(LogEntry {
            time,
            message: message.to_string(),
            is_error,
        });
    }

    fn execute_command(&self, cmd: &str, params: Option<Value>) {
        let cmd = cmd.to_string();
        let pending = self.pending_result.clone();
        std::thread::spawn(move || {
            let config = Config::load();
            let result = commands::execute_command(0, &cmd, params.as_ref(), &config);
            let status = result
                .get("status")
                .and_then(|s| s.as_str())
                .unwrap_or("unknown");
            if status == "error" {
                let err = result
                    .get("error")
                    .and_then(|e| e.as_str())
                    .unwrap_or("unknown error");
                *pending.lock().unwrap() = Some((cmd, Err(err.to_string())));
            } else {
                *pending.lock().unwrap() = Some((cmd, Ok(result)));
            }
        });
    }

    fn process_pending_results(&mut self, ctx: &egui::Context) {
        let result = self.pending_result.lock().unwrap().take();
        if let Some((cmd, result)) = result {
            match result {
                Ok(value) => {
                    self.add_log(&format!("{cmd}: ok"), false);
                    if cmd == "screenshot" {
                        if let Some(image_b64) = value
                            .get("result")
                            .and_then(|r| r.get("image"))
                            .and_then(|i| i.as_str())
                        {
                            self.load_screenshot(ctx, image_b64);
                        }
                    }
                    if cmd == "ui_tree" {
                        if let Some(tree) = value.get("result").and_then(|r| r.get("tree")) {
                            let pretty = serde_json::to_string_pretty(tree).unwrap_or_default();
                            self.ui_tree_text = compact_json(&pretty);
                            self.show_ui_tree = true;
                        }
                    }
                }
                Err(e) => {
                    self.add_log(&format!("{cmd}: {e}"), true);
                }
            }
        }
    }

    fn load_screenshot(&mut self, ctx: &egui::Context, b64: &str) {
        use base64::Engine;
        let bytes = match base64::engine::general_purpose::STANDARD.decode(b64) {
            Ok(b) => b,
            Err(e) => {
                self.add_log(&format!("screenshot decode error: {e}"), true);
                return;
            }
        };

        let img = match image::load_from_memory(&bytes) {
            Ok(img) => img.to_rgba8(),
            Err(e) => {
                self.add_log(&format!("screenshot image error: {e}"), true);
                return;
            }
        };

        let size = [img.width() as usize, img.height() as usize];
        let pixels = img.into_raw();
        let color_image = egui::ColorImage::from_rgba_unmultiplied(size, &pixels);

        self.screenshot_texture = Some(ctx.load_texture(
            "screenshot",
            color_image,
            egui::TextureOptions::LINEAR,
        ));
        self.screenshot_size = (size[0] as f32, size[1] as f32);
    }

    /// Render the test window UI.
    pub fn render(&mut self, ctx: &egui::Context) {
        self.process_pending_results(ctx);

        // Request repaint to pick up async results
        ctx.request_repaint_after(std::time::Duration::from_millis(100));

        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.heading("ScreenMCP Test Window");
                ui.add_space(8.0);

                // Screenshot
                ui.group(|ui| {
                    ui.label(
                        egui::RichText::new("SCREENSHOT")
                            .small()
                            .color(egui::Color32::GRAY),
                    );
                    ui.add_space(4.0);
                    if ui.button("Take Screenshot").clicked() {
                        self.add_log("Taking screenshot...", false);
                        self.execute_command("screenshot", None);
                    }
                    if let Some(ref texture) = self.screenshot_texture {
                        let available_width = ui.available_width();
                        let aspect = self.screenshot_size.1 / self.screenshot_size.0.max(1.0);
                        let display_width = available_width.min(self.screenshot_size.0);
                        let display_height = display_width * aspect;
                        ui.image(egui::load::SizedTexture::new(
                            texture.id(),
                            egui::vec2(display_width, display_height),
                        ));
                    }
                });

                ui.add_space(4.0);

                // Click
                ui.group(|ui| {
                    ui.label(
                        egui::RichText::new("CLICK / TAP")
                            .small()
                            .color(egui::Color32::GRAY),
                    );
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label("X:");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.click_x)
                                .desired_width(60.0)
                                .hint_text("X"),
                        );
                        ui.label("Y:");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.click_y)
                                .desired_width(60.0)
                                .hint_text("Y"),
                        );
                        if ui.button("Click at (X, Y)").clicked() {
                            if let (Ok(x), Ok(y)) =
                                (self.click_x.parse::<f64>(), self.click_y.parse::<f64>())
                            {
                                self.execute_command("click", Some(json!({"x": x, "y": y})));
                            } else {
                                self.add_log("Enter valid coordinates", true);
                            }
                        }
                    });
                });

                ui.add_space(4.0);

                // Drag
                ui.group(|ui| {
                    ui.label(
                        egui::RichText::new("DRAG / SWIPE")
                            .small()
                            .color(egui::Color32::GRAY),
                    );
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label("Start:");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.drag_sx)
                                .desired_width(50.0)
                                .hint_text("X"),
                        );
                        ui.add(
                            egui::TextEdit::singleline(&mut self.drag_sy)
                                .desired_width(50.0)
                                .hint_text("Y"),
                        );
                        ui.label("End:");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.drag_ex)
                                .desired_width(50.0)
                                .hint_text("X"),
                        );
                        ui.add(
                            egui::TextEdit::singleline(&mut self.drag_ey)
                                .desired_width(50.0)
                                .hint_text("Y"),
                        );
                        if ui.button("Drag").clicked() {
                            if let (Ok(sx), Ok(sy), Ok(ex), Ok(ey)) = (
                                self.drag_sx.parse::<f64>(),
                                self.drag_sy.parse::<f64>(),
                                self.drag_ex.parse::<f64>(),
                                self.drag_ey.parse::<f64>(),
                            ) {
                                self.execute_command(
                                    "drag",
                                    Some(json!({"startX": sx, "startY": sy, "endX": ex, "endY": ey})),
                                );
                            } else {
                                self.add_log("Enter valid coordinates", true);
                            }
                        }
                    });
                });

                ui.add_space(4.0);

                // Type
                ui.group(|ui| {
                    ui.label(
                        egui::RichText::new("TYPE TEXT")
                            .small()
                            .color(egui::Color32::GRAY),
                    );
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut self.type_text)
                                .desired_width(200.0)
                                .hint_text("Text to type"),
                        );
                        if ui.button("Type").clicked() {
                            if !self.type_text.is_empty() {
                                let text = self.type_text.clone();
                                self.execute_command("type", Some(json!({"text": text})));
                            } else {
                                self.add_log("Enter text", true);
                            }
                        }
                    });
                    if ui.button("Get Text").clicked() {
                        self.execute_command("get_text", None);
                    }
                });

                ui.add_space(4.0);

                // Clipboard
                ui.group(|ui| {
                    ui.label(
                        egui::RichText::new("CLIPBOARD")
                            .small()
                            .color(egui::Color32::GRAY),
                    );
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        if ui.button("Select All").clicked() {
                            self.execute_command("select_all", None);
                        }
                        if ui.button("Copy").clicked() {
                            self.execute_command("copy", None);
                        }
                        if ui.button("Paste").clicked() {
                            self.execute_command("paste", None);
                        }
                    });
                });

                ui.add_space(4.0);

                // Navigation
                ui.group(|ui| {
                    ui.label(
                        egui::RichText::new("NAVIGATION")
                            .small()
                            .color(egui::Color32::GRAY),
                    );
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        if ui.button("Back").clicked() {
                            self.execute_command("back", None);
                        }
                        if ui.button("Home").clicked() {
                            self.execute_command("home", None);
                        }
                        if ui.button("Recents").clicked() {
                            self.execute_command("recents", None);
                        }
                    });
                });

                ui.add_space(4.0);

                // UI Tree
                ui.group(|ui| {
                    ui.label(
                        egui::RichText::new("UI TREE")
                            .small()
                            .color(egui::Color32::GRAY),
                    );
                    ui.add_space(4.0);
                    if ui.button("Get UI Tree").clicked() {
                        self.add_log("Getting UI tree...", false);
                        self.execute_command("ui_tree", None);
                    }
                    if self.show_ui_tree && !self.ui_tree_text.is_empty() {
                        egui::ScrollArea::vertical()
                            .max_height(300.0)
                            .id_salt("ui_tree_scroll")
                            .show(ui, |ui| {
                                ui.add(
                                    egui::TextEdit::multiline(&mut self.ui_tree_text.as_str())
                                        .font(egui::TextStyle::Monospace)
                                        .desired_width(f32::INFINITY),
                                );
                            });
                    }
                });

                ui.add_space(4.0);

                // Log
                ui.group(|ui| {
                    ui.label(
                        egui::RichText::new("LOG")
                            .small()
                            .color(egui::Color32::GRAY),
                    );
                    ui.add_space(4.0);
                    egui::ScrollArea::vertical()
                        .max_height(160.0)
                        .id_salt("log_scroll")
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            for entry in &self.log {
                                let color = if entry.is_error {
                                    egui::Color32::from_rgb(0xc6, 0x28, 0x28)
                                } else {
                                    egui::Color32::from_rgb(0x2e, 0x7d, 0x32)
                                };
                                ui.horizontal(|ui| {
                                    ui.monospace(
                                        egui::RichText::new(format!(
                                            "[{}] {}",
                                            entry.time, entry.message
                                        ))
                                        .color(color),
                                    );
                                });
                            }
                        });
                });
            });
        });
    }
}
