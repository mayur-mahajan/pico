//! Wall-clock time. There is no RTC: the time is learnt from the `now` field of an API
//! response ([`sync`]) and advanced with the uptime counter; [`task`] publishes it once a second.

use core::sync::atomic::{AtomicU32, Ordering};

use embassy_time::{Duration, Instant, Ticker};
use twine::prelude::Latest;

/// Unix seconds at boot; 0 while the time is unknown.
static UNIX_AT_BOOT: AtomicU32 = AtomicU32::new(0);

/// Records the current Unix time in milliseconds (from an API response).
pub fn sync(unix_ms: u64) {
    let unix = (unix_ms / 1000) as u32;
    let uptime = Instant::now().as_secs() as u32;
    UNIX_AT_BOOT.store(unix.wrapping_sub(uptime), Ordering::Relaxed);
}

/// The current Unix time in seconds, if known.
fn unix_now() -> Option<u32> {
    match UNIX_AT_BOOT.load(Ordering::Relaxed) {
        0 => None,
        boot => Some(boot.wrapping_add(Instant::now().as_secs() as u32)),
    }
}

/// Publishes the local (Pacific) seconds since midnight to `local_secs` once a second, as soon as
/// the time is known (`None` until then).
#[embassy_executor::task]
pub async fn task(local_secs: &'static Latest<Option<u32>>) {
    let mut ticker = Ticker::every(Duration::from_secs(1));
    loop {
        if let Some(unix) = unix_now() {
            let local = (i64::from(unix) + i64::from(pacific_offset(unix))).rem_euclid(86_400);
            local_secs.set(Some(local as u32));
        }
        ticker.next().await;
    }
}

/// Offset of US Pacific time from UTC in seconds: −8 h, or −7 h during daylight saving
/// (second Sunday of March 02:00 local until first Sunday of November 02:00 local).
fn pacific_offset(unix: u32) -> i32 {
    const PST: i64 = -8 * 3600;
    let days = i64::from(unix).div_euclid(86_400);
    let (year, _, _) = civil_from_days(days);
    let dst_start = nth_sunday(year, 3, 2) * 86_400 + 2 * 3600 - PST;
    let dst_end = nth_sunday(year, 11, 1) * 86_400 + 2 * 3600 - (PST + 3600);
    let t = i64::from(unix);
    if (dst_start..dst_end).contains(&t) { -7 * 3600 } else { -8 * 3600 }
}

/// Day number (since 1970-01-01) of the `n`th Sunday of `month`.
fn nth_sunday(year: i64, month: i64, n: i64) -> i64 {
    let first = days_from_civil(year, month, 1);
    // 1970-01-01 was a Thursday; 0 = Sunday.
    let weekday = (first + 4).rem_euclid(7);
    first + (7 - weekday) % 7 + 7 * (n - 1)
}

// Howard Hinnant's civil-date algorithms.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}
