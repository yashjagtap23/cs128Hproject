use anyhow::{Context, Result}; // Use anyhow for easy error handling
                               // Removed unused chrono::Utc and std::path::Path

// Declare modules
mod config;
mod email_sender;
// mod calendar_checker; // Add back when ready

// Use items from modules
use config::AppConfig;
use email_sender::{send_invitation_email, template::EmailTemplate};

#[tokio::main]
async fn main() -> Result<()> {
    println!("Starting Coffee Chat Helper...");

    // --- Load Configuration ---
    dotenvy::dotenv().ok();
    println!("Loading configuration...");
    let app_config = AppConfig::load().context("Failed to load application configuration")?;
    println!("Configuration loaded successfully.");

    // --- DEBUG: Print Loaded SMTP Configuration ---
    println!("\n--- Loaded SMTP Configuration ---");
    println!("  Host: {}", app_config.smtp.host);
    println!("  Port: {}", app_config.smtp.port);
    println!("  User: {}", app_config.smtp.user);
    println!("  From Email: {}", app_config.smtp.from_email);
    println!("  Password: {}", app_config.smtp.get_password());
    // --- End DEBUG ---

    // --- Load Email Template ---
    println!(
        "Loading email template from: {:?}",
        app_config.sender.template_path
    );
    let template = EmailTemplate::load(&app_config.sender.template_path)
        .context("Failed to load email template")?;
    println!("Email template loaded successfully.");

    // --- Get Availabilities (Placeholder) ---
    println!("Fetching availabilities (using placeholder data)...");
    let availabilities: Vec<String> = vec![
        "Monday, April 28, 2025 at 10:00 AM CDT".to_string(),
        "Tuesday, April 29, 2025 at 2:30 PM CDT".to_string(),
        "Thursday, May 1, 2025 at 9:00 AM CDT".to_string(),
    ];
    println!("Availabilities: {:?}", availabilities);

    // --- Scheduling Logic (Placeholder) ---
    if app_config.schedule.enabled {
        println!(
            "Scheduling enabled (cron: {:?}, timezone: {:?}).",
            app_config.schedule.cron_expression, app_config.schedule.timezone
        );
        // TODO: Implement actual scheduling logic here
        println!(
            "NOTE: Actual scheduling not implemented in this version. Sending emails immediately."
        );
    } else {
        println!("Scheduling disabled. Sending emails immediately.");
    }

    // --- Process and Send Emails ---
    println!("\nProcessing {} recipients...", app_config.recipients.len());
    let mut success_count = 0;
    let mut error_count = 0;

    for recipient in &app_config.recipients {
        println!(
            "\nAttempting to send email to: {} ({})",
            recipient.name, recipient.email
        );

        match send_invitation_email(
            &app_config.smtp,
            recipient,
            &app_config.sender.name,
            &availabilities,
            &template,
        )
        .await
        {
            Ok(_) => {
                println!("Successfully processed recipient: {}", recipient.name);
                success_count += 1;
                // tokio::time::sleep(std::time::Duration::from_secs(2)).await; // Optional delay
            }
            Err(e) => {
                eprintln!("ERROR sending email to {}: {}", recipient.email, e);
                error_count += 1;
            }
        }
    }

    // --- Summary ---
    println!("\n--- Sending Complete ---");
    println!("Successfully sent: {}", success_count);
    println!("Failed attempts:   {}", error_count);
    println!("Total recipients:  {}", app_config.recipients.len());

    Ok(())
}

// Placeholder modules for other components
// mod calendar_checker {}
// mod error {} // If using a central error module
