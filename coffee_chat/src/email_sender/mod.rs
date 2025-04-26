// Now brings in structs from the top-level config module
use crate::config::{Recipient, SmtpConfig};
// Use the new template module
pub mod template; // Make template module public if needed elsewhere, or keep private
use template::{EmailTemplate, TemplateError};

use lettre::{
    address::AddressError,
    // Import the general lettre error and address error
    error::Error as LettreError, // Rename to avoid conflict if needed
    transport::smtp::{
        authentication::Credentials,
        client::{Tls, TlsParameters},
    },
    Message,
    SmtpTransport,
    Transport,
};
use thiserror::Error;

// --- Error Handling ---
#[derive(Error, Debug)]
pub enum EmailError {
    #[error("Template error: {0}")]
    Template(#[from] TemplateError),

    #[error("Failed to parse email address: {0}")]
    Address(#[from] AddressError),

    // --- FIX: Handle general lettre::error::Error ---
    #[error("Failed to build email message: {0}")]
    MessageBuild(#[from] LettreError), // Use the general lettre error

    #[error("Failed to create SMTP transport: {0}")]
    TransportCreation(lettre::transport::smtp::Error),

    #[error("Failed to send email: {0}")]
    Send(lettre::transport::smtp::Error),

    #[error("Configuration error for TLS: {0}")]
    TlsConfig(String),

    #[error("General configuration error: {0}")]
    ConfigError(String),
}

// --- Public Function ---
/// Sends a coffee chat invitation email using loaded configuration and templates.
pub async fn send_invitation_email(
    smtp_config: &SmtpConfig,
    recipient: &Recipient,
    sender_name: &str,
    availabilities: &[String],
    template: &EmailTemplate,
) -> Result<(), EmailError> {
    // --- Render Email Content ---
    let (subject, body) = template.render(&recipient.name, sender_name, availabilities)?;

    // --- Email Construction (lettre::Message) ---
    let email = Message::builder()
        .from(smtp_config.from_email.parse()?) // Handles AddressError via From
        .to(recipient.email.parse()?) // Handles AddressError via From
        .subject(subject)
        // --- FIX: Use ? with LettreError ---
        .body(body)?; // Handles LettreError via From

    // --- SMTP Transport & Sending ---
    let creds = Credentials::new(
        smtp_config.user.clone(),
        smtp_config.get_password().to_string(),
    );

    let tls_parameters = TlsParameters::new(smtp_config.host.clone())
        .map_err(|e| EmailError::TlsConfig(format!("Invalid SMTP host for TLS: {}", e)))?;

    let transport = SmtpTransport::relay(&smtp_config.host)
        .map_err(EmailError::TransportCreation)?
        .port(smtp_config.port)
        .credentials(creds)
        .tls(Tls::Required(tls_parameters))
        .build();

    match transport.send(&email) {
        Ok(_) => {
            println!(
                "Email sent successfully to {} ({})!",
                recipient.name, recipient.email
            );
            Ok(())
        }
        Err(e) => {
            eprintln!("Error sending email to {}: {:?}", recipient.email, e);
            Err(EmailError::Send(e))
        }
    }
}

// --- Unit Tests ---
#[cfg(test)]
mod tests {
    // Basic tests might focus on template rendering logic now.
    // Testing the full send_invitation_email requires more setup (mocking).
}
