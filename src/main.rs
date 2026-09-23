use chrono::{Datelike, NaiveDate, Utc, Weekday};
use clap::Parser;
use regex::Regex;
use serde::Deserialize;
use std::fmt;
use std::fs::{create_dir_all, read_to_string, write};
use std::io;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::LazyLock;

/// CLI tool to determine if today's date falls within a configured time period.
#[derive(Parser)]
#[command(name = "TimePeriodChecker")]
#[command(author = "Frederick Price")]
#[command(version = "1.0")]
#[command(about = "Checks what time period a date falls into based on YAML configs", long_about = None)]
struct Cli {
    /// Pass a specific date to evaluate (format: YYYY-MM-DD)
    #[arg(short, long)]
    date: Option<NaiveDate>,

    /// Force-regenerate the user config file
    #[arg(long)]
    init: bool,
}

const DEFAULT_CONFIG_YAML: &str = r"TimePeriods:
  - MothersDay:
      Date: The second Sunday of May
      DaysBefore: 3
      DaysAfter: 1
      Comment: Mother's Day
  - FathersDay:
      Date: The third Sunday of June
      DaysBefore: 3
      DaysAfter: 1
      Comment: Father's Day
  - EasterPeriod:
      Date: Easter
      DaysBefore: 5
      DaysAfter: 2
  - Thanksgiving:
      Date: Thanksgiving
      DaysBefore: 3
      DaysAfter: 2
  - LaborWeek:
      Date: LaborDay
      DaysBefore: 1
      DaysAfter: 2
";

fn main() {
    let cli = Cli::parse();

    if cli.init {
        if let Some(user_path) = get_user_config_path() {
            if let Err(e) = write_user_config(&user_path, true) {
                eprintln!("Error: {e}");
            }
        } else {
            eprintln!("Error: Could not determine user config path");
        }
        return;
    }

    let current_date = cli.date.unwrap_or_else(|| Utc::now().date_naive());

    let system_path = "/etc/NameTimePeriod/time_periods.yaml";
    let user_path = get_user_config_path();

    // Only create user config if both system and user configs don't exist
    if let Some(ref path) = user_path
        && !Path::new(system_path).exists() && !path.exists()
        && let Err(e) = write_user_config(path, false)
    {
        eprintln!("Warning: {e}");
    }

    let merged: Vec<_> = user_path
        .as_ref()
        .map(|path| load_yaml_file(path))
        .unwrap_or_default()
        .into_iter()
        .chain(load_yaml_file(Path::new(system_path)))
        .collect();

    println!("{}", get_current_period(&merged, current_date));
}

fn get_user_config_path() -> Option<PathBuf> {
    dirs::home_dir().map(|p| p.join(".config/NameTimePeriod/time_periods.yaml"))
}

#[derive(Debug)]
enum ConfigError {
    IoError(io::Error),
    DirectoryCreation(io::Error),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IoError(e) => write!(f, "IO error: {e}"),
            Self::DirectoryCreation(e) => write!(f, "Failed to create directory: {e}"),
        }
    }
}

impl From<io::Error> for ConfigError {
    fn from(error: io::Error) -> Self {
        Self::IoError(error)
    }
}

fn write_user_config(path: &Path, force: bool) -> Result<(), ConfigError> {
    if path.exists() && !force {
        return Ok(());
    }

    if let Some(parent) = path.parent()
        && !parent.exists()
    {
        create_dir_all(parent).map_err(ConfigError::DirectoryCreation)?;
    }

    write(path, DEFAULT_CONFIG_YAML)?;
    println!(
        "Default user config {}written to {}",
        if force { "(force) " } else { "" },
        path.display()
    );
    Ok(())
}

fn load_yaml_file(path: &Path) -> Vec<(String, TimePeriod)> {
    load_yaml_file_inner(path).unwrap_or_default()
}

fn load_yaml_file_inner(path: &Path) -> Option<Vec<(String, TimePeriod)>> {
    let content = read_to_string(path).ok()?;
    let doc: serde_yaml::Value = serde_yaml::from_str(&content).ok()?;
    let arr = doc.get("TimePeriods")?.as_sequence()?;

    Some(
        arr.iter()
            .filter_map(|entry| {
                let map = entry.as_mapping()?;
                map.iter().find_map(|(k, v)| {
                    let name = k.as_str()?.to_string();
                    let tp = serde_yaml::from_value::<TimePeriod>(v.clone()).ok()?;
                    Some((name, tp))
                })
            })
            .collect(),
    )
}

#[derive(Debug, Clone, Deserialize)]
struct TimePeriod {
    #[serde(rename = "Date")]
    date: String,
    #[serde(rename = "DaysBefore")]
    days_before: i64,
    #[serde(rename = "DaysAfter")]
    days_after: i64,
}

fn get_current_period(periods: &[(String, TimePeriod)], current_date: NaiveDate) -> String {
    let matches: Vec<&str> = periods
        .iter()
        .filter_map(|(name, period)| {
            let base_date = parse_flexible_date(&period.date, current_date.year())?;
            let start = base_date - chrono::Duration::days(period.days_before);
            let end = base_date + chrono::Duration::days(period.days_after);
            (start..=end)
                .contains(&current_date)
                .then_some(name.as_str())
        })
        .collect();

    if matches.is_empty() {
        "Default".to_string()
    } else {
        matches.join(" ")
    }
}

fn parse_flexible_date(date_str: &str, year: i32) -> Option<NaiveDate> {
    let lower = date_str.trim().to_lowercase();

    match lower.as_str() {
        "easter" => Some(calculate_easter(year)),
        "thanksgiving" => nth_weekday_of_month(year, 11, Weekday::Thu, 4),
        "laborday" => nth_weekday_of_month(year, 9, Weekday::Mon, 1),
        "memorialday" => last_weekday_of_month(year, 5, Weekday::Mon),
        "mlkday" => nth_weekday_of_month(year, 1, Weekday::Mon, 3),
        _ => parse_relative_date(date_str, year).or_else(|| {
            NaiveDate::parse_from_str(&format!("{date_str} {year}"), "%B %d %Y").ok()
        }),
    }
}

static RELATIVE_DATE_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)the\s+(\w+)\s+(\w+)\s+of\s+(\w+)").expect("Invalid regex pattern")
});

fn parse_relative_date(date_str: &str, year: i32) -> Option<NaiveDate> {
    let cap = RELATIVE_DATE_REGEX.captures(date_str)?;

    let nth: u32 = match cap[1].to_lowercase().as_str() {
        "first" => 1,
        "second" => 2,
        "third" => 3,
        "fourth" => 4,
        "fifth" => 5,
        _ => return None,
    };

    let weekday = match cap[2].to_lowercase().as_str() {
        "monday" => Weekday::Mon,
        "tuesday" => Weekday::Tue,
        "wednesday" => Weekday::Wed,
        "thursday" => Weekday::Thu,
        "friday" => Weekday::Fri,
        "saturday" => Weekday::Sat,
        "sunday" => Weekday::Sun,
        _ => return None,
    };

    let month = chrono::Month::from_str(&cap[3]).ok()?.number_from_month();
    nth_weekday_of_month(year, month, weekday, nth)
}

fn nth_weekday_of_month(year: i32, month: u32, weekday: Weekday, nth: u32) -> Option<NaiveDate> {
    (1..=31)
        .filter_map(|day| NaiveDate::from_ymd_opt(year, month, day))
        .filter(|date| date.weekday() == weekday)
        .nth((nth - 1) as usize)
}

fn last_weekday_of_month(year: i32, month: u32, weekday: Weekday) -> Option<NaiveDate> {
    (1..=31)
        .rev()
        .filter_map(|day| NaiveDate::from_ymd_opt(year, month, day))
        .find(|date| date.weekday() == weekday)
}

#[allow(clippy::many_single_char_names)]
const fn calculate_easter(year: i32) -> NaiveDate {
    let a = year % 19;
    let b = year / 100;
    let c = year % 100;
    let d = b / 4;
    let e = b % 4;
    let f = (b + 8) / 25;
    let g = (b - f + 1) / 3;
    let h = (19 * a + b - d - g + 15) % 30;
    let i = c / 4;
    let k = c % 4;
    let l = (32 + 2 * e + 2 * i - h - k) % 7;
    let m = (a + 11 * h + 22 * l) / 451;
    let month = (h + l - 7 * m + 114) / 31;
    let day = ((h + l - 7 * m + 114) % 31) + 1;
    match NaiveDate::from_ymd_opt(year, month.cast_unsigned(), day.cast_unsigned()) {
        Some(d) => d,
        None => panic!("Invalid Easter date calculation"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{NaiveDate, Weekday};

    fn period(name: &str, date_str: &str, days_before: i64, days_after: i64) -> (String, TimePeriod) {
        (
            name.to_string(),
            TimePeriod {
                date: date_str.to_string(),
                days_before,
                days_after,
            },
        )
    }

    // ── calculate_easter ──────────────────────────────────────────────────────

    #[test]
    fn test_easter_2020() {
        assert_eq!(calculate_easter(2020), NaiveDate::from_ymd_opt(2020, 4, 12).unwrap());
    }

    #[test]
    fn test_easter_2021() {
        assert_eq!(calculate_easter(2021), NaiveDate::from_ymd_opt(2021, 4, 4).unwrap());
    }

    #[test]
    fn test_easter_2022() {
        assert_eq!(calculate_easter(2022), NaiveDate::from_ymd_opt(2022, 4, 17).unwrap());
    }

    #[test]
    fn test_easter_2023() {
        assert_eq!(calculate_easter(2023), NaiveDate::from_ymd_opt(2023, 4, 9).unwrap());
    }

    #[test]
    fn test_easter_2024() {
        assert_eq!(calculate_easter(2024), NaiveDate::from_ymd_opt(2024, 3, 31).unwrap());
    }

    #[test]
    fn test_easter_2025() {
        assert_eq!(calculate_easter(2025), NaiveDate::from_ymd_opt(2025, 4, 20).unwrap());
    }

    #[test]
    fn test_easter_2026() {
        assert_eq!(calculate_easter(2026), NaiveDate::from_ymd_opt(2026, 4, 5).unwrap());
    }

    // ── nth_weekday_of_month ──────────────────────────────────────────────────

    #[test]
    fn test_nth_weekday_second_sunday_may_2025() {
        // Mother's Day
        let date = nth_weekday_of_month(2025, 5, Weekday::Sun, 2);
        assert_eq!(date, Some(NaiveDate::from_ymd_opt(2025, 5, 11).unwrap()));
    }

    #[test]
    fn test_nth_weekday_third_sunday_june_2025() {
        // Father's Day
        let date = nth_weekday_of_month(2025, 6, Weekday::Sun, 3);
        assert_eq!(date, Some(NaiveDate::from_ymd_opt(2025, 6, 15).unwrap()));
    }

    #[test]
    fn test_nth_weekday_fourth_thursday_november_2025() {
        // Thanksgiving
        let date = nth_weekday_of_month(2025, 11, Weekday::Thu, 4);
        assert_eq!(date, Some(NaiveDate::from_ymd_opt(2025, 11, 27).unwrap()));
    }

    #[test]
    fn test_nth_weekday_first_monday_september_2025() {
        // Labor Day — Sep 1 is a Monday in 2025
        let date = nth_weekday_of_month(2025, 9, Weekday::Mon, 1);
        assert_eq!(date, Some(NaiveDate::from_ymd_opt(2025, 9, 1).unwrap()));
    }

    #[test]
    fn test_nth_weekday_third_monday_january_2025() {
        // MLK Day
        let date = nth_weekday_of_month(2025, 1, Weekday::Mon, 3);
        assert_eq!(date, Some(NaiveDate::from_ymd_opt(2025, 1, 20).unwrap()));
    }

    #[test]
    fn test_nth_weekday_fifth_exists() {
        // Five Mondays in December 2025: 1, 8, 15, 22, 29
        let date = nth_weekday_of_month(2025, 12, Weekday::Mon, 5);
        assert_eq!(date, Some(NaiveDate::from_ymd_opt(2025, 12, 29).unwrap()));
    }

    #[test]
    fn test_nth_weekday_fifth_does_not_exist() {
        // February 2025 has only 4 Mondays: 3, 10, 17, 24
        let date = nth_weekday_of_month(2025, 2, Weekday::Mon, 5);
        assert_eq!(date, None);
    }

    #[test]
    fn test_nth_weekday_first_saturday_march_2025() {
        // March 1, 2025 is a Saturday
        let date = nth_weekday_of_month(2025, 3, Weekday::Sat, 1);
        assert_eq!(date, Some(NaiveDate::from_ymd_opt(2025, 3, 1).unwrap()));
    }

    #[test]
    fn test_nth_weekday_all_days_january_2025() {
        // Jan 1, 2025 is a Wednesday; verify first of every weekday
        let cases = [
            (Weekday::Mon, 6u32),
            (Weekday::Tue, 7),
            (Weekday::Wed, 1),
            (Weekday::Thu, 2),
            (Weekday::Fri, 3),
            (Weekday::Sat, 4),
            (Weekday::Sun, 5),
        ];
        for (weekday, day) in cases {
            assert_eq!(
                nth_weekday_of_month(2025, 1, weekday, 1),
                Some(NaiveDate::from_ymd_opt(2025, 1, day).unwrap()),
                "first {:?} of January 2025",
                weekday,
            );
        }
    }

    // ── last_weekday_of_month ─────────────────────────────────────────────────

    #[test]
    fn test_last_weekday_memorial_day_2025() {
        let date = last_weekday_of_month(2025, 5, Weekday::Mon);
        assert_eq!(date, Some(NaiveDate::from_ymd_opt(2025, 5, 26).unwrap()));
    }

    #[test]
    fn test_last_weekday_memorial_day_2026() {
        let date = last_weekday_of_month(2026, 5, Weekday::Mon);
        assert_eq!(date, Some(NaiveDate::from_ymd_opt(2026, 5, 25).unwrap()));
    }

    #[test]
    fn test_last_weekday_last_friday_february_2025() {
        // Feb 2025 ends on the 28th (Friday)
        let date = last_weekday_of_month(2025, 2, Weekday::Fri);
        assert_eq!(date, Some(NaiveDate::from_ymd_opt(2025, 2, 28).unwrap()));
    }

    #[test]
    fn test_last_weekday_last_sunday_december_2025() {
        // Dec 2025: Sundays on 7, 14, 21, 28
        let date = last_weekday_of_month(2025, 12, Weekday::Sun);
        assert_eq!(date, Some(NaiveDate::from_ymd_opt(2025, 12, 28).unwrap()));
    }

    // ── parse_flexible_date keywords ──────────────────────────────────────────

    #[test]
    fn test_parse_keyword_easter() {
        assert_eq!(
            parse_flexible_date("Easter", 2025),
            Some(NaiveDate::from_ymd_opt(2025, 4, 20).unwrap()),
        );
    }

    #[test]
    fn test_parse_keyword_easter_case_insensitive() {
        assert_eq!(
            parse_flexible_date("EASTER", 2025),
            Some(NaiveDate::from_ymd_opt(2025, 4, 20).unwrap()),
        );
    }

    #[test]
    fn test_parse_keyword_thanksgiving() {
        assert_eq!(
            parse_flexible_date("Thanksgiving", 2025),
            Some(NaiveDate::from_ymd_opt(2025, 11, 27).unwrap()),
        );
    }

    #[test]
    fn test_parse_keyword_laborday() {
        assert_eq!(
            parse_flexible_date("LaborDay", 2025),
            Some(NaiveDate::from_ymd_opt(2025, 9, 1).unwrap()),
        );
    }

    #[test]
    fn test_parse_keyword_memorialday() {
        assert_eq!(
            parse_flexible_date("MemorialDay", 2025),
            Some(NaiveDate::from_ymd_opt(2025, 5, 26).unwrap()),
        );
    }

    #[test]
    fn test_parse_keyword_mlkday() {
        assert_eq!(
            parse_flexible_date("MLKDay", 2025),
            Some(NaiveDate::from_ymd_opt(2025, 1, 20).unwrap()),
        );
    }

    #[test]
    fn test_parse_fixed_date_february() {
        assert_eq!(
            parse_flexible_date("February 6", 2025),
            Some(NaiveDate::from_ymd_opt(2025, 2, 6).unwrap()),
        );
    }

    #[test]
    fn test_parse_fixed_date_december() {
        assert_eq!(
            parse_flexible_date("December 25", 2025),
            Some(NaiveDate::from_ymd_opt(2025, 12, 25).unwrap()),
        );
    }

    // ── parse_relative_date via parse_flexible_date ───────────────────────────

    #[test]
    fn test_parse_relative_first_monday() {
        assert_eq!(
            parse_flexible_date("The first Monday of September", 2025),
            Some(NaiveDate::from_ymd_opt(2025, 9, 1).unwrap()),
        );
    }

    #[test]
    fn test_parse_relative_second_sunday() {
        assert_eq!(
            parse_flexible_date("The second Sunday of May", 2025),
            Some(NaiveDate::from_ymd_opt(2025, 5, 11).unwrap()),
        );
    }

    #[test]
    fn test_parse_relative_third_sunday() {
        assert_eq!(
            parse_flexible_date("The third Sunday of June", 2025),
            Some(NaiveDate::from_ymd_opt(2025, 6, 15).unwrap()),
        );
    }

    #[test]
    fn test_parse_relative_fourth_thursday() {
        assert_eq!(
            parse_flexible_date("The fourth Thursday of November", 2025),
            Some(NaiveDate::from_ymd_opt(2025, 11, 27).unwrap()),
        );
    }

    #[test]
    fn test_parse_relative_fifth_monday() {
        // December 2025 has five Mondays: 1, 8, 15, 22, 29
        assert_eq!(
            parse_flexible_date("The fifth Monday of December", 2025),
            Some(NaiveDate::from_ymd_opt(2025, 12, 29).unwrap()),
        );
    }

    #[test]
    fn test_parse_relative_invalid_ordinal() {
        assert_eq!(parse_flexible_date("The sixth Monday of January", 2025), None);
    }

    #[test]
    fn test_parse_relative_invalid_weekday() {
        assert_eq!(parse_flexible_date("The first Blursday of January", 2025), None);
    }

    #[test]
    fn test_parse_invalid_date() {
        assert_eq!(parse_flexible_date("Invalid date", 2025), None);
    }

    #[test]
    fn test_parse_empty_string() {
        assert_eq!(parse_flexible_date("", 2025), None);
    }

    // ── get_current_period boundaries ─────────────────────────────────────────

    #[test]
    fn test_period_exact_base_date() {
        let ps = vec![period("P", "February 6", 0, 0)];
        assert_eq!(get_current_period(&ps, NaiveDate::from_ymd_opt(2025, 2, 6).unwrap()), "P");
    }

    #[test]
    fn test_period_start_boundary() {
        // Feb 6 - 3 days = Feb 3; should match
        let ps = vec![period("P", "February 6", 3, 2)];
        assert_eq!(get_current_period(&ps, NaiveDate::from_ymd_opt(2025, 2, 3).unwrap()), "P");
    }

    #[test]
    fn test_period_end_boundary() {
        // Feb 6 + 2 days = Feb 8; should match
        let ps = vec![period("P", "February 6", 3, 2)];
        assert_eq!(get_current_period(&ps, NaiveDate::from_ymd_opt(2025, 2, 8).unwrap()), "P");
    }

    #[test]
    fn test_period_one_day_before_start() {
        // Feb 6 - 3 - 1 = Feb 2; should not match
        let ps = vec![period("P", "February 6", 3, 2)];
        assert_eq!(get_current_period(&ps, NaiveDate::from_ymd_opt(2025, 2, 2).unwrap()), "Default");
    }

    #[test]
    fn test_period_one_day_after_end() {
        // Feb 6 + 2 + 1 = Feb 9; should not match
        let ps = vec![period("P", "February 6", 3, 2)];
        assert_eq!(get_current_period(&ps, NaiveDate::from_ymd_opt(2025, 2, 9).unwrap()), "Default");
    }

    #[test]
    fn test_period_empty_list_returns_default() {
        let ps: Vec<(String, TimePeriod)> = vec![];
        assert_eq!(get_current_period(&ps, NaiveDate::from_ymd_opt(2025, 6, 15).unwrap()), "Default");
    }

    #[test]
    fn test_period_zero_window_adjacent_days_miss() {
        let ps = vec![period("P", "February 6", 0, 0)];
        assert_eq!(get_current_period(&ps, NaiveDate::from_ymd_opt(2025, 2, 5).unwrap()), "Default");
        assert_eq!(get_current_period(&ps, NaiveDate::from_ymd_opt(2025, 2, 7).unwrap()), "Default");
    }

    #[test]
    fn test_period_multiple_matches_joined_by_space() {
        let ps = vec![
            period("Period1", "February 6", 2, 2),
            period("Period2", "February 6", 1, 1),
        ];
        assert_eq!(
            get_current_period(&ps, NaiveDate::from_ymd_opt(2025, 2, 6).unwrap()),
            "Period1 Period2",
        );
    }

    #[test]
    fn test_period_only_wider_period_matches_boundary() {
        // Feb 4 is within Period1 (days_before=2) but outside Period2 (days_before=1)
        let ps = vec![
            period("Period1", "February 6", 2, 2),
            period("Period2", "February 6", 1, 1),
        ];
        assert_eq!(
            get_current_period(&ps, NaiveDate::from_ymd_opt(2025, 2, 4).unwrap()),
            "Period1",
        );
    }

    // ── YAML deserialization ──────────────────────────────────────────────────

    #[test]
    fn test_yaml_roundtrip_basic() {
        let yaml = "Date: Easter\nDaysBefore: 3\nDaysAfter: 2\n";
        let tp: TimePeriod = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(tp.date, "Easter");
        assert_eq!(tp.days_before, 3);
        assert_eq!(tp.days_after, 2);
    }

    #[test]
    fn test_yaml_ignores_comment_field() {
        let yaml = "Date: Easter\nDaysBefore: 5\nDaysAfter: 2\nComment: Easter celebration\n";
        let tp: TimePeriod = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(tp.date, "Easter");
        assert_eq!(tp.days_before, 5);
    }

    #[test]
    fn test_yaml_zero_days() {
        let yaml = "Date: Christmas\nDaysBefore: 0\nDaysAfter: 0\n";
        let tp: TimePeriod = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(tp.days_before, 0);
        assert_eq!(tp.days_after, 0);
    }

    #[test]
    fn test_yaml_missing_required_field_fails() {
        let yaml = "Date: Easter\nDaysBefore: 3\n";
        assert!(serde_yaml::from_str::<TimePeriod>(yaml).is_err());
    }
}
