use config::{Config, ConfigError, Environment, File}; // Use the config crate
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use std::path::PathBuf; // For handling secrets like passwords

// Structure for SMTP server configuration
#[derive(Debug, Deserialize, Clone)]
pub struct SmtpConfig {
    pub host: String,
    pub port: u16,
    pub user: String,
    // Use `secrecy::Secret` for the password to prevent accidental logging
    #[serde(default)] // Make password optional in file if set by env
    pub password: SecretString,
    pub from_email: String,
}

// Structure for sender information
#[derive(Debug, Deserialize, Clone)]
pub struct SenderConfig {
    pub name: String,
    pub template_path: PathBuf, // Use PathBuf for file paths
}

// Structure for a single recipient
#[derive(Debug, Deserialize, Clone)]
pub struct Recipient {
    pub name: String,
    pub email: String,
    // Add schedule field here if needed later
}

// Optional: Structure for scheduling configuration
#[derive(Debug, Deserialize, Clone, Default)]
pub struct ScheduleConfig {
    #[serde(default)]
    pub enabled: bool,
    pub cron_expression: Option<String>,
    pub timezone: Option<String>,
}

// Top-level application configuration structure
#[derive(Debug, Deserialize, Clone)]
pub struct AppConfig {
    pub smtp: SmtpConfig,
    pub sender: SenderConfig,
    pub recipients: Vec<Recipient>,
    #[serde(default)] // Make schedule optional
    pub schedule: ScheduleConfig,
}

impl AppConfig {
    /// Loads configuration from files and environment variables.
    ///
    /// Reads configuration from:
    /// 1. `config/default.toml` (optional base defaults)
    /// 2. `config.toml` (user overrides)
    /// 3. Environment variables prefixed with `APP_` (e.g., `APP_SMTP__PASSWORD`)
    pub fn load() -> Result<Self, ConfigError> {
        // Initialize configuration builder
        let builder = Config::builder()
            // Add default configuration file (optional)
            // .add_source(File::with_name("config/default").required(false))
            // Add user configuration file (e.g., config.toml at project root)
            .add_source(File::with_name("config").required(true))
            // Add environment variables with a prefix, e.g., APP_SMTP__HOST
            // Note: `__` separates struct levels, `_` is ignored in names.
            // Example: Set SMTP password via `APP_SMTP__PASSWORD="your_secret"`
            .add_source(Environment::default().separator("_"));

        // Build the configuration
        let config = builder.build()?;

        // Deserialize the configuration into the AppConfig struct
        config.try_deserialize()
    }
}

// Example of how to access the secret password safely
impl SmtpConfig {
    pub fn get_password(&self) -> &str {
        self.password.expose_secret()
    }
}
