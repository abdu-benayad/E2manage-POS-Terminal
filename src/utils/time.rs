//! Time Utilities - Phase 29
//!
//! Provides time-related utility functions for the POS terminal,
//! including human-readable "time ago" formatting with localization.

use chrono::{DateTime, Utc};

/// Formats a datetime as a human-readable "time ago" string.
///
/// # Arguments
///
/// * `dt` - The datetime to format
/// * `locale` - The locale code ("ar" for Arabic, "en" for English, etc.)
///
/// # Returns
///
/// A localized string like "just now", "5 min ago", "2 hours ago", "3 days ago"
///
/// # Examples
///
/// ```rust
/// use chrono::Utc;
/// use e2manage_pos_terminal::utils::time_ago;
///
/// let now = Utc::now();
/// assert_eq!(time_ago(now, "en"), "just now");
/// assert_eq!(time_ago(now, "ar"), "الآن");
/// ```
pub fn time_ago(dt: DateTime<Utc>, locale: &str) -> String {
    let now = Utc::now();
    let diff = now.signed_duration_since(dt);

    let seconds = diff.num_seconds();
    let minutes = diff.num_minutes();
    let hours = diff.num_hours();
    let days = diff.num_days();
    let weeks = days / 7;
    let months = days / 30;

    match locale {
        "ar" => {
            if seconds < 60 {
                "الآن".to_string()
            } else if minutes < 60 {
                if minutes == 1 {
                    "منذ دقيقة".to_string()
                } else if minutes == 2 {
                    "منذ دقيقتين".to_string()
                } else if minutes >= 3 && minutes <= 10 {
                    format!("منذ {} دقائق", minutes)
                } else {
                    format!("منذ {} دقيقة", minutes)
                }
            } else if hours < 24 {
                if hours == 1 {
                    "منذ ساعة".to_string()
                } else if hours == 2 {
                    "منذ ساعتين".to_string()
                } else if hours >= 3 && hours <= 10 {
                    format!("منذ {} ساعات", hours)
                } else {
                    format!("منذ {} ساعة", hours)
                }
            } else if days < 7 {
                if days == 1 {
                    "منذ يوم".to_string()
                } else if days == 2 {
                    "منذ يومين".to_string()
                } else if days >= 3 && days <= 10 {
                    format!("منذ {} أيام", days)
                } else {
                    format!("منذ {} يوم", days)
                }
            } else if weeks < 4 {
                if weeks == 1 {
                    "منذ أسبوع".to_string()
                } else if weeks == 2 {
                    "منذ أسبوعين".to_string()
                } else {
                    format!("منذ {} أسابيع", weeks)
                }
            } else {
                if months == 1 {
                    "منذ شهر".to_string()
                } else if months == 2 {
                    "منذ شهرين".to_string()
                } else if months >= 3 && months <= 10 {
                    format!("منذ {} أشهر", months)
                } else {
                    format!("منذ {} شهر", months)
                }
            }
        }
        "fr" => {
            if seconds < 60 {
                "à l'instant".to_string()
            } else if minutes < 60 {
                format!("il y a {} min", minutes)
            } else if hours < 24 {
                if hours == 1 {
                    "il y a 1 heure".to_string()
                } else {
                    format!("il y a {} heures", hours)
                }
            } else if days < 7 {
                if days == 1 {
                    "il y a 1 jour".to_string()
                } else {
                    format!("il y a {} jours", days)
                }
            } else if weeks < 4 {
                if weeks == 1 {
                    "il y a 1 semaine".to_string()
                } else {
                    format!("il y a {} semaines", weeks)
                }
            } else {
                if months == 1 {
                    "il y a 1 mois".to_string()
                } else {
                    format!("il y a {} mois", months)
                }
            }
        }
        _ => {
            // Default to English
            if seconds < 60 {
                "just now".to_string()
            } else if minutes < 60 {
                if minutes == 1 {
                    "1 min ago".to_string()
                } else {
                    format!("{} min ago", minutes)
                }
            } else if hours < 24 {
                if hours == 1 {
                    "1 hour ago".to_string()
                } else {
                    format!("{} hours ago", hours)
                }
            } else if days < 7 {
                if days == 1 {
                    "1 day ago".to_string()
                } else {
                    format!("{} days ago", days)
                }
            } else if weeks < 4 {
                if weeks == 1 {
                    "1 week ago".to_string()
                } else {
                    format!("{} weeks ago", weeks)
                }
            } else {
                if months == 1 {
                    "1 month ago".to_string()
                } else {
                    format!("{} months ago", months)
                }
            }
        }
    }
}

/// Formats a duration in seconds as a human-readable countdown string.
///
/// # Arguments
///
/// * `seconds` - The number of seconds remaining
/// * `locale` - The locale code
///
/// # Returns
///
/// A localized string like "5:00", "4:30", etc.
pub fn format_countdown(seconds: i64, _locale: &str) -> String {
    if seconds < 0 {
        return "0:00".to_string();
    }

    let mins = seconds / 60;
    let secs = seconds % 60;
    format!("{}:{:02}", mins, secs)
}

/// Formats a timestamp for display on receipts.
///
/// # Arguments
///
/// * `dt` - The datetime to format
/// * `locale` - The locale code
///
/// # Returns
///
/// A formatted date/time string suitable for receipts
pub fn format_receipt_datetime(dt: DateTime<Utc>, locale: &str) -> String {
    match locale {
        "ar" => dt.format("%Y/%m/%d %H:%M").to_string(),
        _ => dt.format("%Y-%m-%d %H:%M").to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn test_time_ago_just_now_en() {
        let now = Utc::now();
        assert_eq!(time_ago(now, "en"), "just now");
    }

    #[test]
    fn test_time_ago_just_now_ar() {
        let now = Utc::now();
        assert_eq!(time_ago(now, "ar"), "الآن");
    }

    #[test]
    fn test_time_ago_minutes_en() {
        let dt = Utc::now() - Duration::minutes(5);
        assert_eq!(time_ago(dt, "en"), "5 min ago");
    }

    #[test]
    fn test_time_ago_minutes_ar() {
        let dt = Utc::now() - Duration::minutes(5);
        assert_eq!(time_ago(dt, "ar"), "منذ 5 دقائق");
    }

    #[test]
    fn test_time_ago_one_minute_en() {
        let dt = Utc::now() - Duration::minutes(1);
        assert_eq!(time_ago(dt, "en"), "1 min ago");
    }

    #[test]
    fn test_time_ago_hours_en() {
        let dt = Utc::now() - Duration::hours(3);
        assert_eq!(time_ago(dt, "en"), "3 hours ago");
    }

    #[test]
    fn test_time_ago_hours_ar() {
        let dt = Utc::now() - Duration::hours(3);
        assert_eq!(time_ago(dt, "ar"), "منذ 3 ساعات");
    }

    #[test]
    fn test_time_ago_days_en() {
        let dt = Utc::now() - Duration::days(2);
        assert_eq!(time_ago(dt, "en"), "2 days ago");
    }

    #[test]
    fn test_time_ago_days_ar() {
        let dt = Utc::now() - Duration::days(2);
        assert_eq!(time_ago(dt, "ar"), "منذ يومين");
    }

    #[test]
    fn test_time_ago_weeks_en() {
        let dt = Utc::now() - Duration::weeks(2);
        assert_eq!(time_ago(dt, "en"), "2 weeks ago");
    }

    #[test]
    fn test_time_ago_french() {
        let dt = Utc::now() - Duration::minutes(5);
        assert_eq!(time_ago(dt, "fr"), "il y a 5 min");
    }

    #[test]
    fn test_format_countdown() {
        assert_eq!(format_countdown(300, "en"), "5:00");
        assert_eq!(format_countdown(150, "en"), "2:30");
        assert_eq!(format_countdown(65, "en"), "1:05");
        assert_eq!(format_countdown(0, "en"), "0:00");
        assert_eq!(format_countdown(-10, "en"), "0:00");
    }

    #[test]
    fn test_format_receipt_datetime() {
        let dt = chrono::TimeZone::with_ymd_and_hms(&Utc, 2024, 1, 15, 14, 30, 0).unwrap();
        assert_eq!(format_receipt_datetime(dt, "en"), "2024-01-15 14:30");
        assert_eq!(format_receipt_datetime(dt, "ar"), "2024/01/15 14:30");
    }
}
