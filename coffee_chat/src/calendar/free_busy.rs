// src/calendar/free_busy.rs
use crate::app::TokioConnector; // Import the type alias from app.rs
use chrono::{
    DateTime,
    Duration, // Removed unused Datelike, NaiveDateTime
    Local,
    TimeZone,
    Timelike,
    Utc,
};
use google_calendar3::{
    api::{FreeBusyRequest, FreeBusyRequestItem, TimePeriod},
    CalendarHub, // Removed Connector import
};
use log::{debug, error, trace}; // <-- Add this
use std::collections::BTreeMap;
use std::error::Error;

// Change the function signature to use the concrete Hub type
pub async fn get_busy_slots(
    hub: &CalendarHub<TokioConnector>, // Use concrete type
    calendar_id: &str,
    time_min: DateTime<Utc>,
    time_max: DateTime<Utc>,
) -> Result<Vec<TimePeriod>, Box<dyn Error>>
// Remove the 'where C: ...' clause
{
    let req = FreeBusyRequest {
        time_min: Some(time_min),
        time_max: Some(time_max),
        time_zone: Some("UTC".to_string()), // Use UTC for consistency
        items: Some(vec![FreeBusyRequestItem {
            id: Some(calendar_id.to_string()),
        }]),
        calendar_expansion_max: None,
        group_expansion_max: None,
    };

    trace!("Sending FreeBusy query: {:?}", req);
    let (_, resp) = hub.freebusy().query(req).doit().await?;
    trace!("Received FreeBusy response");

    let busy = resp
        .calendars
        .and_then(|m| m.get(calendar_id).cloned())
        .and_then(|c| c.busy)
        .unwrap_or_default();

    debug!("Busy periods for {}: {:?}", calendar_id, busy);
    Ok(busy)
}

// ... (summarize_slots remains the same) ...
pub fn summarize_slots(slots: &[(DateTime<Utc>, DateTime<Utc>)], min_len: Duration) -> Vec<String> {
    // 1) bucket by calendar date (local time)
    let mut by_day: BTreeMap<_, Vec<_>> = BTreeMap::new();
    for &(start, end) in slots {
        // Use local date for grouping
        let local_start_date = start.with_timezone(&Local).date_naive();
        by_day
            .entry(local_start_date)
            .or_default()
            .push((start, end));
    }
    debug!("Grouped slots by day (local): {} days", by_day.len());

    let mut out = Vec::new();

    for (day, mut day_slots) in by_day {
        day_slots.sort_by_key(|(s, _)| *s); // Sort by UTC start time

        // 2) merge contiguous slots
        let mut merged = Vec::new();
        let mut iter = day_slots.into_iter();
        if let Some((mut cs, mut ce)) = iter.next() {
            for (s, e) in iter {
                // Check for contiguousness (allow for small gaps if needed, but exact match is safer)
                if s == ce {
                    ce = e; // Extend the current merged slot
                } else {
                    // Finish the previous merged slot and start a new one
                    if ce - cs >= min_len {
                        // Apply min_len filter *before* adding
                        merged.push((cs, ce));
                    }
                    cs = s;
                    ce = e;
                }
            }
            // Add the last merged slot
            if ce - cs >= min_len {
                merged.push((cs, ce));
            }
        }
        trace!("Merged slots for day {:?}: {:?}", day, merged);

        // 3) format (already filtered by min_len during merge)
        for (s_utc, e_utc) in merged {
            let s_loc = s_utc.with_timezone(&Local);
            let e_loc = e_utc.with_timezone(&Local);

            // Simplified time formatting function
            fn fmt_time(dt: DateTime<Local>) -> String {
                if dt.minute() == 0 {
                    dt.format("%-I%P").to_string()
                } else {
                    dt.format("%-I:%M%P").to_string()
                }
            }

            let weekday = s_loc.format("%A"); // Full weekday name
            let mdy = s_loc.format("%b %-d"); // e.g., "May 5"
            let start_t = fmt_time(s_loc);
            let end_t = fmt_time(e_loc);

            // Handle slots crossing midnight locally - show date range if needed
            if s_loc.date_naive() != e_loc.date_naive() {
                out.push(format!(
                    "{} {} {} – {} {} {}",
                    weekday,
                    mdy,
                    start_t,
                    e_loc.format("%A"),
                    e_loc.format("%b %-d"),
                    end_t
                ));
            } else {
                out.push(format!("{} {}: {}–{}", weekday, mdy, start_t, end_t));
            }
        }
    }
    debug!("Summarized slots ({} total): {:?}", out.len(), out);
    out
}

// ... (find_free_windows remains the same) ...
pub fn find_free_windows(
    busy: &[TimePeriod],
    window_start: DateTime<Utc>,
    window_end: DateTime<Utc>,
) -> Vec<(DateTime<Utc>, DateTime<Utc>)> {
    let mut windows = Vec::new();
    let mut periods = busy.to_vec();
    // Sort busy periods by start time
    periods.sort_by_key(|p| p.start);

    let mut cursor = window_start;

    for p in periods {
        // API might return None for start/end, default reasonably
        let busy_start = p.start.unwrap_or(window_start);
        let busy_end = p.end.unwrap_or(window_end);

        // Ignore busy periods entirely before our cursor
        if busy_end <= cursor {
            continue;
        }

        // If there's a gap between cursor and the start of this busy period
        if busy_start > cursor {
            windows.push((cursor, busy_start));
        }

        // Move cursor to the end of the current busy period (avoiding going backwards)
        cursor = cursor.max(busy_end);

        // Stop if cursor goes beyond the desired window
        if cursor >= window_end {
            break;
        }
    }

    // Check for a final gap between the last busy period and the window end
    if window_end > cursor {
        windows.push((cursor, window_end));
    }

    debug!("Raw free windows: {:?}", windows);
    windows
}

// ... (split_at_midnight remains the same) ...
pub fn split_at_midnight(
    windows: &[(DateTime<Utc>, DateTime<Utc>)],
) -> Vec<(DateTime<Utc>, DateTime<Utc>)> {
    let mut out = Vec::new();
    for &(s_utc, e_utc) in windows {
        let s_local = s_utc.with_timezone(&Local);
        let e_local = e_utc.with_timezone(&Local);

        let mut current_start_utc = s_utc;
        let mut current_date_local = s_local.date_naive();
        let end_date_local = e_local.date_naive();

        trace!("Splitting window: {:?} to {:?}", s_utc, e_utc);

        // While the current segment starts on a day before the end date
        while current_date_local < end_date_local {
            // Calculate the *next* local midnight
            let next_midnight_naive = current_date_local
                .succ_opt()
                .expect("Date should have successor") // Next day
                .and_hms_opt(0, 0, 0)
                .expect("00:00:00 is valid"); // At 00:00:00

            // Convert local midnight to UTC
            // Handle potential ambiguity/non-existence during DST changes
            let next_midnight_utc = match Local.from_local_datetime(&next_midnight_naive).single() {
                Some(dt) => dt.with_timezone(&Utc),
                None => {
                    // Handle ambiguity if necessary, e.g., pick the earlier one
                    match Local.from_local_datetime(&next_midnight_naive).earliest() {
                        Some(dt) => dt.with_timezone(&Utc),
                        None => {
                            // Use log::error here now
                            error!(
                                "Could not resolve local midnight to UTC: {}",
                                next_midnight_naive
                            );
                            // Skip this split point or handle error appropriately
                            break; // Exit the loop for this window
                        }
                    }
                }
            };

            // Ensure we don't create a split point past the original end time
            if next_midnight_utc >= e_utc {
                break;
            }

            // Add the segment from the current start to the midnight split
            if next_midnight_utc > current_start_utc {
                // Avoid zero-duration slots
                out.push((current_start_utc, next_midnight_utc));
                trace!(
                    "  Added split: {:?} to {:?}",
                    current_start_utc,
                    next_midnight_utc
                );
            }

            // Move the start to the midnight for the next iteration
            current_start_utc = next_midnight_utc;
            current_date_local = current_start_utc.with_timezone(&Local).date_naive();
        }

        // Add the final segment from the last split point (or original start) to the original end
        if e_utc > current_start_utc {
            // Avoid zero-duration slots
            out.push((current_start_utc, e_utc));
            trace!(
                "  Added final segment: {:?} to {:?}",
                current_start_utc,
                e_utc
            );
        }
    }
    debug!("Windows after splitting at midnight: {:?}", out);
    out
}
