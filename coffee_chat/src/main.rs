// Declare modules before use
mod app;
mod config;
mod email_sender;
// mod calendar_checker;

use app::MyApp;
use eframe::egui;

fn main() -> Result<(), eframe::Error> {
    // --- Load .env file at the very beginning ---
    match dotenvy::dotenv() {
        Ok(path) => println!("Loaded .env file from: {:?}", path),
        Err(_) => println!("Note: .env file not found or failed to load. Relying on config file and existing environment variables."),
    }

    // Initialize logger (optional but helpful)
    env_logger::init();

    // --- Use simple NativeOptions ---
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([900.0, 700.0]) // Keep size settings
            .with_min_inner_size([650.0, 500.0]),
        ..Default::default() // Use eframe defaults
    };

    println!("Starting egui application...");

    eframe::run_native(
        "Coffee Chat Helper",
        options,
        // --- Setup the theme during creation ---
        Box::new(|cc| Ok(Box::new(MyApp::new(cc)))),
    )
}
