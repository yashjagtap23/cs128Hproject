// src/app.rs
use crate::calendar;
use crate::config::{AppConfig, Recipient, SmtpConfig};
use crate::email_sender::{send_invitation_email, template::EmailTemplate};
use chrono::Duration;
use eframe::egui;
// Import necessary egui types for styling
use egui::{Color32, Margin, Stroke, Vec2, Visuals}; // Use CornerRadius, remove Rounding
use egui_double_slider::DoubleSlider;
use google_calendar3::CalendarHub;
use hyper_rustls::HttpsConnector;
// Use the yup_oauth2 hyper client if feature enabled, otherwise stick to manual build
#[cfg(not(feature = "yup-oauth2-hyper-client"))]
use http_body_util::Full;
#[cfg(not(feature = "yup-oauth2-hyper-client"))] // Fallback if feature not enabled
use hyper_util::client::legacy::Client;
#[cfg(feature = "yup-oauth2-hyper-client")] // Conditional compilation can be used
use yup_oauth2::hyper_client; // Only needed for manual client build

use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::rt::TokioExecutor;
use log::{debug, error, info, warn};
use secrecy::{ExposeSecret, SecretString};
use std::path::PathBuf;
use std::sync::{mpsc, Arc};
use std::thread;
use tokio::runtime::Runtime;
use yup_oauth2::{read_application_secret, InstalledFlowAuthenticator, InstalledFlowReturnMethod};

use std::future::Future;
use std::pin::Pin;
use std::process::Command;
use yup_oauth2::authenticator_delegate::InstalledFlowDelegate;

use directories_next::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::{BufReader, BufWriter}; // For efficient file reading/writing // For config directory

struct BrowserFlowDelegate;

impl InstalledFlowDelegate for BrowserFlowDelegate {
    fn present_user_url<'a>(
        &'a self,
        url: &'a str,
        _need_code: bool,
    ) -> Pin<Box<dyn Future<Output = Result<String, String>> + Send + 'a>> {
        Box::pin(async move {
            info!("Opening OAuth URL in browser: {}", url);

            // Try to open browser - first attempt with xdg-open (Linux/WSL)
            if let Err(e) = Command::new("xdg-open").arg(url).spawn() {
                warn!("Failed to open browser with xdg-open: {}", e);

                // Fallback to "open" for macOS
                if let Err(e) = Command::new("open").arg(url).spawn() {
                    warn!("Failed to open browser with open: {}", e);

                    // Fallback to "cmd.exe /c start" for Windows
                    if let Err(e) = Command::new("cmd.exe")
                        .args(&["/c", "start", "", url])
                        .spawn()
                    {
                        warn!("Failed to open browser with cmd.exe: {}", e);

                        // Last resort: Print URL and instruct user
                        println!("Please open this URL in your browser:");
                        println!("{}", url);
                    }
                }
            }

            // Return empty string because we're using HTTPRedirect flow
            // which doesn't need a manual code entry
            Ok(String::new())
        })
    }
}

// --- Define types based on yup-oauth2 feature ---

// Common connector type used by hyper-rustls
pub type HttpConnector = hyper_util::client::legacy::connect::HttpConnector;
pub type TokioConnector = HttpsConnector<HttpConnector>;

// Define client and hub types - Adjust based on how client is created
// If using yup-oauth2 hyper_client builder, the exact type might be simpler:
// type CalendarClient = yup_oauth2::hyper_client::Client; <- Check yup_oauth2 docs
// For now, assume manual build path OR yup-oauth2 handles it internally.
// The key is that CalendarHub::new needs compatible types.
// We define TokioConnector, and let CalendarHub handle the client generics if possible.
pub type AppCalendarHub = Arc<CalendarHub<TokioConnector>>;

// --- Message Enum ---
// (Enum remains the same)
enum Message {
    EmailSent(String),
    EmailFailed(String, String),
    FinishedSending(usize, usize),
    ConfigLoaded(Result<AppConfig, String>),
    TemplateLoaded(Result<(String, String), String>),
    CalendarConnected(AppCalendarHub),
    CalendarConnectionFailed(String),
    SlotsFetched(Vec<String>),
    SlotsFetchFailed(String),
}

// --- UIRecipient ---
// (Struct remains the same)
#[derive(Clone, Serialize, Deserialize)]
struct UIRecipient {
    name: String,
    email: String,
}

struct SavedAppState {
    smtp_host: String,
    smtp_port_str: String,
    smtp_user: String,
    smtp_password: SecretString, // Requires 'serde' feature for secrecy crate
    from_email: String,
    sender_name: String,
    email_subject: String,
    email_body: String,
    recipients: Vec<UIRecipient>,
    calendar_buffer_minutes: u32,
    day_start_hour: u32,
    day_end_hour: u32,
    // Optional: Persist these if they should be remembered across sessions
    // credentials_path: String,
    // token_cache_path: String,
}

// --- Manual Serialize Implementation ---
impl Serialize for SavedAppState {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        // Define the number of fields
        let mut state = serializer.serialize_struct("SavedAppState", 12)?; // Update count if fields change

        state.serialize_field("smtp_host", &self.smtp_host)?;
        state.serialize_field("smtp_port_str", &self.smtp_port_str)?;
        state.serialize_field("smtp_user", &self.smtp_user)?;
        // Expose the secret *before* serializing the inner String
        state.serialize_field("smtp_password", self.smtp_password.expose_secret())?;
        state.serialize_field("from_email", &self.from_email)?;
        state.serialize_field("sender_name", &self.sender_name)?;
        state.serialize_field("email_subject", &self.email_subject)?;
        state.serialize_field("email_body", &self.email_body)?;
        state.serialize_field("recipients", &self.recipients)?; // Vec<UIRecipient> needs UIRecipient to derive Serialize
        state.serialize_field("calendar_buffer_minutes", &self.calendar_buffer_minutes)?;
        state.serialize_field("day_start_hour", &self.day_start_hour)?;
        state.serialize_field("day_end_hour", &self.day_end_hour)?;
        // Add optional fields here if saving them:
        // state.serialize_field("credentials_path", &self.credentials_path)?;
        // state.serialize_field("token_cache_path", &self.token_cache_path)?;

        state.end()
    }
}

// --- Manual Deserialize Implementation ---
impl<'de> Deserialize<'de> for SavedAppState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        // Define the fields we expect
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "snake_case")] // Match field names used in Serialize
        enum Field {
            SmtpHost,
            SmtpPortStr,
            SmtpUser,
            SmtpPassword,
            FromEmail,
            SenderName,
            EmailSubject,
            EmailBody,
            Recipients,
            CalendarBufferMinutes,
            DayStartHour,
            DayEndHour, /* , CredentialsPath, TokenCachePath */
        }

        struct SavedAppStateVisitor;

        impl<'de> serde::de::Visitor<'de> for SavedAppStateVisitor {
            type Value = SavedAppState;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("struct SavedAppState")
            }

            fn visit_map<V>(self, mut map: V) -> Result<SavedAppState, V::Error>
            where
                V: serde::de::MapAccess<'de>,
            {
                // Use Option for each field to track if it was seen
                let mut smtp_host = None;
                let mut smtp_port_str = None;
                let mut smtp_user = None;
                let mut smtp_password_str: Option<String> = None; // Deserialize password as String first
                let mut from_email = None;
                let mut sender_name = None;
                let mut email_subject = None;
                let mut email_body = None;
                let mut recipients = None;
                let mut calendar_buffer_minutes = None;
                let mut day_start_hour = None;
                let mut day_end_hour = None;
                // let mut credentials_path = None;
                // let mut token_cache_path = None;

                // Loop through the map data provided by the deserializer
                while let Some(key) = map.next_key()? {
                    match key {
                        Field::SmtpHost => {
                            if smtp_host.is_some() {
                                return Err(serde::de::Error::duplicate_field("smtp_host"));
                            }
                            smtp_host = Some(map.next_value()?);
                        }
                        Field::SmtpPortStr => {
                            if smtp_port_str.is_some() {
                                return Err(serde::de::Error::duplicate_field("smtp_port_str"));
                            }
                            smtp_port_str = Some(map.next_value()?);
                        }
                        Field::SmtpUser => {
                            if smtp_user.is_some() {
                                return Err(serde::de::Error::duplicate_field("smtp_user"));
                            }
                            smtp_user = Some(map.next_value()?);
                        }
                        // Deserialize password as a String
                        Field::SmtpPassword => {
                            if smtp_password_str.is_some() {
                                return Err(serde::de::Error::duplicate_field("smtp_password"));
                            }
                            smtp_password_str = Some(map.next_value()?);
                        }
                        Field::FromEmail => {
                            if from_email.is_some() {
                                return Err(serde::de::Error::duplicate_field("from_email"));
                            }
                            from_email = Some(map.next_value()?);
                        }
                        Field::SenderName => {
                            if sender_name.is_some() {
                                return Err(serde::de::Error::duplicate_field("sender_name"));
                            }
                            sender_name = Some(map.next_value()?);
                        }
                        Field::EmailSubject => {
                            if email_subject.is_some() {
                                return Err(serde::de::Error::duplicate_field("email_subject"));
                            }
                            email_subject = Some(map.next_value()?);
                        }
                        Field::EmailBody => {
                            if email_body.is_some() {
                                return Err(serde::de::Error::duplicate_field("email_body"));
                            }
                            email_body = Some(map.next_value()?);
                        }
                        Field::Recipients => {
                            if recipients.is_some() {
                                return Err(serde::de::Error::duplicate_field("recipients"));
                            }
                            recipients = Some(map.next_value()?);
                        } // UIRecipient needs derive Deserialize
                        Field::CalendarBufferMinutes => {
                            if calendar_buffer_minutes.is_some() {
                                return Err(serde::de::Error::duplicate_field(
                                    "calendar_buffer_minutes",
                                ));
                            }
                            calendar_buffer_minutes = Some(map.next_value()?);
                        }
                        Field::DayStartHour => {
                            if day_start_hour.is_some() {
                                return Err(serde::de::Error::duplicate_field("day_start_hour"));
                            }
                            day_start_hour = Some(map.next_value()?);
                        }
                        Field::DayEndHour => {
                            if day_end_hour.is_some() {
                                return Err(serde::de::Error::duplicate_field("day_end_hour"));
                            }
                            day_end_hour = Some(map.next_value()?);
                        } // Add optional fields here if saving them
                          // Field::CredentialsPath => { if credentials_path.is_some() { return Err(serde::de::Error::duplicate_field("credentials_path")); } credentials_path = Some(map.next_value()?); }
                          // Field::TokenCachePath => { if token_cache_path.is_some() { return Err(serde::de::Error::duplicate_field("token_cache_path")); } token_cache_path = Some(map.next_value()?); }
                    }
                }

                // Check that all required fields were found and unwrap them
                let smtp_host =
                    smtp_host.ok_or_else(|| serde::de::Error::missing_field("smtp_host"))?;
                let smtp_port_str = smtp_port_str
                    .ok_or_else(|| serde::de::Error::missing_field("smtp_port_str"))?;
                let smtp_user =
                    smtp_user.ok_or_else(|| serde::de::Error::missing_field("smtp_user"))?;
                let smtp_password_str = smtp_password_str
                    .ok_or_else(|| serde::de::Error::missing_field("smtp_password"))?;
                let from_email =
                    from_email.ok_or_else(|| serde::de::Error::missing_field("from_email"))?;
                let sender_name =
                    sender_name.ok_or_else(|| serde::de::Error::missing_field("sender_name"))?;
                let email_subject = email_subject
                    .ok_or_else(|| serde::de::Error::missing_field("email_subject"))?;
                let email_body =
                    email_body.ok_or_else(|| serde::de::Error::missing_field("email_body"))?;
                let recipients =
                    recipients.ok_or_else(|| serde::de::Error::missing_field("recipients"))?;
                let calendar_buffer_minutes = calendar_buffer_minutes
                    .ok_or_else(|| serde::de::Error::missing_field("calendar_buffer_minutes"))?;
                let day_start_hour = day_start_hour
                    .ok_or_else(|| serde::de::Error::missing_field("day_start_hour"))?;
                let day_end_hour =
                    day_end_hour.ok_or_else(|| serde::de::Error::missing_field("day_end_hour"))?;
                // Unwrap optional fields here if saving them
                // let credentials_path = credentials_path.ok_or_else(|| serde::de::Error::missing_field("credentials_path"))?;
                // let token_cache_path = token_cache_path.ok_or_else(|| serde::de::Error::missing_field("token_cache_path"))?;

                // Construct the SavedAppState, wrapping the password String in SecretString
                Ok(SavedAppState {
                    smtp_host,
                    smtp_port_str,
                    smtp_user,
                    smtp_password: SecretString::new(smtp_password_str.into()), // Wrap here
                    from_email,
                    sender_name,
                    email_subject,
                    email_body,
                    recipients,
                    calendar_buffer_minutes,
                    day_start_hour,
                    day_end_hour,
                    // Add optional fields here if saving them
                    // credentials_path,
                    // token_cache_path,
                })
            }
        }

        // Define the field names for the deserializer
        const FIELDS: &'static [&'static str] = &[
            "smtp_host",
            "smtp_port_str",
            "smtp_user",
            "smtp_password",
            "from_email",
            "sender_name",
            "email_subject",
            "email_body",
            "recipients",
            "calendar_buffer_minutes",
            "day_start_hour",
            "day_end_hour", /* "credentials_path", "token_cache_path" */
        ];
        deserializer.deserialize_struct("SavedAppState", FIELDS, SavedAppStateVisitor)
    }
}

// --- MyApp Struct ---
// (Struct remains the same)
pub struct MyApp {
    // Configuration State
    smtp_host: String,
    smtp_port_str: String,
    smtp_user: String,
    smtp_password: SecretString,
    from_email: String,
    sender_name: String,
    template_path: PathBuf,

    // Email Content State
    email_subject: String,
    email_body: String,

    // Recipient State
    recipients: Vec<UIRecipient>,
    new_recipient_name: String,
    new_recipient_email: String,

    // Calendar State
    calendar_hub: Option<AppCalendarHub>,
    calendar_status: String,
    available_slots: Vec<String>,
    is_connecting_calendar: bool,
    is_fetching_slots: bool,
    credentials_path: String,
    token_cache_path: String,
    calendar_buffer_minutes: u32, // New: Buffer in minutes
    day_start_hour: u32,          // New: Start hour (0-23)
    day_end_hour: u32,            // New: End hour (0-23)

    // Application Status
    status_message: String,
    is_sending_email: bool,
    config_loaded: bool,
    template_loaded: bool,
    state_loaded_from_file: bool,

    // Background Communication
    tokio_rt: Option<Runtime>,
    receiver: mpsc::Receiver<Message>,
    sender: mpsc::Sender<Message>,
}

// --- Default Implementation ---
impl Default for MyApp {
    fn default() -> Self {
        let (sender, receiver) = mpsc::channel();

        // --- Initial config/template loading task ---
        // This still runs in the background after app starts
        let initial_sender = sender.clone();
        thread::spawn(move || {
            match AppConfig::load() {
                // Tries to load config.toml
                Ok(config) => {
                    let config_clone = config.clone();
                    // Send message even if state loaded later, App::update decides how to use it
                    initial_sender.send(Message::ConfigLoaded(Ok(config))).ok();
                    match EmailTemplate::load(&config_clone.sender.template_path) {
                        // Tries to load template
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
                                    "Failed to load template initially: {}",
                                    e
                                ))))
                                .ok();
                        }
                    }
                }
                Err(e) => {
                    initial_sender
                        .send(Message::ConfigLoaded(Err(format!(
                            "Failed to load config initially: {}",
                            e
                        ))))
                        .ok();
                    initial_sender
                        .send(Message::TemplateLoaded(Err(
                            "Template not loaded (initial config error)".to_string(),
                        )))
                        .ok();
                }
            }
        });
        // --- End initial loading task ---

        // Set initial default values for the struct fields
        Self {
            smtp_host: String::new(),
            smtp_port_str: "587".to_string(),
            smtp_user: String::new(),
            smtp_password: SecretString::new("".to_string().into()),
            from_email: String::new(),
            sender_name: String::new(),
            template_path: PathBuf::from("email_template.txt"), // Default path
            email_subject: "Coffee Chat Invitation".to_string(), // Default subject
            email_body: "Hi {{recipient_name}},\n\nWould you be available for a brief coffee chat sometime soon?\n\nMy availability:\n{{availabilities}}\n\nBest,\n{{sender_name}}".to_string(), // Default body
            recipients: Vec::new(),
            new_recipient_name: String::new(),
            new_recipient_email: String::new(),
            calendar_hub: None,
            calendar_status: "Calendar: Not Connected".to_string(),
            available_slots: Vec::new(),
            is_connecting_calendar: false,
            is_fetching_slots: false,
            credentials_path: "credentials.json".to_string(),
            token_cache_path: "tokencache.json".to_string(),
            calendar_buffer_minutes: 15,
            day_start_hour: 9,
            day_end_hour: 17,
            status_message: "Initializing...".to_string(), // Changed initial message
            is_sending_email: false,
            config_loaded: false, // Not processed yet
            template_loaded: false, // Not processed yet
            state_loaded_from_file: false, // Initialize flag to false
            tokio_rt: None,
            receiver,
            sender,
        }
    }
}

// --- MyApp Implementation ---
impl MyApp {
    // --- Constructor `new` with Theme Fixes ---
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        Self::configure_visuals(cc);
        let mut app = Self::default();

        // --- Load State Manually from File ---
        if let Some(proj_dirs) = ProjectDirs::from("com", "YourOrg", "CoffeeChatHelper") {
            // Adjust qualifier/org/app names
            let config_dir = proj_dirs.config_dir();
            let state_path = config_dir.join("app_state.json");

            if state_path.exists() {
                info!("Attempting to load state from: {:?}", state_path);
                match fs::File::open(&state_path) {
                    Ok(file) => {
                        let reader = BufReader::new(file);
                        match serde_json::from_reader::<_, SavedAppState>(reader) {
                            Ok(loaded_state) => {
                                info!("Successfully loaded saved application state from file.");
                                // Overwrite default fields with loaded state
                                app.smtp_host = loaded_state.smtp_host;
                                app.smtp_port_str = loaded_state.smtp_port_str;
                                app.smtp_user = loaded_state.smtp_user;
                                app.smtp_password = loaded_state.smtp_password;
                                app.from_email = loaded_state.from_email;
                                app.sender_name = loaded_state.sender_name;
                                app.email_subject = loaded_state.email_subject;
                                app.email_body = loaded_state.email_body;
                                app.recipients = loaded_state.recipients;
                                app.calendar_buffer_minutes = loaded_state.calendar_buffer_minutes;
                                app.day_start_hour = loaded_state.day_start_hour;
                                app.day_end_hour = loaded_state.day_end_hour;
                                // Optional load paths
                                // app.credentials_path = loaded_state.credentials_path;
                                // app.token_cache_path = loaded_state.token_cache_path;
                                app.status_message = "Loaded previous session state.".to_string();
                                app.state_loaded_from_file = true;
                            }
                            Err(e) => {
                                warn!(
                                    "Failed to deserialize state file {:?}: {}. Using defaults.",
                                    state_path, e
                                );
                                app.status_message =
                                    "Failed to load saved state. Using defaults.".to_string();
                                // Optionally delete the corrupt file?
                                // fs::remove_file(state_path).ok();
                            }
                        }
                    }
                    Err(e) => {
                        warn!(
                            "Failed to open state file {:?}: {}. Using defaults.",
                            state_path, e
                        );
                        app.status_message =
                            "Couldn't open state file. Using defaults.".to_string();
                    }
                }
            } else {
                info!("No state file found at {:?}. Using defaults.", state_path);
                app.status_message = "No saved state found. Using defaults.".to_string();
            }
        } else {
            warn!("Could not determine project directory for saving state. Using defaults.");
            app.status_message = "State saving unavailable. Using defaults.".to_string();
        }
        app.ensure_runtime();
        info!("Tokio runtime ensured.");
        app
    }

    fn configure_visuals(cc: &eframe::CreationContext<'_>) {
        let mut style = (*cc.egui_ctx.style()).clone();

        // Define Ayu Light theme colors (same as before)
        let bg_color = Color32::from_rgb(250, 250, 250);
        let panel_color = Color32::from_rgb(252, 252, 252);
        let accent_color = Color32::from_rgb(255, 184, 108);
        let text_color = Color32::from_rgb(75, 80, 92);
        let faint_text_color = Color32::from_rgb(138, 143, 153);
        let border_color = Color32::from_rgb(224, 224, 224);
        let widget_bg_inactive = Color32::from_rgb(240, 240, 240);
        let widget_bg_hovered = Color32::from_rgb(230, 230, 230);
        let widget_bg_active = Color32::from_rgb(220, 220, 220);
        let selection_color = accent_color.linear_multiply(0.3);
        let error_color = Color32::from_rgb(255, 77, 77);

        // Create custom light visuals based on Ayu
        let mut visuals = Visuals::light();
        visuals.override_text_color = Some(text_color);
        visuals.widgets.noninteractive.fg_stroke = Stroke::new(1.0, text_color);
        visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, faint_text_color);
        visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, text_color);
        visuals.widgets.active.fg_stroke = Stroke::new(1.0, text_color);

        // Backgrounds
        visuals.window_fill = bg_color;
        visuals.panel_fill = panel_color;
        visuals.extreme_bg_color = bg_color;
        visuals.faint_bg_color = Color32::from_rgb(245, 245, 245);

        // Widget backgrounds & strokes
        visuals.widgets.noninteractive.bg_fill = panel_color;
        visuals.widgets.noninteractive.bg_stroke = Stroke::NONE;
        visuals.widgets.inactive.bg_fill = widget_bg_inactive;
        visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, border_color);
        visuals.widgets.hovered.bg_fill = widget_bg_hovered;
        visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, border_color.gamma_multiply(1.2));
        visuals.widgets.active.bg_fill = widget_bg_active;
        visuals.widgets.active.bg_stroke = Stroke::new(1.0, border_color.gamma_multiply(1.5));

        // Selection, Links, Errors
        visuals.selection.bg_fill = selection_color;
        visuals.selection.stroke = Stroke::new(1.0, accent_color);
        visuals.hyperlink_color = Color32::from_rgb(51, 153, 255);
        visuals.error_fg_color = error_color;
        visuals.warn_fg_color = accent_color;

        // Window & Panel Appearance
        visuals.window_stroke = Stroke::new(1.0, border_color);
        // FIX: Replace Shadow::small_light()
        visuals.window_shadow = egui::epaint::Shadow::NONE;
        visuals.popup_shadow = egui::epaint::Shadow::NONE; // Also fix popup shadow

        // Apply the custom visuals to the style FIRST
        style.visuals = visuals; // Assign fixed visuals to style

        // Spacing adjustments (using f32)
        style.spacing.item_spacing = Vec2::new(8.0, 6.0);
        style.spacing.button_padding = Vec2::new(10.0, 5.0);
        style.spacing.interact_size = Vec2::new(40.0, 20.0);

        // Apply the fully customized style to the context
        cc.egui_ctx.set_style(style);
        info!("Visuals configured.")
    }

    // (ensure_runtime remains the same)
    fn ensure_runtime(&mut self) -> &Runtime {
        self.tokio_rt.get_or_insert_with(|| {
            info!("Creating Tokio runtime.");
            tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime")
        })
    }

    // --- NEW: Method to save state manually ---
    fn save_state(&self) {
        info!("Attempting to save application state...");
        if let Some(proj_dirs) = ProjectDirs::from("com", "YourOrg", "CoffeeChatHelper") {
            // Use same identifiers as in new()
            let config_dir = proj_dirs.config_dir();
            // Ensure config directory exists
            if let Err(e) = fs::create_dir_all(config_dir) {
                error!("Failed to create config directory {:?}: {}", config_dir, e);
                return;
            }

            let state_path = config_dir.join("app_state.json");

            // Create the state object
            let state_to_save = SavedAppState {
                smtp_host: self.smtp_host.clone(),
                smtp_port_str: self.smtp_port_str.clone(),
                smtp_user: self.smtp_user.clone(),
                smtp_password: self.smtp_password.clone(),
                from_email: self.from_email.clone(),
                sender_name: self.sender_name.clone(),
                email_subject: self.email_subject.clone(),
                email_body: self.email_body.clone(),
                recipients: self.recipients.clone(),
                calendar_buffer_minutes: self.calendar_buffer_minutes,
                day_start_hour: self.day_start_hour,
                day_end_hour: self.day_end_hour,
                // Optional save paths
                // credentials_path: self.credentials_path.clone(),
                // token_cache_path: self.token_cache_path.clone(),
            };

            // Attempt to write the file
            match fs::File::create(&state_path) {
                Ok(file) => {
                    let writer = BufWriter::new(file);
                    match serde_json::to_writer_pretty(writer, &state_to_save) {
                        Ok(_) => info!("Application state saved successfully to {:?}", state_path),
                        Err(e) => {
                            error!("Failed to serialize state to file {:?}: {}", state_path, e)
                        }
                    }
                }
                Err(e) => error!("Failed to create state file {:?}: {}", state_path, e),
            }
        } else {
            error!("Could not determine project directory for saving state.");
        }
    }
    // --- UI Sections ---

    // (ui_recipient_list remains the same)
    fn ui_recipient_list(&mut self, ui: &mut egui::Ui) {
        ui.heading("Recipients");
        ui.add_space(5.0);
        egui::Grid::new("add_recipient_grid")
            .num_columns(2)
            .spacing([10.0, 8.0])
            .show(ui, |ui| {
                ui.label("Name:");
                ui.text_edit_singleline(&mut self.new_recipient_name)
                    .on_hover_text("Enter recipient's first name");
                ui.end_row();
                ui.label("Email:");
                ui.horizontal(|ui| {
                    ui.text_edit_singleline(&mut self.new_recipient_email)
                        .on_hover_text("Enter recipient's email address");
                    if ui
                        .add_sized([60.0, 25.0], egui::Button::new("Add"))
                        .on_hover_text("Add recipient to the list")
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
                                    let remove_button = egui::Button::new(
                                        egui::RichText::new("X")
                                            .color(ui.style().visuals.error_fg_color)
                                            .small(),
                                    )
                                    .frame(false)
                                    .small();
                                    if ui
                                        .add(remove_button)
                                        .on_hover_text("Remove recipient")
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
                        ui.colored_label(
                            ui.style().visuals.widgets.inactive.fg_stroke.color,
                            "(No recipients added)",
                        );
                    }
                });
        });
    }

    // FIX: Second SecretString::new type mismatch
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
                let response = ui.add(
                    egui::TextEdit::singleline(&mut password_string)
                        .password(true)
                        .hint_text("Enter SMTP password"),
                );
                if response.changed() {
                    // FIX: Use .into() here as well
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

    // (ui_email_message remains the same)
    fn ui_email_message(&mut self, ui: &mut egui::Ui) {
        ui.heading("Email Message & Calendar");
        ui.add_space(5.0);

        // --- Email Subject ---
        ui.horizontal(|ui| {
            ui.label("Subject:");
            ui.add(
                egui::TextEdit::singleline(&mut self.email_subject).desired_width(f32::INFINITY),
            );
        });
        ui.add_space(8.0);

        // --- Email Body ---
        ui.label("Body:");
        egui::ScrollArea::vertical()
        .id_salt("email_body_scroll")
        .max_height(200.0)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.add(
                egui::TextEdit::multiline(&mut self.email_body)
                    .desired_width(f32::INFINITY)
                    .desired_rows(8)
                    .hint_text("Enter email body here. Use {{recipient_name}}, {{sender_name}}, and {{availabilities}} as placeholders.")
                    .frame(true),
            );
        });
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(10.0);

        // --- Calendar Connection ---
        ui.horizontal(|ui| {
            let connect_button_text = if self.calendar_hub.is_some() {
                "✅ Calendar Connected"
            } else {
                "📅 Connect Google Calendar"
            };
            let connect_button = egui::Button::new(connect_button_text);
            if ui
                .add_enabled(!self.is_connecting_calendar, connect_button)
                .on_hover_text(if self.calendar_hub.is_some() {
                    "Calendar is connected"
                } else {
                    "Connect to Google Calendar to fetch availability"
                })
                .clicked()
            {
                if self.calendar_hub.is_none() {
                    self.handle_connect_calendar();
                } else {
                    self.status_message = "Calendar already connected.".to_string();
                }
            }
            if self.is_connecting_calendar {
                ui.add(egui::Spinner::new().size(16.0));
                ui.label("Connecting...");
            } else {
                ui.label(&self.calendar_status);
            }
        });
        ui.add_space(10.0);

        // --- Calendar Settings (Collapsible Section) ---
        ui.collapsing("Calendar Settings", |ui| {
            egui::Grid::new("calendar_settings_grid")
                .num_columns(3)
                .spacing([10.0, 8.0])
                .show(ui, |ui| {
                    // --- Buffer Setting ---
                    ui.label("Buffer Time:"); // Label
                    ui.add(
                        // Standard Slider
                        egui::Slider::new(&mut self.calendar_buffer_minutes, 0..=60) // Range 0-60 mins
                            .show_value(false), // Don't show value on slider itself
                    );
                    ui.add(
                        // Text input (DragValue) for precise control
                        egui::DragValue::new(&mut self.calendar_buffer_minutes)
                            .speed(1.0)
                            .range(0..=120) // Use .range (corrected)
                            .suffix(" min"), // Add units
                    );
                    ui.end_row();

                    // --- Day Start/End Time Setting ---
                    ui.label("Daily Availability:");

                    // Combine Slider and Text Edits horizontally
                    ui.horizontal(|ui| {
                        // Use DoubleSlider
                        ui.add(DoubleSlider::new(
                            // Takes two mutable references and the full range
                            &mut self.day_start_hour,
                            &mut self.day_end_hour,
                            0..=23, // The total possible range
                        ));

                        // Add some spacing
                        ui.add_space(10.0);

                        // Text boxes (DragValue) for precise start/end hour input
                        ui.label("From:");
                        let start_resp = ui.add(
                            egui::DragValue::new(&mut self.day_start_hour)
                                .speed(1.0)
                                .range(0..=22)
                                .suffix(":00"),
                        );
                        ui.label(" To:");
                        let end_resp = ui.add(
                            egui::DragValue::new(&mut self.day_end_hour)
                                .speed(1.0)
                                .range(1..=23)
                                .suffix(":00"),
                        );

                        // Re-validate if text boxes or slider changed, ensuring start < end
                        if start_resp.changed() || end_resp.changed() {
                            if self.day_start_hour >= self.day_end_hour {
                                self.day_end_hour = (self.day_start_hour + 1).min(23);
                            }
                        }
                    });
                    ui.end_row();
                });
        });
        ui.add_space(10.0);

        // --- Fetch Slots Button ---
        ui.horizontal(|ui| {
            let fetch_button = egui::Button::new("🔄 Fetch Slots");
            if ui
                .add_enabled(
                    self.calendar_hub.is_some() && !self.is_fetching_slots,
                    fetch_button,
                )
                .on_hover_text("Fetch available time slots using current settings")
                .clicked()
            {
                self.handle_fetch_slots(); // Ensure only one definition of this exists
            }
            if self.is_fetching_slots {
                ui.add(egui::Spinner::new().size(16.0));
                ui.label("Fetching...");
            }
        });

        // --- Available Slots Display ---
        ui.add_space(10.0);
        ui.label("Available Slots:");
        egui::Frame::group(ui.style()).show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("slots_scroll_area")
                .max_height(120.0)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if !self.available_slots.is_empty() {
                        for slot in &self.available_slots {
                            ui.label(slot);
                        }
                    } else if self.calendar_hub.is_some()
                        && !self.is_fetching_slots
                        && !self.is_connecting_calendar
                    {
                        ui.colored_label(
                            ui.style().visuals.widgets.inactive.fg_stroke.color,
                            "(No slots fetched or none available with current filters)",
                        );
                    } else if self.calendar_hub.is_none() {
                        ui.colored_label(
                            ui.style().visuals.widgets.inactive.fg_stroke.color,
                            "(Connect calendar and fetch slots)",
                        );
                    } else if self.is_fetching_slots {
                        ui.colored_label(
                            ui.style().visuals.widgets.inactive.fg_stroke.color,
                            "(Fetching...)",
                        );
                    }
                });
        });
        ui.add_space(10.0);
        ui.separator();
    }
    // --- Async Handlers ---

    // (handle_connect_calendar remains the same)
    fn handle_connect_calendar(&mut self) {
        if self.is_connecting_calendar {
            return;
        }
        self.is_connecting_calendar = true;
        self.calendar_status = "Calendar: Connecting...".to_string();
        self.status_message =
            "Attempting to connect to Google Calendar... Check your browser.".to_string();
        self.available_slots.clear();
        let sender = self.sender.clone();
        let rt_handle = self.ensure_runtime().handle().clone();
        let creds_path = self.credentials_path.clone();
        let token_cache = self.token_cache_path.clone();
        rt_handle.spawn(async move {
            info!("Starting calendar connection task.");
            match Self::setup_calendar_hub(&creds_path, &token_cache).await {
                Ok(hub) => {
                    info!("Successfully connected to Google Calendar.");
                    sender.send(Message::CalendarConnected(Arc::new(hub))).ok();
                }
                Err(e) => {
                    error!("Failed to connect to Google Calendar: {}", e);
                    sender
                        .send(Message::CalendarConnectionFailed(format!(
                            "Calendar connection failed: {}. Check credentials/permissions.",
                            e
                        )))
                        .ok();
                }
            }
        });
    }

    // FIX: Use yup_oauth2::hyper_client Builder for correct client type
    async fn setup_calendar_hub(
        creds_path: &str,
        token_cache: &str,
    ) -> Result<CalendarHub<TokioConnector>, Box<dyn std::error::Error>> {
        info!("Reading application secret from: {}", creds_path);
        let secret = read_application_secret(PathBuf::from(creds_path)).await?;

        info!("Building authenticator (token cache: {})...", token_cache);

        // Create a custom auth flow that opens the browser automatically
        let auth =
            InstalledFlowAuthenticator::builder(secret, InstalledFlowReturnMethod::HTTPRedirect)
                .persist_tokens_to_disk(PathBuf::from(token_cache))
                .flow_delegate(Box::new(BrowserFlowDelegate {})) // Add custom flow delegate
                .build()
                .await?;

        info!("Authenticator built.");

        // Build compatible hyper client using yup-oauth2 helper
        let https = HttpsConnectorBuilder::new()
            .with_native_roots()?
            .https_only()
            .enable_http1()
            .build();

        // wrap in hyper-util client
        let client = Client::builder(hyper_util::rt::TokioExecutor::new()).build(https);

        // Use explicit typing to help with trait resolution
        let hub: CalendarHub<_> = CalendarHub::new(client, auth);

        Ok(hub)
    }

    // (handle_send_invitations remains the same)
    fn handle_send_invitations(&mut self) {
        if self.is_sending_email {
            self.status_message = "Already sending emails...".to_string();
            return;
        }
        if self.recipients.is_empty() {
            self.status_message = "Cannot send: No recipients added.".to_string();
            return;
        }
        let port = match self.smtp_port_str.parse::<u16>() {
            Ok(p) => p,
            Err(_) => {
                self.status_message = "Invalid SMTP Port number.".to_string();
                error!("Invalid SMTP port entered: {}", self.smtp_port_str);
                return;
            }
        };
        if self.available_slots.is_empty() {
            if self.calendar_hub.is_some() {
                warn!("Proceeding to send email, but no available slots were fetched or found.");
                self.status_message = "Warning: Sending email without available slots.".to_string();
            } else {
                warn!("Proceeding to send email without calendar connection/slots.");
                self.status_message = "Warning: Sending email without calendar slots.".to_string();
            }
        }
        let smtp_config = SmtpConfig {
            host: self.smtp_host.clone(),
            port,
            user: self.smtp_user.clone(),
            password: self.smtp_password.clone(),
            from_email: self.from_email.clone(),
        };
        if smtp_config.host.is_empty()
            || smtp_config.user.is_empty()
            || smtp_config.from_email.is_empty()
            || smtp_config.password.expose_secret().is_empty()
        {
            self.status_message =
                "Error: Missing required SMTP settings (Host, User, Password, From Email)."
                    .to_string();
            error!("Attempted send with incomplete SMTP config.");
            return;
        }
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
        let availabilities = self.available_slots.clone();
        self.is_sending_email = true;
        self.status_message = format!(
            "Sending emails to {} recipients...",
            recipients_to_send.len()
        );
        let rt = self.ensure_runtime().handle().clone();
        let sender_clone = self.sender.clone();
        rt.spawn(async move {
            info!("Starting email sending task.");
            let mut success_count = 0;
            let mut error_count = 0;
            match EmailTemplate::from_content(&email_subject, &email_body, "ui_template") {
                Ok(runtime_template) => {
                    debug!("Runtime template created from UI content.");
                    for recipient in recipients_to_send {
                        debug!("Attempting to send email to: {}", recipient.email);
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
                                info!("Email sent successfully to {}", recipient.email);
                                sender_clone.send(Message::EmailSent(recipient.email)).ok();
                            }
                            Err(e) => {
                                error_count += 1;
                                error!("Error sending email to {}: {}", recipient.email, e);
                                sender_clone
                                    .send(Message::EmailFailed(recipient.email, e.to_string()))
                                    .ok();
                            }
                        }
                    }
                }
                Err(template_err) => {
                    error!(
                        "Failed to create template from UI content: {}",
                        template_err
                    );
                    error_count = recipients_to_send.len();
                    sender_clone
                        .send(Message::EmailFailed(
                            "All Recipients".to_string(),
                            format!("Template Error (Subject/Body invalid): {}", template_err),
                        ))
                        .ok();
                }
            }
            info!(
                "Email sending task finished. Success: {}, Errors: {}",
                success_count, error_count
            );
            sender_clone
                .send(Message::FinishedSending(success_count, error_count))
                .ok();
        });
    }

    fn handle_fetch_slots(&mut self) {
        if self.is_fetching_slots {
            return;
        }
        if let Some(hub) = self.calendar_hub.clone() {
            self.is_fetching_slots = true;
            self.status_message = "Fetching available slots...".to_string();
            self.available_slots.clear();

            let sender = self.sender.clone();
            let rt_handle = self.ensure_runtime().handle().clone();
            let hub_clone = hub;
            // Clone the new settings
            let buffer_minutes = self.calendar_buffer_minutes;
            let start_hour = self.day_start_hour;
            let end_hour = self.day_end_hour;

            rt_handle.spawn(async move {
                info!(
                    "Starting slot fetching task with buffer={} min, hours={}-{}",
                    buffer_minutes, start_hour, end_hour
                );
                // Pass the new settings to find_available_slots
                match calendar::find_available_slots(
                    &hub_clone,
                    buffer_minutes,
                    start_hour,
                    end_hour,
                )
                .await
                {
                    Ok(free_slots) => {
                        info!(
                            "Successfully found {} raw free slots (pre-filtering).",
                            free_slots.len()
                        );
                        // Note: Summarization now happens *after* filtering inside find_available_slots
                        let summarized = calendar::free_busy::summarize_slots(
                            &free_slots,
                            Duration::minutes(30), // Keep min_len for summarization distinct
                        );
                        info!("Summarized to {} displayable slots.", summarized.len());
                        sender.send(Message::SlotsFetched(summarized)).ok();
                    }
                    Err(e) => {
                        error!("Failed to find available slots: {}", e);
                        sender
                            .send(Message::SlotsFetchFailed(format!(
                                "Failed to fetch slots: {}",
                                e
                            )))
                            .ok();
                    }
                }
            });
        } else {
            self.status_message = "Cannot fetch slots: Calendar not connected.".to_string();
            warn!("Attempted to fetch slots without calendar connection.");
        }
    }
}

// --- App::update Implementation ---
impl eframe::App for MyApp {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_state();
    }

    // FIX: Update margin calls
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // --- Process Background Messages ---
        while let Ok(message) = self.receiver.try_recv() {
            match message {
                Message::ConfigLoaded(Ok(config)) => {
                    info!("Processing initial config load message.");
                    // --- Apply config ONLY if state wasn't loaded ---
                    if !self.state_loaded_from_file {
                        info!("Applying config.toml values as no saved state was loaded.");
                        self.smtp_host = config.smtp.host;
                        self.smtp_port_str = config.smtp.port.to_string();
                        self.smtp_user = config.smtp.user;
                        self.smtp_password = config.smtp.password; // This might overwrite user input if they change password before config loads? Consider carefully.
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
                        // NOTE: We are NOT applying calendar settings from config, letting saved state rule.
                        if self.status_message.contains("Using defaults") {
                            self.status_message = "Applied defaults from config.toml.".to_string();
                        }
                    } else {
                        info!("Saved state already loaded, ignoring most values from config.toml.");
                        if self
                            .status_message
                            .contains("Loaded previous session state.")
                        {
                            self.status_message =
                                "Loaded previous session. Initial config processed.".to_string();
                        }
                    }
                    // Always update template path from config, as it's not saved in app_state.json
                    self.template_path = config.sender.template_path;
                    self.config_loaded = true; // Mark config loading sequence step as done
                    debug!("Config message processed.");
                }
                Message::ConfigLoaded(Err(e)) => {
                    error!("Initial Config loading error message received: {}", e);
                    if !self.state_loaded_from_file
                        && self.status_message.contains("Using defaults")
                    {
                        self.status_message = format!("ERROR loading initial config: {}", e);
                    }
                    self.config_loaded = true; // Mark sequence step as done
                }
                Message::TemplateLoaded(Ok((subject, body))) => {
                    info!("Processing initial template load message.");
                    // --- Apply template ONLY if state wasn't loaded ---
                    if !self.state_loaded_from_file {
                        info!("Applying template file content as no saved state was loaded.");
                        self.email_subject = subject;
                        self.email_body = body;
                        if self
                            .status_message
                            .contains("Applied defaults from config.toml")
                        {
                            self.status_message =
                                "Applied defaults from config and template.".to_string();
                        } else if self.status_message.contains("Using defaults") {
                            self.status_message = "Applied defaults from template.".to_string();
                        }
                    } else {
                        info!(
                            "Saved state already loaded, ignoring content from email_template.txt."
                        );
                        if self.status_message.contains("Initial config processed") {
                            self.status_message =
                                "Loaded previous session. Initial config/template processed."
                                    .to_string();
                        }
                    }
                    self.template_loaded = true; // Mark sequence step as done
                    debug!("Template message processed.");
                }
                Message::TemplateLoaded(Err(e)) => {
                    error!("Initial Template loading error message received: {}", e);
                    if !self.state_loaded_from_file
                        && self.status_message.contains("Using defaults")
                    {
                        self.status_message = format!("ERROR loading initial template: {}", e);
                    }
                    self.template_loaded = true; // Mark sequence step as done
                }
                Message::EmailSent(email) => {
                    debug!("UI Update: Email sent to {}", email);
                }
                Message::EmailFailed(email, error) => {
                    error!("UI Update: Email failed for {}: {}", email, error);
                    self.status_message = format!("ERROR sending to {}: {}", email, error);
                }
                Message::FinishedSending(success, errors) => {
                    info!(
                        "UI Update: Finished sending emails (Success: {}, Failed: {})",
                        success, errors
                    );
                    self.is_sending_email = false;
                    self.status_message =
                        format!("Finished sending. Success: {}, Failed: {}", success, errors);
                }
                Message::CalendarConnected(hub) => {
                    info!("UI Update: Calendar connected.");
                    self.is_connecting_calendar = false;
                    self.calendar_hub = Some(hub);
                    self.calendar_status = "Calendar: Connected".to_string();
                    self.status_message = "Successfully connected to Google Calendar.".to_string();
                    info!("Triggering automatic slot fetch after connection.");
                    self.handle_fetch_slots();
                }
                Message::CalendarConnectionFailed(error_msg) => {
                    error!("UI Update: Calendar connection failed: {}", error_msg);
                    self.is_connecting_calendar = false;
                    self.calendar_hub = None;
                    self.calendar_status = "Calendar: Connection Failed".to_string();
                    self.status_message = error_msg;
                }
                Message::SlotsFetched(slots) => {
                    info!("UI Update: Slots fetched ({} slots).", slots.len());
                    self.is_fetching_slots = false;
                    self.available_slots = slots;
                    self.status_message = format!(
                        "Fetched {} available time slots.",
                        self.available_slots.len()
                    );
                    if self.calendar_hub.is_some() {
                        self.calendar_status = "Calendar: Connected (Slots Loaded)".to_string();
                    }
                }
                Message::SlotsFetchFailed(error_msg) => {
                    error!("UI Update: Slot fetching failed: {}", error_msg);
                    self.is_fetching_slots = false;
                    self.available_slots.clear();
                    self.status_message = error_msg;
                    if self.calendar_hub.is_some() {
                        self.calendar_status = "Calendar: Connected (Slot Error)".to_string();
                    }
                }
            }
        }

        // --- UI Layout ---
        egui::TopBottomPanel::bottom("status_panel")
            // FIX: Use f32 for Margin methods
            .frame(
                egui::Frame::new()
                    .inner_margin(Margin::symmetric(10, 5))
                    .fill(ctx.style().visuals.panel_fill),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if self.is_sending_email
                        || self.is_connecting_calendar
                        || self.is_fetching_slots
                    {
                        ui.add(egui::Spinner::new().size(14.0));
                        ui.add_space(5.0);
                    }
                    ui.label(&self.status_message);
                });
            });

        egui::SidePanel::right("side_panel")
            .resizable(true)
            .default_width(300.0)
            .width_range(250.0..=450.0)
            // FIX: Use f32 for Margin methods
            .frame(
                egui::Frame::new()
                    .inner_margin(Margin::same(15))
                    .fill(ctx.style().visuals.panel_fill),
            )
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    self.ui_recipient_list(ui);
                    ui.add_space(20.0);
                    ui.separator();
                    ui.add_space(20.0);
                    self.ui_smtp_settings(ui);
                });
            });

        egui::CentralPanel::default()
             // FIX: Use f32 for Margin methods
             .frame(egui::Frame::new().inner_margin(Margin::same(15)).fill(ctx.style().visuals.panel_fill))
            .show(ctx, |ui| {
                ui.heading("Coffee Chat Helper"); ui.separator(); ui.add_space(10.0);
                // FIX: Replace Align::stretch with Align::Min
                ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                     egui::ScrollArea::vertical().id_salt("main_scroll").show(ui, |ui| { // Use id_salt if id_source deprecated
                        self.ui_email_message(ui);
                    });
                    ui.add_space(ui.available_height() * 0.05);
                     ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                         ui.add_space(10.0);
                         let send_button = egui::Button::new("🚀 Send Invitations").min_size(Vec2::new(200.0, 35.0));
                         let send_enabled = !self.is_sending_email && !self.is_connecting_calendar && !self.is_fetching_slots && self.config_loaded && self.template_loaded;
                         if ui.add_enabled(send_enabled, send_button).on_hover_text("Send emails based on current settings, template, and fetched slots").clicked() { self.handle_send_invitations(); }
                         if !self.config_loaded || !self.template_loaded {
                             ui.add_space(5.0);
                              ui.horizontal(|ui| { ui.add(egui::Spinner::new().size(12.0)); ui.colored_label(ctx.style().visuals.widgets.inactive.fg_stroke.color, "Waiting for initial config/template..."); });
                         }
                     });
                });
            });

        if self.is_sending_email || self.is_connecting_calendar || self.is_fetching_slots {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
}

// (No guard! macro needed)
