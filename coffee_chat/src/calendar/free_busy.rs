// src/calendar/free_busy.rs

use crate::app::TokioConnector; // your concrete connector type
use chrono::{DateTime, Duration, Local, TimeZone, Timelike, Utc};
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
        group_expansion_max:   None,
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
    window_end:   DateTime<Utc>,
    buffer:       Duration,
) -> Vec<(DateTime<Utc>, DateTime<Utc>)> {
    let mut windows = Vec::new();
    let mut periods = busy.to_vec();
    periods.sort_by_key(|p| p.start.clone());

    let mut cursor = window_start;

    for p in periods {
        let bs = p.start.unwrap_or(window_start);
        let be = p.end.unwrap_or(window_end);

        // 1) Compute the candidate end time by subtracting the buffer
        let candidate_end = bs - buffer;
        // 2) Clamp it so it never goes before window_start
        let free_end = if candidate_end > window_start {
            candidate_end
        } else {
            window_start
        };

        if free_end > cursor {
            windows.push((cursor, free_end));
        }

        // 3) Advance cursor past the busy period + buffer
        let next_cursor = be + buffer;
        cursor = next_cursor.max(cursor);

        if cursor >= window_end {
            break;
        }
    }

    // 4) Tail‐end gap
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
        let mut cur_date  = s_utc.with_timezone(&Local).date_naive();
        let end_date      = e_utc.with_timezone(&Local).date_naive();

        trace!("Splitting window: {:?}–{:?}", s_utc, e_utc);

        while cur_date < end_date {
            let nm_naive = cur_date
                .succ_opt().unwrap()
                .and_hms_opt(0, 0, 0).unwrap();

            let nm_utc = match Local.from_local_datetime(&nm_naive).single() {
                Some(dt) => dt.with_timezone(&Utc),
                None => {
                    match Local.from_local_datetime(&nm_naive).earliest() {
                        Some(dt) => dt.with_timezone(&Utc),
                        None => {
                            error!("Could not resolve midnight {:?}", nm_naive);
                            break;
                        }
                    }
                }
            };

            if nm_utc >= e_utc {
                break;
            }

            out.push((cur_start, nm_utc));
            trace!("  Added split: {:?}–{:?}", cur_start, nm_utc);

            cur_start = nm_utc;
            cur_date  = cur_start.with_timezone(&Local).date_naive();
        }

        if e_utc > cur_start {
            out.push((cur_start, e_utc));
            trace!("  Added final: {:?}–{:?}", cur_start, e_utc);
        }
    }
    debug!("After splitting at midnight: {:?}", out);
    out
}

/// Collapse contiguous same-day slots & format them into user-readable strings.
pub fn summarize_slots(
    slots: &[(DateTime<Utc>, DateTime<Utc>)],
    min_len: Duration,
) -> Vec<String> {
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
        let mut iter   = day_slots.into_iter();
        if let Some((mut cs, mut ce)) = iter.next() {
            for (s, e) in iter {
                if s == ce {
                    ce = e;
                } else {
                    if ce - cs >= min_len {
                        merged.push((cs, ce));
                    }
                    cs = s; ce = e;
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

            let wk    = s_loc.format("%A");
            let date  = s_loc.format("%b %-d");
            let start = fmt_time(s_loc);
            let end   = fmt_time(e_loc);

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
