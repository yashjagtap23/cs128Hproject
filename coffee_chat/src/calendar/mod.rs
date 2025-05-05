// src/calendar/mod.rs
pub mod free_busy;

use crate::app::TokioConnector; // Import the type alias from app.rs
use chrono::{DateTime, Duration, Utc};
use google_calendar3::{api::TimePeriod, CalendarHub}; // Remove Connector import
use log::{debug, info}; // <-- Add this
use std::error::Error;
use std::io::{Error as IoError, ErrorKind};

// Change the function signature to use the concrete Hub type
pub async fn find_available_slots(
    hub: &CalendarHub<TokioConnector>,
    buffer_minutes: u32, // New: Buffer parameter
    start_hour: u32,     // New: Start hour
    end_hour: u32,       // New: End hour
) -> Result<Vec<(DateTime<Utc>, DateTime<Utc>)>, Box<dyn Error>> {
    info!("Fetching primary calendar ID...");
    // ... (find primary_id logic remains the same) ...
    let primary_id = {
        let (_, list) = hub.calendar_list().list().doit().await?;
        let id = list
            .items
            .and_then(|items| {
                items
                    .into_iter()
                    .find(|c| c.primary.unwrap_or(false))
                    .and_then(|c| c.id)
            })
            .ok_or_else(|| {
                // Use a more specific error type if desired, but Box<dyn Error> handles it
                Box::<dyn Error>::from("Primary calendar not found")
            })?; // Propagate error if not found
        id // This is the String value returned by the block
    };
    info!("Found primary calendar ID: {}", primary_id); // Now primary_id is String

    let now = Utc::now();
    let time_min = now;
    let time_max = now + Duration::days(14); // Look ahead 14 days

    info!(
        "Fetching busy slots for calendar '{}' between {} and {}",
        primary_id, time_min, time_max
    );
    let busy: Vec<TimePeriod> =
        free_busy::get_busy_slots(hub, &primary_id, time_min, time_max).await?;
    info!("Found {} busy periods.", busy.len());

    info!(
        "Calculating free windows with {} minute buffer...",
        buffer_minutes
    );
    // Convert minutes to Duration
    let buffer = Duration::minutes(buffer_minutes as i64);
    // Pass the buffer to find_free_windows
    let raw_windows = free_busy::find_free_windows(&busy, time_min, time_max, buffer);
    info!("Found {} raw free windows.", raw_windows.len());

    info!("Splitting windows at midnight...");
    let split_windows = free_busy::split_at_midnight(&raw_windows);
    info!(
        "Found {} free windows after splitting.",
        split_windows.len()
    );

    // --- NEW: Filter by time of day ---
    info!(
        "Filtering windows between hours {} and {}...",
        start_hour, end_hour
    );
    let filtered_windows =
        free_busy::filter_slots_by_time_of_day(&split_windows, start_hour, end_hour);
    info!(
        "Found {} windows after time filtering.",
        filtered_windows.len()
    );
    // --- End Filtering ---

    // Summarization will use the filtered slots, but it's called by the App after this returns
    // Ok(filtered_windows) // Return the filtered but unsummarized slots

    // If you prefer find_available_slots to return the *summarized* strings directly:
    // let min_summarize_duration = Duration::minutes(30); // Or make configurable
    // let summarized = free_busy::summarize_slots(&filtered_windows, min_summarize_duration);
    // Ok(summarized) // <-- Change return type to Result<Vec<String>, Box<dyn Error>> if doing this

    // Let's return the filtered slots for now, summarization happens in app.rs
    Ok(filtered_windows)
}
