// src/calendar/free_busy.rs

use crate::app::TokioConnector; // your concrete connector type
use chrono::{DateTime, Duration, Local, NaiveTime, TimeZone, Timelike, Utc};
use google_calendar3::{
    api::{FreeBusyRequest, FreeBusyRequestItem, TimePeriod},
    CalendarHub,
};
use log::{debug, error, trace};
use std::collections::BTreeMap;
use std::error::Error;

/// Fetch busy periods from the FreeBusy API for a calendar.
pub async fn get_busy_slots(
    hub: &CalendarHub<TokioConnector>,
    calendar_id: &str,
    time_min: DateTime<Utc>,
    time_max: DateTime<Utc>,
) -> Result<Vec<TimePeriod>, Box<dyn Error>> {
    let req = FreeBusyRequest {
        time_min: Some(time_min),
        time_max: Some(time_max),
        time_zone: Some("UTC".to_string()),
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

/// Compute full free windows with a buffer **before** and **after** each busy slot.
pub fn find_free_windows(
    busy: &[TimePeriod],
    window_start: DateTime<Utc>,
    window_end: DateTime<Utc>,
    buffer: Duration, // Use the passed buffer duration
) -> Vec<(DateTime<Utc>, DateTime<Utc>)> {
    let mut windows = Vec::new();
    let mut periods = busy.to_vec();
    // Ensure sorting by start time
    periods.sort_by_key(|p| p.start);

    let mut cursor = window_start;

    for p in periods {
        // Gracefully handle missing start/end, though FreeBusy usually provides them
        let busy_start = match p.start {
            Some(dt) => dt,
            None => {
                debug!("Skipping busy period with no start time");
                continue; // Skip if no start time
            }
        };
        let busy_end = match p.end {
            Some(dt) => dt,
            None => {
                debug!("Skipping busy period with no end time");
                continue; // Skip if no end time
            }
        };

        // Calculate the start of the 'blocked' period (including buffer)
        let blocked_start = busy_start - buffer;
        // Calculate the end of the 'blocked' period (including buffer)
        let blocked_end = busy_end + buffer;

        // If there's a gap between the current cursor and the start of the blocked period
        if blocked_start > cursor {
            windows.push((cursor, blocked_start));
        }

        // Advance the cursor to the end of the blocked period, ensuring it only moves forward
        cursor = cursor.max(blocked_end);

        // If cursor is already past the overall window, we can stop
        if cursor >= window_end {
            break;
        }
    }

    // If there's a gap between the last cursor position and the overall window end
    if window_end > cursor {
        windows.push((cursor, window_end));
    }

    debug!("Raw free windows (with buffer): {:?}", windows);
    windows
}

/// Split windows at local midnight so each window stays on one date.
pub fn split_at_midnight(
    windows: &[(DateTime<Utc>, DateTime<Utc>)],
) -> Vec<(DateTime<Utc>, DateTime<Utc>)> {
    let mut out = Vec::new();
    for &(s_utc, e_utc) in windows {
        let mut cur_start = s_utc;
        let mut cur_date = s_utc.with_timezone(&Local).date_naive();
        let end_date = e_utc.with_timezone(&Local).date_naive();

        trace!("Splitting window: {:?}–{:?}", s_utc, e_utc);

        while cur_date < end_date {
            let nm_naive = cur_date.succ_opt().unwrap().and_hms_opt(0, 0, 0).unwrap();

            let nm_utc = match Local.from_local_datetime(&nm_naive).single() {
                Some(dt) => dt.with_timezone(&Utc),
                None => match Local.from_local_datetime(&nm_naive).earliest() {
                    Some(dt) => dt.with_timezone(&Utc),
                    None => {
                        error!("Could not resolve midnight {:?}", nm_naive);
                        break;
                    }
                },
            };

            if nm_utc >= e_utc {
                break;
            }

            out.push((cur_start, nm_utc));
            trace!("  Added split: {:?}–{:?}", cur_start, nm_utc);

            cur_start = nm_utc;
            cur_date = cur_start.with_timezone(&Local).date_naive();
        }

        if e_utc > cur_start {
            out.push((cur_start, e_utc));
            trace!("  Added final: {:?}–{:?}", cur_start, e_utc);
        }
    }
    debug!("After splitting at midnight: {:?}", out);
    out
}

pub fn filter_slots_by_time_of_day(
    slots: &[(DateTime<Utc>, DateTime<Utc>)],
    start_hour: u32,
    end_hour: u32,
) -> Vec<(DateTime<Utc>, DateTime<Utc>)> {
    let mut filtered = Vec::new();

    // Ensure valid hour range (basic check)
    if start_hour >= end_hour || start_hour > 23 || end_hour > 23 {
        error!("Invalid start/end hour range: {}-{}", start_hour, end_hour);
        return slots.to_vec(); // Return original if range is invalid
    }

    let start_time = NaiveTime::from_hms_opt(start_hour, 0, 0).unwrap();
    let end_time = NaiveTime::from_hms_opt(end_hour, 0, 0).unwrap(); // End is exclusive usually, but let's treat HH:00 as inclusive start of hour

    for &(slot_start_utc, slot_end_utc) in slots {
        // Convert slot times to local time
        let slot_start_local = slot_start_utc.with_timezone(&Local);
        let slot_end_local = slot_end_utc.with_timezone(&Local);

        // Get the date part for comparison
        let slot_date = slot_start_local.date_naive();

        // Define the valid time range for this specific date in Local time
        let valid_start_local = Local
            .from_local_datetime(&slot_date.and_time(start_time))
            .single() // Handle potential DST ambiguity simply
            .unwrap_or_else(|| slot_start_local); // Fallback
        let valid_end_local = Local
            .from_local_datetime(&slot_date.and_time(end_time))
            .single()
            .unwrap_or_else(|| slot_end_local); // Fallback

        // If the valid range spans midnight due to DST or timezone shifts, adjust (simple approach)
        // This part might need refinement for complex timezone edge cases near midnight
        let valid_end_local = if valid_end_local <= valid_start_local {
            valid_end_local + Duration::days(1)
        } else {
            valid_end_local
        };

        // Calculate the intersection of the slot and the valid time range for that day
        let effective_start_local = slot_start_local.max(valid_start_local);
        let effective_end_local = slot_end_local.min(valid_end_local);

        // If there is a valid intersection (start < end)
        if effective_start_local < effective_end_local {
            // Convert back to UTC and add to filtered list
            filtered.push((
                effective_start_local.with_timezone(&Utc),
                effective_end_local.with_timezone(&Utc),
            ));
            trace!(
                "Kept/Trimmed slot: {:?} - {:?}",
                effective_start_local,
                effective_end_local
            );
        } else {
            trace!(
                "Discarded slot: {:?} - {:?}",
                slot_start_local,
                slot_end_local
            );
        }

        // Note: This simple approach assumes slots don't span across the valid/invalid boundary *multiple* times
        // within a single original slot (e.g., valid 9-12, slot is 8-13 -> keeps 9-12).
        // Handling slots that start before start_hour AND end after end_hour on the *same day*
        // correctly creates a single segment. A slot spanning midnight AND the filter times
        // requires careful handling based on the `split_at_midnight` output.
    }

    debug!(
        "Filtered slots by time ({} to {}): {:?}",
        start_hour, end_hour, filtered
    );
    filtered
}

/// Collapse contiguous same-day slots & format them into user-readable strings.
pub fn summarize_slots(slots: &[(DateTime<Utc>, DateTime<Utc>)], min_len: Duration) -> Vec<String> {
    let mut by_day: BTreeMap<_, Vec<_>> = BTreeMap::new();
    for &(s, e) in slots {
        // group by local date
        let d = s.with_timezone(&Local).date_naive();
        by_day.entry(d).or_default().push((s, e));
    }
    debug!("Grouped slots for {} days", by_day.len());

    let mut out = Vec::new();
    for (day, mut day_slots) in by_day {
        day_slots.sort_by_key(|(s, _)| *s);

        // merge contiguous & filter
        let mut merged = Vec::new();
        let mut iter = day_slots.into_iter();
        if let Some((mut cs, mut ce)) = iter.next() {
            for (s, e) in iter {
                if s == ce {
                    ce = e;
                } else {
                    if ce - cs >= min_len {
                        merged.push((cs, ce));
                    }
                    cs = s;
                    ce = e;
                }
            }
            if ce - cs >= min_len {
                merged.push((cs, ce));
            }
        }
        trace!("Day {:?} merged: {:?}", day, merged);

        // format each window
        for (s_utc, e_utc) in merged {
            let s_loc = s_utc.with_timezone(&Local);
            let e_loc = e_utc.with_timezone(&Local);

            fn fmt_time(dt: DateTime<Local>) -> String {
                if dt.minute() == 0 {
                    dt.format("%-I%P").to_string()
                } else {
                    dt.format("%-I:%M%P").to_string()
                }
            }

            let wk = s_loc.format("%A");
            let date = s_loc.format("%b %-d");
            let start = fmt_time(s_loc);
            let end = fmt_time(e_loc);

            if s_loc.date_naive() != e_loc.date_naive() {
                out.push(format!(
                    "{} {}: {}–{} {}: {}",
                    wk,
                    date,
                    start,
                    e_loc.format("%A"),
                    e_loc.format("%b %-d"),
                    end
                ));
            } else {
                out.push(format!("{} {}: {}–{}", wk, date, start, end));
            }
        }
    }
    debug!("Summarized slots ({}): {:?}", out.len(), out);
    out
}
