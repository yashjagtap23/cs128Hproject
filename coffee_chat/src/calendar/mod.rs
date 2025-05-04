// src/calendar/mod.rs
pub mod free_busy;

use crate::app::TokioConnector; // Import the type alias from app.rs
use chrono::{DateTime, Duration, Utc};
use google_calendar3::{api::TimePeriod, CalendarHub}; // Remove Connector import
use log::info; // <-- Add this
use std::error::Error;
use std::io::{Error as IoError, ErrorKind};

// Change the function signature to use the concrete Hub type
pub async fn find_available_slots(
    hub: &CalendarHub<TokioConnector>, // Use concrete type
) -> Result<Vec<(DateTime<Utc>, DateTime<Utc>)>, Box<dyn Error>>
// Remove the 'where C: ...' clause
{
    info!("Fetching primary calendar ID...");
    // Find primary calendar ID
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
            .ok_or_else(|| IoError::new(ErrorKind::Other, "Primary calendar not found"))?;
        info!("Found primary calendar ID: {}", id);
        id
    };

    let now = Utc::now();
    let time_min = now;
    let time_max = now + Duration::days(14); // Look ahead 14 days

    info!(
        "Fetching busy slots for calendar '{}' between {} and {}",
        primary_id, time_min, time_max
    );
    // Pass the hub directly
    let busy: Vec<TimePeriod> =
        free_busy::get_busy_slots(hub, &primary_id, time_min, time_max).await?;
    info!("Found {} busy periods.", busy.len());

    info!("Calculating free windows...");
    let buffer = Duration::minutes(15);
    let raw    = free_busy::find_free_windows(&busy, time_min, time_max, buffer);
    info!("Found {} raw free windows.", raw.len());

    info!("Splitting windows at midnight...");
    let free_split = free_busy::split_at_midnight(&raw);
    info!("Found {} free windows after splitting.", free_split.len());

    Ok(free_split)
}
