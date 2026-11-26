//! Wall-clock time in the forms the app data area writes: the log's UTC
//! timestamps and the stamp in a damaged file's new name. Written by hand so
//! the core needs no date crate.

/// Milliseconds since the Unix epoch, now. 0 if the clock is before 1970.
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// A UTC time broken into calendar parts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Utc {
    /// The year, for example 2026.
    pub year: i64,
    /// 1 to 12.
    pub month: u32,
    /// 1 to 31.
    pub day: u32,
    /// 0 to 23.
    pub hour: u32,
    /// 0 to 59.
    pub minute: u32,
    /// 0 to 59.
    pub second: u32,
    /// 0 to 999.
    pub milli: u32,
}

impl Utc {
    /// The UTC calendar time `ms` milliseconds after the Unix epoch.
    pub fn from_ms(ms: i64) -> Self {
        let days = ms.div_euclid(86_400_000);
        let in_day = ms.rem_euclid(86_400_000);
        let (year, month, day) = civil_from_days(days);
        // `in_day` is below 86,400,000, so every part fits a u32.
        let part = |v: i64| u32::try_from(v).unwrap_or(0);
        Self {
            year,
            month,
            day,
            hour: part(in_day / 3_600_000),
            minute: part(in_day / 60_000 % 60),
            second: part(in_day / 1000 % 60),
            milli: part(in_day % 1000),
        }
    }

    /// ISO 8601 with milliseconds and a `Z`: `2026-11-26T09:05:03.007Z`.
    pub fn iso(&self) -> String {
        format!(
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
            self.year, self.month, self.day, self.hour, self.minute, self.second, self.milli
        )
    }

    /// The stamp in a set-aside file's name: `20261126-090503`, the same
    /// form album safety uses for `owlbum.damaged-<stamp>.sqlite`.
    pub fn stamp(&self) -> String {
        format!(
            "{:04}{:02}{:02}-{:02}{:02}{:02}",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }
}

/// Year, month and day of the day `days` after 1970-01-01 in the proleptic
/// Gregorian calendar (Howard Hinnant's `civil_from_days`).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (
        year,
        u32::try_from(month).unwrap_or(1),
        u32::try_from(day).unwrap_or(1),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_epoch_is_the_first_of_january_1970() {
        assert_eq!(Utc::from_ms(0).iso(), "1970-01-01T00:00:00.000Z");
    }

    #[test]
    fn a_known_instant_formats_with_milliseconds() {
        // 2026-11-26T09:05:03.007Z
        let ms = 1_795_683_903_007;
        assert_eq!(Utc::from_ms(ms).iso(), "2026-11-26T09:05:03.007Z");
        assert_eq!(Utc::from_ms(ms).stamp(), "20261126-090503");
    }

    #[test]
    fn leap_days_and_year_ends_are_right() {
        // 2024-02-29T23:59:59.999Z and the next millisecond.
        let ms = 1_709_251_199_999;
        assert_eq!(Utc::from_ms(ms).iso(), "2024-02-29T23:59:59.999Z");
        assert_eq!(Utc::from_ms(ms + 1).iso(), "2024-03-01T00:00:00.000Z");
        // 2000-12-31T12:00:00Z, the end of a leap century year.
        assert_eq!(
            Utc::from_ms(978_264_000_000).iso(),
            "2000-12-31T12:00:00.000Z"
        );
    }

    #[test]
    fn times_before_1970_count_backwards() {
        assert_eq!(Utc::from_ms(-1).iso(), "1969-12-31T23:59:59.999Z");
    }

    #[test]
    fn now_is_after_2025() {
        assert!(now_ms() > 1_735_689_600_000);
    }
}
