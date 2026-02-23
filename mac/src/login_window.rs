use eframe::egui;

use crate::auth;
use crate::config::Config;

/// State for the login viewport.
pub struct LoginState {
    pub local_port: u16,
    pub api_url: String,

    // Pre-login OSS fields
    pub oss_enabled: bool,
    pub oss_user_id: String,
    pub oss_api_url: String,

    // Post-login state
    pub signed_in: bool,
    pub signed_in_email: String,

    pub status: String,
}

impl LoginState {
    pub fn new(local_port: u16, api_url: String) -> Self {
        let config = Config::load();

        let cloud_signed_in = !config.token.is_empty() || !config.email.is_empty();
        let oss_signed_in = config.opensource_server_enabled
            && !config.opensource_user_id.is_empty()
            && !config.opensource_api_url.is_empty();
        let signed_in = cloud_signed_in || oss_signed_in;

        let email = if oss_signed_in {
            format!("Open Source ({})", config.opensource_user_id)
        } else if !config.email.is_empty() {
            config.email.clone()
        } else if !config.token.is_empty() {
            format!("{}...", &config.token[..config.token.len().min(11)])
        } else {
            String::new()
        };

        Self {
            local_port,
            api_url,
            oss_enabled: config.opensource_server_enabled,
            oss_user_id: config.opensource_user_id.clone(),
            oss_api_url: config.opensource_api_url.clone(),
            signed_in,
            signed_in_email: email,
            status: "Not connected".to_string(),
        }
    }

    /// Render the login UI. Returns true if the viewport should close.
    pub fn render(&mut self, ctx: &egui::Context) -> bool {
        let mut should_close = false;

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(20.0);
                ui.label(egui::RichText::new("ScreenMCP").size(26.0).strong());
                ui.add_space(4.0);

                if self.signed_in {
                    // ── Post-login state ──
                    ui.add_space(16.0);
                    ui.label(
                        egui::RichText::new("Signed in as")
                            .size(15.0)
                            .color(ui.visuals().weak_text_color()),
                    );
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(&self.signed_in_email)
                            .size(17.0)
                            .strong(),
                    );
                    ui.add_space(20.0);

                    if ui
                        .add_sized([220.0, 38.0], egui::Button::new("Close"))
                        .clicked()
                    {
                        should_close = true;
                    }

                    ui.add_space(8.0);

                    if ui
                        .add_sized([220.0, 38.0], egui::Button::new("Sign Out"))
                        .clicked()
                    {
                        let mut config = Config::load();
                        config.token.clear();
                        config.email.clear();
                        config.opensource_server_enabled = false;
                        config.opensource_user_id.clear();
                        config.opensource_api_url.clear();
                        let _ = config.save();
                        self.signed_in = false;
                        self.signed_in_email.clear();
                        self.oss_enabled = false;
                        self.oss_user_id.clear();
                        self.oss_api_url.clear();
                        self.status = "Not connected".to_string();
                    }
                } else {
                    // ── Pre-login state ──
                    ui.label(
                        egui::RichText::new("Connect your device to AI")
                            .size(15.0)
                            .color(ui.visuals().weak_text_color()),
                    );
                    ui.add_space(16.0);

                    // Google sign-in button (disabled when OSS mode is on)
                    ui.add_enabled_ui(!self.oss_enabled, |ui| {
                        if ui
                            .add_sized(
                                [260.0, 38.0],
                                egui::Button::new("Sign in with Google"),
                            )
                            .clicked()
                        {
                            self.status = "Opening browser...".to_string();
                            auth::open_google_sign_in(self.local_port, &self.api_url);
                        }
                    });

                    ui.add_space(16.0);

                    // Divider
                    ui.horizontal(|ui| {
                        ui.add_space(4.0);
                        ui.add(
                            egui::Separator::default()
                                .horizontal()
                                .shrink(ui.available_height()),
                        );
                        ui.label(
                            egui::RichText::new("or")
                                .size(13.0)
                                .color(ui.visuals().weak_text_color()),
                        );
                        ui.add(
                            egui::Separator::default()
                                .horizontal()
                                .shrink(ui.available_height()),
                        );
                    });

                    ui.add_space(12.0);

                    // OSS checkbox
                    ui.checkbox(&mut self.oss_enabled, "Open Source Server");

                    ui.add_space(8.0);

                    // OSS fields
                    ui.add_enabled_ui(self.oss_enabled, |ui| {
                        ui.set_min_width(280.0);

                        ui.horizontal(|ui| {
                            ui.label("User ID:");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.oss_user_id)
                                    .desired_width(200.0)
                                    .hint_text("local-user"),
                            );
                        });

                        ui.add_space(4.0);

                        ui.horizontal(|ui| {
                            ui.label("API URL:");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.oss_api_url)
                                    .desired_width(200.0)
                                    .hint_text("http://localhost:3000"),
                            );
                        });

                        ui.add_space(12.0);

                        let can_connect = self.oss_enabled
                            && !self.oss_user_id.trim().is_empty()
                            && !self.oss_api_url.trim().is_empty();

                        ui.add_enabled_ui(can_connect, |ui| {
                            if ui
                                .add_sized([220.0, 38.0], egui::Button::new("Connect"))
                                .clicked()
                            {
                                let mut config = Config::load();
                                config.opensource_server_enabled = true;
                                config.opensource_user_id =
                                    self.oss_user_id.trim().to_string();
                                config.opensource_api_url =
                                    self.oss_api_url.trim().to_string();
                                if let Err(e) = config.save() {
                                    self.status = format!("Error: {e}");
                                } else {
                                    self.status = "Config saved".to_string();
                                    should_close = true;
                                }
                            }
                        });
                    });

                    ui.add_space(16.0);

                    // Status line
                    ui.label(
                        egui::RichText::new(format!("Status: {}", self.status))
                            .size(13.0)
                            .color(ui.visuals().weak_text_color()),
                    );
                }

                ui.add_space(12.0);
            });
        });

        should_close
    }
}
