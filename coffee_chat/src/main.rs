// src/main.rs
mod app;
mod calendar;
mod config;
mod email_sender; // <-- Add this

use app::MyApp;
use eframe::egui;
use rustls::crypto::ring; // <-- Add for crypto provider installation

fn main() -> Result<(), eframe::Error> {
    // --- Load .env file ---
    match dotenvy::dotenv() {
        Ok(path) => println!("Loaded .env file from: {:?}", path),
        Err(_) => println!("Note: .env file not found or failed to load. Relying on config file and existing environment variables."),
    }

    // --- Install Rustls Crypto Provider ---
    ring::default_provider()
        .install_default()
        .expect("Failed to install rustls crypto provider");

    // --- Initialize logger ---
    env_logger::init();
    log::info!("Logger initialized."); // Use log crate

    // --- Native Options ---
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([900.0, 700.0])
            .with_min_inner_size([650.0, 500.0]),
        ..Default::default()
    };

    println!("Starting egui application...");

    eframe::run_native(
        "Coffee Chat Helper",
        options,
        Box::new(|cc| {
            // --- Pass CreationContext to MyApp::new ---
            // Ensure MyApp::new accepts cc and potentially sets up styles
            let app = MyApp::new(cc);
            Ok(Box::new(app))
        }),
    )
}
