// CyberManju OS — schedule expressions for the cron daemon (`cron` verb,
// `POST /api/cron`, the ProcessPanel Schedules tab). Two shapes:
//
//   * classic 5-field cron: `m h dom mon dow` — `* , - /` subsets,
//     aliases `@hourly/@daily/@weekly/@monthly/@yearly`;
//   * interval: `every 10m`, `every 2h`, `every 30s`, `every 1d`.
//
// Parsing is pure; `next_after` walks minute-by-minute with a hard search
// cap so an impossible expression (e.g. `0 0 30 2 *`) returns `None`
// instead of spinning forever. No threads, no I/O — the daemon in
// `scheduler.rs` owns that.

use chrono::{Datelike, Duration, Timelike, Utc};

/// A parsed schedule: either a cron expression or a fixed interval.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScheduleSpec {
    /// Five-field cron: minute, hour, day-of-month, month, day-of-week.
    Cron {
        m: Vec<u8>,
        h: Vec<u8>,
        dom: Vec<u8>,
        mon: Vec<u8>,
        dow: Vec<u8>,
    },
    /// `every <n><s|m|h|d>`.
    Every { n: u32, unit: EveryUnit },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EveryUnit {
    Seconds,
    Minutes,
    Hours,
    Days,
}

/// Search cap for `next_after`: 5 years of minutes ≈ 2.6 M iterations.
const MAX_SEARCH_STEPS: i64 = 5 * 366 * 24 * 60;

impl ScheduleSpec {
    /// Parse a user expression. Errors are `invalid: …` (AGENT-1 family).
    pub fn parse(raw: &str) -> Result<Self, String> {
        let s = raw.trim().to_lowercase();
        if s.is_empty() {
            return Err("invalid: empty schedule expression".to_string());
        }
        if let Some(spec) = parse_alias(&s) {
            return Ok(spec);
        }
        if let Some(rest) = s.strip_prefix("every ") {
            return parse_every(rest.trim());
        }
        parse_cron(&s)
    }

    /// Next fire time strictly after `after`, or `None` when nothing fires
    /// within the search cap (impossible date like Feb 30).
    pub fn next_after(&self, after: chrono::DateTime<Utc>) -> Option<chrono::DateTime<Utc>> {
        match self {
            ScheduleSpec::Every { n, unit } => {
                let secs: i64 = match unit {
                    EveryUnit::Seconds => i64::from(*n),
                    EveryUnit::Minutes => i64::from(*n) * 60,
                    EveryUnit::Hours => i64::from(*n) * 3600,
                    EveryUnit::Days => i64::from(*n) * 86_400,
                };
                if secs <= 0 {
                    return None;
                }
                Some(after + Duration::seconds(secs))
            }
            ScheduleSpec::Cron {
                m,
                h,
                dom,
                mon,
                dow,
            } => next_cron(m, h, dom, mon, dow, after),
        }
    }

    /// Human-readable form for the UI preview. A field covering its whole
    /// range renders as `*` (the browser twin does the same, so both sides
    /// show `0 * * * *` for `@hourly`, not `0 0,1,2,… 1,2,…`).
    pub fn describe(&self) -> String {
        match self {
            ScheduleSpec::Every { n, unit } => {
                let u = match unit {
                    EveryUnit::Seconds => "s",
                    EveryUnit::Minutes => "m",
                    EveryUnit::Hours => "h",
                    EveryUnit::Days => "d",
                };
                format!("every {n}{u}")
            }
            ScheduleSpec::Cron {
                m,
                h,
                dom,
                mon,
                dow,
            } => format!(
                "{} {} {} {} {}",
                fmt_field(m, 0, 59),
                fmt_field(h, 0, 23),
                fmt_field(dom, 1, 31),
                fmt_field(mon, 1, 12),
                fmt_field(dow, 0, 6)
            ),
        }
    }
}

fn fmt_field(v: &[u8], min: u8, max: u8) -> String {
    if v.is_empty() || v.len() == usize::from(max - min + 1) {
        "*".to_string()
    } else {
        v.iter()
            .map(|x| x.to_string())
            .collect::<Vec<_>>()
            .join(",")
    }
}

fn parse_alias(s: &str) -> Option<ScheduleSpec> {
    match s {
        "@hourly" => cron_from_parts("0 * * * *"),
        "@daily" | "@midnight" => cron_from_parts("0 0 * * *"),
        "@weekly" => cron_from_parts("0 0 * * 0"),
        "@monthly" => cron_from_parts("0 0 1 * *"),
        "@yearly" | "@annually" => cron_from_parts("0 0 1 1 *"),
        _ => None,
    }
}

fn cron_from_parts(s: &str) -> Option<ScheduleSpec> {
    ScheduleSpec::parse(s).ok()
}

fn parse_every(s: &str) -> Result<ScheduleSpec, String> {
    if s.is_empty() {
        return Err("invalid: `every` needs a value like `10m`".to_string());
    }
    let (num, unit) = s.split_at(s.len() - 1);
    let n: u32 = num
        .parse()
        .map_err(|_| format!("invalid: `every {s}` is not a number + unit"))?;
    if n == 0 {
        return Err("invalid: `every 0…` never fires".to_string());
    }
    let unit = match unit {
        "s" | "sec" | "secs" => EveryUnit::Seconds,
        "m" | "min" | "mins" => EveryUnit::Minutes,
        "h" | "hr" | "hrs" => EveryUnit::Hours,
        "d" | "day" | "days" => EveryUnit::Days,
        _ => {
            return Err(format!(
                "invalid: unknown unit `{unit}` in `every {s}` (use s|m|h|d)"
            ))
        }
    };
    Ok(ScheduleSpec::Every { n, unit })
}

fn parse_cron(s: &str) -> Result<ScheduleSpec, String> {
    let fields: Vec<&str> = s.split_whitespace().collect();
    if fields.len() != 5 {
        return Err(format!(
            "invalid: cron needs 5 fields (m h dom mon dow), got {} — `* * * * *` is every minute",
            fields.len()
        ));
    }
    Ok(ScheduleSpec::Cron {
        m: parse_field(fields[0], 0, 59, "minute")?,
        h: parse_field(fields[1], 0, 23, "hour")?,
        dom: parse_field(fields[2], 1, 31, "day-of-month")?,
        mon: parse_field(fields[3], 1, 12, "month")?,
        dow: parse_dow(fields[4])?,
    })
}

/// Day-of-week field: cron allows 7 as a second spelling of Sunday (0).
/// Normalize at parse time so `next_cron`'s weekday match (0 = Sunday)
/// never has to special-case two values for one day.
fn parse_dow(field: &str) -> Result<Vec<u8>, String> {
    let mut dow = parse_field(field, 0, 7, "day-of-week")?;
    for v in dow.iter_mut() {
        if *v == 7 {
            *v = 0;
        }
    }
    dow.sort_unstable();
    dow.dedup();
    Ok(dow)
}

/// Parse one cron field: `*`, `5`, `1-5`, `*/15`, `1-30/5`, comma lists.
/// Ranges expand to explicit value lists so `next_after` is a set lookup.
fn parse_field(field: &str, min: u8, max: u8, label: &str) -> Result<Vec<u8>, String> {
    let mut out: Vec<u8> = Vec::new();
    for part in field.split(',') {
        let p = part.trim();
        if p.is_empty() {
            return Err(format!("invalid: empty {label} field"));
        }
        let (range, step) = match p.split_once('/') {
            Some((r, st)) => {
                let st: u8 = st
                    .parse()
                    .map_err(|_| format!("invalid: bad step `{st}` in {label}"))?;
                if st == 0 {
                    return Err(format!("invalid: step 0 in {label}"));
                }
                (r, Some(st))
            }
            None => (p, None),
        };
        let (lo, hi) = if range == "*" {
            (min, max)
        } else if let Some((a, b)) = range.split_once('-') {
            let a: u8 = a
                .trim()
                .parse()
                .map_err(|_| format!("invalid: bad range start in {label} `{field}`"))?;
            let b: u8 = b
                .trim()
                .parse()
                .map_err(|_| format!("invalid: bad range end in {label} `{field}`"))?;
            (a, b)
        } else {
            let v: u8 = range
                .parse()
                .map_err(|_| format!("invalid: bad {label} value `{range}`"))?;
            (v, v)
        };
        if lo < min || hi > max || lo > hi {
            return Err(format!(
                "invalid: {label} `{field}` out of range {min}-{max}"
            ));
        }
        let step = step.unwrap_or(1);
        let mut v = lo;
        while v <= hi {
            if !out.contains(&v) {
                out.push(v);
            }
            match v.checked_add(step) {
                Some(next) => v = next,
                None => break,
            }
        }
    }
    out.sort_unstable();
    Ok(out)
}

/// Minute-iteration `next_after` for cron. Day-of-week in cron is 0-7 with
/// both 0 and 7 = Sunday; we match both. Day-of-month and day-of-week
/// combine with OR when both are restricted (standard cron behaviour).
fn next_cron(
    m: &[u8],
    h: &[u8],
    dom: &[u8],
    mon: &[u8],
    dow: &[u8],
    after: chrono::DateTime<Utc>,
) -> Option<chrono::DateTime<Utc>> {
    let base = after
        .with_second(0)
        .and_then(|t| t.with_nanosecond(0))
        .unwrap_or(after)
        + Duration::minutes(1);
    // A field covering every value is treated as unrestricted (`*`).
    let dom_any = dom.len() == 31;
    let dow_any = dow.len() >= 7;
    let dow_hit = |w: u32| -> bool {
        let d = w as u8 % 7;
        dow.contains(&d)
    };
    let day_hit = |t: &chrono::DateTime<Utc>| -> bool {
        if dom_any && dow_any {
            return true;
        }
        if !dom_any && dow_any {
            return dom.contains(&(t.day() as u8));
        }
        if dom_any && !dow_any {
            return dow_hit(t.weekday().num_days_from_sunday());
        }
        // Both restricted: standard cron ORs them.
        dom.contains(&(t.day() as u8)) || dow_hit(t.weekday().num_days_from_sunday())
    };
    for step in 0..MAX_SEARCH_STEPS {
        let t = base + Duration::minutes(step);
        if !mon.contains(&(t.month() as u8)) {
            continue;
        }
        if !day_hit(&t) {
            continue;
        }
        if !h.contains(&(t.hour() as u8)) {
            continue;
        }
        if !m.contains(&(t.minute() as u8)) {
            continue;
        }
        return Some(t);
    }
    None
}

/// Convenience: first fire after now, or `None`.
pub fn next_after(raw: &str, after: chrono::DateTime<Utc>) -> Option<chrono::DateTime<Utc>> {
    ScheduleSpec::parse(raw).ok()?.next_after(after)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Datelike, TimeZone};

    fn at(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(y, mo, d, h, mi, 0).unwrap()
    }

    #[test]
    fn parses_every_minute() {
        let spec = ScheduleSpec::parse("* * * * *").expect("parse");
        let next = spec.next_after(at(2026, 1, 1, 12, 0)).expect("next");
        assert_eq!(next, at(2026, 1, 1, 12, 1));
    }

    #[test]
    fn parses_daily_two_am() {
        let spec = ScheduleSpec::parse("30 2 * * *").expect("parse");
        let next = spec.next_after(at(2026, 1, 1, 3, 0)).expect("next");
        assert_eq!(next, at(2026, 1, 2, 2, 30));
    }

    #[test]
    fn parses_step() {
        let spec = ScheduleSpec::parse("*/15 * * * *").expect("parse");
        let next = spec.next_after(at(2026, 1, 1, 12, 1)).expect("next");
        assert_eq!(next, at(2026, 1, 1, 12, 15));
    }

    #[test]
    fn parses_range_list() {
        let spec = ScheduleSpec::parse("0 9-17 * * 1-5").expect("parse");
        let next = spec.next_after(at(2026, 1, 3, 12, 0)).expect("next"); // Sat
                                                                          // Next weekday (Mon) 09:00 — Jan 5 2026 is a Monday.
        assert_eq!(next, at(2026, 1, 5, 9, 0));
        assert_eq!(next.weekday().num_days_from_sunday(), 1);
    }

    #[test]
    fn alias_daily() {
        let spec = ScheduleSpec::parse("@daily").expect("parse");
        let next = spec.next_after(at(2026, 1, 1, 6, 0)).expect("next");
        assert_eq!(next, at(2026, 1, 2, 0, 0));
    }

    #[test]
    fn every_interval() {
        let spec = ScheduleSpec::parse("every 10m").expect("parse");
        let next = spec.next_after(at(2026, 1, 1, 12, 0)).expect("next");
        assert_eq!(next, at(2026, 1, 1, 12, 10));
        let spec = ScheduleSpec::parse("every 2h").expect("parse");
        let next = spec.next_after(at(2026, 1, 1, 12, 0)).expect("next");
        assert_eq!(next, at(2026, 1, 1, 14, 0));
    }

    #[test]
    fn impossible_date_is_none() {
        let spec = ScheduleSpec::parse("0 0 30 2 *").expect("parse");
        assert!(spec.next_after(at(2026, 1, 1, 0, 0)).is_none());
    }

    #[test]
    fn rejects_bad_expressions() {
        assert!(ScheduleSpec::parse("").is_err());
        assert!(ScheduleSpec::parse("* * *").is_err());
        assert!(ScheduleSpec::parse("61 * * * *").is_err());
        assert!(ScheduleSpec::parse("every 0m").is_err());
        assert!(ScheduleSpec::parse("every 5x").is_err());
    }

    #[test]
    fn sunday_seven_matches_zero() {
        // dow 7 = Sunday; must fire on a Sunday.
        let spec = ScheduleSpec::parse("0 0 * * 7").expect("parse");
        let next = spec.next_after(at(2026, 1, 1, 0, 0)).expect("next");
        // Jan 4 2026 is a Sunday.
        assert_eq!(next, at(2026, 1, 4, 0, 0));
    }

    #[test]
    fn describe_round_trips() {
        let spec = ScheduleSpec::parse("every 10m").expect("parse");
        assert_eq!(spec.describe(), "every 10m");
    }

    #[test]
    fn describe_renders_full_range_fields_as_star() {
        let spec = ScheduleSpec::parse("@hourly").expect("parse");
        assert_eq!(spec.describe(), "0 * * * *");
        let spec = ScheduleSpec::parse("30 2 * * *").expect("parse");
        assert_eq!(spec.describe(), "30 2 * * *");
    }

    #[test]
    fn dom_and_dow_or_when_both_restricted() {
        // Jan 4 2026 is a Sunday; the 1st has passed at 01:00.
        let next = next_after("0 0 1 * 0", at(2026, 1, 4, 1, 0)).expect("next");
        assert_eq!(next, at(2026, 1, 11, 0, 0));
    }
}
