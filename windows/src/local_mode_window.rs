use eframe::egui;

use crate::config::Config;

pub struct LocalModeState {
    pub key: String,
    pub port_str: String,
    pub show_key: bool,
    pub status: String,
    pub saved: bool,
    /// Default model for the copyable MCP URL: "default" | "claude" | "gemini" | "chatgpt".
    pub model: String,
}

impl LocalModeState {
    pub fn new() -> Self {
        let config = Config::load();
        Self {
            key: config.local_mode_key.clone(),
            port_str: config.local_mode_port.to_string(),
            show_key: false,
            status: String::new(),
            saved: false,
            model: "default".to_string(),
        }
    }

    /// Render the local mode settings UI. Returns true if the viewport should close.
    pub fn render(&mut self, ctx: &egui::Context) -> bool {
        let mut should_close = false;

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(20.0);
                ui.label(egui::RichText::new("Local Mode Settings").size(22.0).strong());
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new(
                        "Configure direct HTTP access for AI assistants and scripts",
                    )
                    .size(13.0)
                    .color(ui.visuals().weak_text_color()),
                );
                ui.add_space(20.0);
            });

            ui.set_min_width(320.0);

            // API Key field
            ui.horizontal(|ui| {
                ui.label("API Key:");
                ui.add(
                    egui::TextEdit::singleline(&mut self.key)
                        .desired_width(200.0)
                        .password(!self.show_key)
                        .hint_text("Enter key or generate one"),
                );
            });

            ui.add_space(4.0);

            ui.horizontal(|ui| {
                ui.add_space(60.0); // Alignment spacer
                if ui.button("Generate").clicked() {
                    let mut bytes = [0u8; 16];
                    getrandom::getrandom(&mut bytes).expect("random failed");
                    self.key = bytes.iter().map(|b| format!("{:02x}", b)).collect();
                    self.show_key = true;
                }
                let show_label = if self.show_key { "Hide" } else { "Show" };
                if ui.button(show_label).clicked() {
                    self.show_key = !self.show_key;
                }
            });

            ui.add_space(12.0);

            // Port field
            ui.horizontal(|ui| {
                ui.label("Port:       ");
                ui.add(
                    egui::TextEdit::singleline(&mut self.port_str)
                        .desired_width(80.0)
                        .hint_text("6767"),
                );
            });

            ui.add_space(8.0);

            // Model: sets a provider-tuned default screenshot size via ?model= in the URL
            ui.horizontal(|ui| {
                ui.label("Model:     ");
                let label = match self.model.as_str() {
                    "claude" => "Claude",
                    "gemini" => "Gemini",
                    "chatgpt" => "ChatGPT",
                    _ => "Default",
                };
                egui::ComboBox::from_id_salt("local_mode_model")
                    .selected_text(label)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.model, "default".to_string(), "Default");
                        ui.selectable_value(&mut self.model, "claude".to_string(), "Claude");
                        ui.selectable_value(&mut self.model, "gemini".to_string(), "Gemini");
                        ui.selectable_value(&mut self.model, "chatgpt".to_string(), "ChatGPT");
                    });
            });

            ui.add_space(20.0);

            ui.vertical_centered(|ui| {
                if ui
                    .add_sized([220.0, 38.0], egui::Button::new("Save"))
                    .clicked()
                {
                    let port: u16 = match self.port_str.trim().parse() {
                        Ok(p) if p > 0 => p,
                        _ => {
                            self.status = "Invalid port number".to_string();
                            return;
                        }
                    };

                    let mut config = Config::load();
                    config.local_mode_key = self.key.trim().to_string();
                    config.local_mode_port = port;
                    match config.save() {
                        Ok(()) => {
                            self.status = if config.local_mode_key.is_empty() {
                                "Saved. Local mode is disabled (no key).".to_string()
                            } else {
                                format!("Saved. Restart app to apply. Will listen on :{port}")
                            };
                            self.saved = true;
                        }
                        Err(e) => {
                            self.status = format!("Error: {e}");
                        }
                    }
                }

                ui.add_space(8.0);

                if ui
                    .add_sized([220.0, 38.0], egui::Button::new("Close"))
                    .clicked()
                {
                    should_close = true;
                }

                if !self.status.is_empty() {
                    ui.add_space(12.0);
                    ui.label(
                        egui::RichText::new(&self.status)
                            .size(13.0)
                            .color(ui.visuals().weak_text_color()),
                    );
                }

                // Show MCP config snippet when key is set
                if !self.key.trim().is_empty() {
                    ui.add_space(16.0);
                    ui.label(
                        egui::RichText::new("Claude Code MCP config:")
                            .size(13.0)
                            .strong(),
                    );
                    let port = self.port_str.trim();
                    let model_q = if self.model != "default" {
                        format!("?model={}", self.model)
                    } else {
                        String::new()
                    };
                    let snippet = format!(
                        r#"{{
  "mcpServers": {{
    "screenmcp": {{
      "type": "url",
      "url": "http://127.0.0.1:{port}/mcp{model_q}",
      "headers": {{
        "Authorization": "Bearer {}"
      }}
    }}
  }}
}}"#,
                        self.key.trim()
                    );
                    let mut snippet_display = snippet.clone();
                    ui.add(
                        egui::TextEdit::multiline(&mut snippet_display)
                            .desired_width(380.0)
                            .desired_rows(8)
                            .font(egui::TextStyle::Monospace),
                    );
                }
            });
        });

        should_close
    }
}
