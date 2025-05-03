use crate::config::{AppConfig, Recipient, SmtpConfig};
use crate::email_sender::{send_invitation_email, template::EmailTemplate};
use eframe::egui;
// Removed unused imports from previous styling attempts
use egui::{Color32, Margin, Style, Vec2, Visuals};
use secrecy::{ExposeSecret, SecretString};
use std::sync::mpsc;
use std::thread;
use tokio::runtime::Runtime;

// Enum definition remains the same...
enum Message {
    EmailSent(String),
    EmailFailed(String, String),
    FinishedSending(usize, usize),
    ConfigLoaded(Result<AppConfig, String>),
    TemplateLoaded(Result<(String, String), String>),
}

// UIRecipient struct remains the same...
#[derive(Clone)]
struct UIRecipient {
    name: String,
    email: String,
}

// MyApp struct definition remains the same...
pub struct MyApp {
    // Configuration State
    smtp_host: String,
    smtp_port_str: String,
    smtp_user: String,
    smtp_password: SecretString,
    from_email: String,
    sender_name: String,

    // Email Content State
    email_subject: String,
    email_body: String,

    // Recipient State
    recipients: Vec<UIRecipient>,
    new_recipient_name: String,
    new_recipient_email: String,

    // Application Status
    status_message: String,
    is_sending: bool,
    config_loaded: bool,
    template_loaded: bool,

    // Background Communication
    tokio_rt: Option<Runtime>,
    receiver: mpsc::Receiver<Message>,
    sender: mpsc::Sender<Message>,
}

// Default implementation remains the same...
impl Default for MyApp {
    fn default() -> Self {
        let (sender, receiver) = mpsc::channel();

        let initial_sender = sender.clone();
        thread::spawn(move || {
            match AppConfig::load() {
                Ok(config) => {
                    let config_clone = config.clone(); // Clone for template loading
                    initial_sender.send(Message::ConfigLoaded(Ok(config))).ok();
                    match EmailTemplate::load(&config_clone.sender.template_path) {
                        Ok(template) => {
                            initial_sender
                                .send(Message::TemplateLoaded(Ok((
                                    template.subject_template,
                                    template.body_template,
                                ))))
                                .ok();
                        }
                        Err(e) => {
                            initial_sender
                                .send(Message::TemplateLoaded(Err(format!(
                                    "Failed to load template: {}",
                                    e
                                ))))
                                .ok();
                        }
                    }
                }
                Err(e) => {
                    initial_sender
                        .send(Message::ConfigLoaded(Err(format!(
                            "Failed to load config: {}",
                            e
                        ))))
                        .ok();
                    initial_sender
                        .send(Message::TemplateLoaded(Err(
                            "Template not loaded due to config error".to_string(),
                        )))
                        .ok();
                }
            }
        });

        Self {
            smtp_host: String::new(),
            smtp_port_str: String::new(),
            smtp_user: String::new(),
            smtp_password: SecretString::new("".to_string().into()),
            from_email: String::new(),
            sender_name: String::new(),
            email_subject: String::new(),
            email_body: String::new(),
            recipients: Vec::new(),
            new_recipient_name: String::new(),
            new_recipient_email: String::new(),
            status_message: "Loading configuration...".to_string(),
            is_sending: false,
            config_loaded: false,
            template_loaded: false,
            tokio_rt: None,
            receiver,
            sender,
        }
    }
}

// Implementation of MyApp methods remains largely the same...
impl MyApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        // Customize egui here with cc.egui_ctx.set_fonts and cc.egui_ctx.set_visuals.
        // Restore app state using cc.storage (requires the "persistence" feature).
        // Use the cc.gl (a glow::Context) to create graphics shaders and buffers that you can use
        // for e.g. egui::PaintCallback.
        let mut style = (*egui::Context::default().style()).clone(); // Start with default style
        style.visuals = Visuals::light(); // Apply light visuals preset
        style.visuals.panel_fill = Color32::from_rgb(0xFC, 0xFC, 0xFC);
        style.visuals.window_fill = Color32::from_rgb(0xFC, 0xFC, 0xFC);
        style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(0xF3, 0xF4, 0xF5);
        style.visuals.override_text_color = Some(Color32::from_rgb(0x5C, 0x67, 0x73));
        style.visuals.extreme_bg_color = egui::Color32::from_rgb(45, 51, 59);

        style.visuals.faint_bg_color = egui::Color32::from_rgb(45, 51, 59);
        style.visuals.code_bg_color = egui::Color32::from_rgb(45, 51, 59);
        style.visuals.hyperlink_color = egui::Color32::from_rgb(255, 0, 0);
        style.visuals.override_text_color = Some(egui::Color32::from_rgb(173, 186, 199));
        style.visuals.window_corner_radius = 10.into();
        style.visuals.button_frame = true;
        style.visuals.collapsing_header_frame = true;
        style.visuals.widgets.noninteractive.bg_fill = egui::Color32::from_rgb(35, 39, 46);
        style.visuals.widgets.noninteractive.fg_stroke =
            egui::Stroke::new(0., egui::Color32::from_rgb(173, 186, 199));
        style.visuals.widgets.inactive.bg_fill = egui::Color32::TRANSPARENT;
        style.visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(45, 51, 59);
        style.visuals.widgets.active.bg_fill = egui::Color32::from_rgb(45, 51, 59);
        style.visuals.widgets.open.bg_fill = egui::Color32::from_rgb(45, 51, 59);

        cc.egui_ctx.set_style(style);
        cc.egui_ctx.set_visuals(egui::Visuals::light());
        cc.egui_ctx.set_theme(egui::Theme::Light);
        Self::default()
    }

    fn ensure_runtime(&mut self) -> &Runtime {
        self.tokio_rt.get_or_insert_with(|| {
            tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime")
        })
    }

    // ui_recipient_list remains the same...
    fn ui_recipient_list(&mut self, ui: &mut egui::Ui) {
        ui.heading("Recipients");
        ui.add_space(5.0);

        egui::Grid::new("add_recipient_grid")
            .num_columns(2)
            .spacing([10.0, 8.0])
            .show(ui, |ui| {
                ui.label("Name:");
                ui.text_edit_singleline(&mut self.new_recipient_name);
                ui.end_row();

                ui.label("Email:");
                ui.horizontal(|ui| {
                    ui.text_edit_singleline(&mut self.new_recipient_email);
                    if ui
                        .add_sized([60.0, 25.0], egui::Button::new("Add"))
                        .clicked()
                    {
                        if !self.new_recipient_email.is_empty()
                            && !self.new_recipient_name.is_empty()
                        {
                            if self.new_recipient_email.contains('@') {
                                self.recipients.push(UIRecipient {
                                    name: self.new_recipient_name.clone(),
                                    email: self.new_recipient_email.clone(),
                                });
                                self.new_recipient_name.clear();
                                self.new_recipient_email.clear();
                                self.status_message = "Recipient added.".to_string();
                            } else {
                                self.status_message = "Invalid email format.".to_string();
                            }
                        } else {
                            self.status_message = "Please enter both name and email.".to_string();
                        }
                    }
                });
                ui.end_row();
            });
        ui.add_space(10.0);

        ui.label("Current List:");
        egui::Frame::group(ui.style()).show(ui, |ui| {
            egui::ScrollArea::vertical()
                .max_height(150.0)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let mut recipient_to_remove = None;
                    for (index, recipient) in self.recipients.iter().enumerate() {
                        ui.horizontal(|ui| {
                            ui.label(format!("{} ({})", recipient.name, recipient.email))
                                .on_hover_text(format!("{} <{}>", recipient.name, recipient.email));
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if ui
                                        .add(
                                            egui::Button::new(
                                                egui::RichText::new("X")
                                                    .color(Color32::DARK_RED)
                                                    .size(10.0),
                                            )
                                            .frame(false),
                                        )
                                        .clicked()
                                    {
                                        recipient_to_remove = Some(index);
                                    }
                                },
                            );
                        });
                        ui.add_space(2.0);
                    }
                    if let Some(index) = recipient_to_remove {
                        self.recipients.remove(index);
                        self.status_message = "Recipient removed.".to_string();
                    }
                    if self.recipients.is_empty() {
                        ui.label("(No recipients added)");
                    }
                });
        });
    }

    // ui_smtp_settings remains the same...
    fn ui_smtp_settings(&mut self, ui: &mut egui::Ui) {
        ui.heading("SMTP Settings");
        ui.add_space(5.0);
        egui::Grid::new("smtp_grid")
            .num_columns(2)
            .spacing([10.0, 8.0])
            .show(ui, |ui| {
                ui.label("Host:");
                ui.text_edit_singleline(&mut self.smtp_host);
                ui.end_row();

                ui.label("Port:");
                ui.text_edit_singleline(&mut self.smtp_port_str);
                ui.end_row();

                ui.label("Username:");
                ui.text_edit_singleline(&mut self.smtp_user);
                ui.end_row();

                ui.label("Password:");
                let mut password_string = self.smtp_password.expose_secret();
                let response =
                    ui.add(egui::TextEdit::singleline(&mut password_string).password(true));
                if response.changed() {
                    self.smtp_password = SecretString::new(password_string.into());
                }
                ui.end_row();

                ui.label("From Email:");
                ui.text_edit_singleline(&mut self.from_email);
                ui.end_row();

                ui.label("Sender Name:");
                ui.text_edit_singleline(&mut self.sender_name);
                ui.end_row();
            });
    }

    // ui_email_message remains the same...
    fn ui_email_message(&mut self, ui: &mut egui::Ui) {
        ui.heading("Email Message");
        ui.add_space(5.0);

        ui.horizontal(|ui| {
            ui.label("Subject:");
            ui.add(
                egui::TextEdit::singleline(&mut self.email_subject).desired_width(f32::INFINITY),
            );
        });
        ui.add_space(8.0);

        ui.label("Body:");
        egui::ScrollArea::vertical()
            .auto_shrink([false, true])
            .show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut self.email_body)
                        .desired_width(f32::INFINITY)
                        .desired_rows(10)
                        .frame(true),
                );
            });
        ui.add_space(8.0);

        // Reverted button style
        if ui.button("🗓 Connect to Google Calendar").clicked() {
            self.status_message = "Google Calendar connection not implemented yet.".to_string();
        }
    }

    // handle_send_invitations remains the same...
    fn handle_send_invitations(&mut self) {
        if self.is_sending {
            self.status_message = "Already sending emails...".to_string();
            return;
        }
        if self.recipients.is_empty() {
            self.status_message = "No recipients added.".to_string();
            return;
        }

        let port = match self.smtp_port_str.parse::<u16>() {
            Ok(p) => p,
            Err(_) => {
                self.status_message = "Invalid SMTP Port number.".to_string();
                return;
            }
        };

        let smtp_config = SmtpConfig {
            host: self.smtp_host.clone(),
            port,
            user: self.smtp_user.clone(),
            password: self.smtp_password.clone(),
            from_email: self.from_email.clone(),
        };
        let recipients_to_send: Vec<Recipient> = self
            .recipients
            .iter()
            .map(|ui_r| Recipient {
                name: ui_r.name.clone(),
                email: ui_r.email.clone(),
            })
            .collect();
        let sender_name = self.sender_name.clone();
        let email_subject = self.email_subject.clone();
        let email_body = self.email_body.clone();

        let availabilities: Vec<String> = vec![
            "Mon, May 5, 10:00 AM".to_string(),
            "Wed, May 7, 3:00 PM".to_string(),
        ];

        self.is_sending = true;
        self.status_message = format!(
            "Sending emails to {} recipients...",
            recipients_to_send.len()
        );

        let rt = self.ensure_runtime().handle().clone();
        let sender_clone = self.sender.clone();

        rt.spawn(async move {
            let mut success_count = 0;
            let mut error_count = 0;

            match EmailTemplate::from_content(&email_subject, &email_body, "runtime_email") {
                Ok(runtime_template) => {
                    for recipient in recipients_to_send {
                        match send_invitation_email(
                            &smtp_config,
                            &recipient,
                            &sender_name,
                            &availabilities,
                            &runtime_template,
                        )
                        .await
                        {
                            Ok(_) => {
                                success_count += 1;
                                sender_clone.send(Message::EmailSent(recipient.email)).ok();
                            }
                            Err(e) => {
                                error_count += 1;
                                sender_clone
                                    .send(Message::EmailFailed(recipient.email, e.to_string()))
                                    .ok();
                            }
                        }
                    }
                }
                Err(template_err) => {
                    error_count = recipients_to_send.len();
                    sender_clone
                        .send(Message::EmailFailed(
                            "N/A".to_string(),
                            format!(
                                "Failed to parse UI email content as template: {}",
                                template_err
                            ),
                        ))
                        .ok();
                }
            }
            sender_clone
                .send(Message::FinishedSending(success_count, error_count))
                .ok();
        });
    }
}

// App::update implementation reverted to simpler panel structure
impl eframe::App for MyApp {
    // Removed clear_color

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Process Background Messages (remains the same)...
        while let Ok(message) = self.receiver.try_recv() {
            match message {
                Message::ConfigLoaded(Ok(config)) => {
                    self.smtp_host = config.smtp.host;
                    self.smtp_port_str = config.smtp.port.to_string();
                    self.smtp_user = config.smtp.user;
                    self.smtp_password = config.smtp.password;
                    self.from_email = config.smtp.from_email;
                    self.sender_name = config.sender.name;
                    self.recipients = config
                        .recipients
                        .into_iter()
                        .map(|r| UIRecipient {
                            name: r.name,
                            email: r.email,
                        })
                        .collect();
                    self.config_loaded = true;
                    if self.template_loaded {
                        self.status_message = "Configuration and template loaded.".to_string();
                    } else {
                        self.status_message =
                            "Configuration loaded. Waiting for template...".to_string();
                    }
                }
                Message::ConfigLoaded(Err(e)) => {
                    self.status_message = format!("ERROR loading config: {}", e);
                    self.config_loaded = true; // Mark attempt done
                }
                Message::TemplateLoaded(Ok((subject, body))) => {
                    self.email_subject = subject;
                    self.email_body = body;
                    self.template_loaded = true;
                    if self.config_loaded {
                        self.status_message = "Configuration and template loaded.".to_string();
                    } else {
                        self.status_message = "Template loaded. Waiting for config...".to_string();
                    }
                }
                Message::TemplateLoaded(Err(e)) => {
                    self.status_message = format!("ERROR loading template: {}", e);
                    self.template_loaded = true; // Mark attempt done
                }
                Message::EmailSent(email) => {
                    self.status_message = format!("Email sent successfully to {}", email);
                }
                Message::EmailFailed(email, error) => {
                    self.status_message = format!("ERROR sending to {}: {}", email, error);
                }
                Message::FinishedSending(success, errors) => {
                    self.is_sending = false;
                    self.status_message =
                        format!("Finished sending. Success: {}, Failed: {}", success, errors);
                }
            }
        }

        // --- Reverted Layout ---
        // Status bar at the bottom
        egui::TopBottomPanel::bottom("status_panel")
            .frame(egui::Frame::new().inner_margin(Margin::symmetric(10, 5))) // Add padding
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if self.is_sending {
                        ui.add(egui::Spinner::new().size(14.0));
                        ui.add_space(5.0);
                    }
                    ui.label(&self.status_message);
                });
            });

        // Right panel for recipients and settings
        egui::SidePanel::right("recipients_panel")
            .resizable(true)
            .default_width(300.0)
            .width_range(250.0..=450.0)
            .frame(egui::Frame::new().inner_margin(Margin::same(15))) // Add padding
            .show(ctx, |ui| {
                self.ui_recipient_list(ui);
                ui.add_space(20.0);
                self.ui_smtp_settings(ui);
            });

        // Central panel for the main email content and send button
        egui::CentralPanel::default()
            .frame(egui::Frame::new().inner_margin(Margin::same(15))) // Add padding
            .show(ctx, |ui| {
                ui.heading("Coffee Chat Helper"); // Add heading back
                ui.separator();
                ui.add_space(10.0);

                // Use a vertical layout to allow email body to expand
                ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                    self.ui_email_message(ui);
                    ui.add_space(15.0); // Space before send button

                    // Center the Send button
                    ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                        // Reverted button style
                        let send_button = egui::Button::new("🚀 Send Invitations")
                            .min_size(Vec2::new(ui.available_width() * 0.5, 30.0)); // Adjusted size

                        let enabled =
                            !self.is_sending && self.config_loaded && self.template_loaded;
                        if ui.add_enabled(enabled, send_button).clicked() {
                            self.handle_send_invitations();
                        }

                        if !self.config_loaded || !self.template_loaded {
                            ui.add_space(5.0);
                            ui.horizontal(|ui| {
                                ui.spinner();
                                ui.label("Waiting for initial configuration and template load...");
                            });
                        }
                    });
                });
            }); // End CentralPanel show

        // Request repaint remains the same...
        if self.is_sending {
            ctx.request_repaint();
        }
    }
}
