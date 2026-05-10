//! Formatting Utilities - Phase 40
//!
//! Provides locale-aware formatting functions for numbers, currencies,
//! and percentages with RTL language support.
//!
//! All monetary formatting functions accept `Decimal` for exact precision.

use rust_decimal::Decimal;

/// Format a Decimal number for display with locale awareness.
///
/// Uses Western Arabic numerals (0-9) by default, which are standard
/// in Libya and most Arab countries for financial/business contexts.
///
/// # Arguments
///
/// * `value` - The Decimal number to format
/// * `decimals` - Number of decimal places
/// * `locale` - The locale code ("ar", "en", "fr")
///
/// # Returns
///
/// A formatted string representation of the number
///
/// # Examples
///
/// ```rust
/// use e2manage_pos_terminal::utils::format_number;
/// use rust_decimal::Decimal;
///
/// assert_eq!(format_number(Decimal::new(1234567, 3), 3, "en"), "1,234.567");
/// assert_eq!(format_number(Decimal::new(1234567, 3), 3, "ar"), "1,234.567");
/// ```
pub fn format_number(value: Decimal, decimals: u32, locale: &str) -> String {
    let abs_value = value.abs();
    let sign = if value.is_sign_negative() { "-" } else { "" };

    // Round to specified decimal places
    let rounded = abs_value.round_dp(decimals);

    // Format with the right number of decimal places
    let formatted = if decimals > 0 {
        // Use Display formatting with precision
        format!("{:.prec$}", rounded, prec = decimals as usize)
    } else {
        format!("{}", rounded)
    };

    // Split into integer and decimal parts
    let parts: Vec<&str> = formatted.split('.').collect();
    let integer_part = parts[0];
    let decimal_part = parts.get(1).copied().unwrap_or("");

    // Add thousand separators
    let integer_with_separators = add_thousand_separators(integer_part, locale);

    // Get decimal separator based on locale
    let decimal_sep = decimal_separator(locale);

    if decimals > 0 {
        format!(
            "{}{}{}{}",
            sign, integer_with_separators, decimal_sep, decimal_part
        )
    } else {
        format!("{}{}", sign, integer_with_separators)
    }
}

/// Format a currency amount for display.
///
/// Handles locale-specific currency positioning:
/// - Arabic: Amount followed by currency symbol (e.g., "1,234.567 د.ل")
/// - English: Amount followed by currency code (e.g., "1,234.567 LYD")
///
/// # Arguments
///
/// * `amount` - The Decimal amount to format
/// * `currency` - The currency code (e.g., "LYD", "USD")
/// * `locale` - The locale code
///
/// # Returns
///
/// A formatted currency string
///
/// # Examples
///
/// ```rust
/// use e2manage_pos_terminal::utils::format_currency;
/// use rust_decimal::Decimal;
///
/// assert_eq!(format_currency(Decimal::new(1234567, 3), "LYD", "ar"), "1,234.567 د.ل");
/// assert_eq!(format_currency(Decimal::new(1234567, 3), "LYD", "en"), "1,234.567 LYD");
/// ```
pub fn format_currency(amount: Decimal, currency: &str, locale: &str) -> String {
    let decimals = currency_decimals(currency);
    let formatted_amount = format_number(amount, decimals, locale);
    let currency_symbol = get_currency_symbol(currency, locale);

    match locale {
        "ar" => {
            // Arabic: Amount followed by currency symbol
            format!("{} {}", formatted_amount, currency_symbol)
        }
        "fr" => {
            // French: Amount followed by currency (with non-breaking space)
            format!("{} {}", formatted_amount, currency_symbol)
        }
        _ => {
            // English/default: Amount followed by currency code
            format!("{} {}", formatted_amount, currency_symbol)
        }
    }
}

/// Format a percentage value.
///
/// # Arguments
///
/// * `value` - The Decimal percentage value (e.g., 15.5 for 15.5%)
/// * `decimals` - Number of decimal places
/// * `locale` - The locale code
///
/// # Returns
///
/// A formatted percentage string
///
/// # Examples
///
/// ```rust
/// use e2manage_pos_terminal::utils::format_percentage;
/// use rust_decimal::Decimal;
///
/// assert_eq!(format_percentage(Decimal::new(155, 1), 1, "en"), "15.5%");
/// assert_eq!(format_percentage(Decimal::new(155, 1), 1, "ar"), "15.5%");
/// ```
pub fn format_percentage(value: Decimal, decimals: u32, locale: &str) -> String {
    let formatted = format_number(value, decimals, locale);

    match locale {
        "ar" => format!("{}%", formatted),
        _ => format!("{}%", formatted),
    }
}

/// Format quantity for display (integer with thousand separators).
///
/// # Arguments
///
/// * `quantity` - The quantity value
/// * `locale` - The locale code
///
/// # Returns
///
/// A formatted quantity string
pub fn format_quantity(quantity: i64, locale: &str) -> String {
    format_number(Decimal::from(quantity), 0, locale)
}

/// Get the decimal separator for a locale.
fn decimal_separator(locale: &str) -> &'static str {
    match locale {
        "fr" => ",", // French uses comma as decimal separator
        _ => ".",    // English, Arabic use period
    }
}

/// Get the thousand separator for a locale.
fn thousand_separator(locale: &str) -> &'static str {
    match locale {
        "fr" => " ", // French uses space as thousand separator
        _ => ",",    // English, Arabic use comma
    }
}

/// Add thousand separators to an integer string.
fn add_thousand_separators(integer_str: &str, locale: &str) -> String {
    let sep = thousand_separator(locale);
    let chars: Vec<char> = integer_str.chars().collect();
    let len = chars.len();

    if len <= 3 {
        return integer_str.to_string();
    }

    let mut result = String::with_capacity(len + (len - 1) / 3);
    for (i, c) in chars.iter().enumerate() {
        if i > 0 && (len - i).is_multiple_of(3) {
            result.push_str(sep);
        }
        result.push(*c);
    }

    result
}

/// Get the number of decimal places for a currency.
pub fn currency_decimals(currency: &str) -> u32 {
    match currency {
        // Libyan Dinar uses 3 decimal places
        "LYD" => 3,
        // Japanese Yen, Korean Won use 0 decimal places
        "JPY" | "KRW" => 0,
        // Kuwaiti Dinar, Bahraini Dinar use 3 decimal places
        "KWD" | "BHD" | "OMR" => 3,
        // Most currencies use 2 decimal places
        _ => 2,
    }
}

/// Get the currency symbol for display based on locale.
fn get_currency_symbol(currency: &str, locale: &str) -> &'static str {
    match (currency, locale) {
        // Libyan Dinar
        ("LYD", "ar") => "د.ل",
        ("LYD", _) => "LYD",
        // US Dollar
        ("USD", "ar") => "دولار",
        ("USD", _) => "USD",
        // Euro
        ("EUR", "ar") => "يورو",
        ("EUR", _) => "EUR",
        // Saudi Riyal
        ("SAR", "ar") => "ر.س",
        ("SAR", _) => "SAR",
        // UAE Dirham
        ("AED", "ar") => "د.إ",
        ("AED", _) => "AED",
        // Egyptian Pound
        ("EGP", "ar") => "ج.م",
        ("EGP", _) => "EGP",
        // British Pound
        ("GBP", "ar") => "جنيه",
        ("GBP", _) => "GBP",
        // Default: use currency code
        (code, _) => {
            // Return the code - we can't return a dynamically created string
            // so we fall back to a static string
            match code {
                "TND" => "TND",
                "DZD" => "DZD",
                "MAD" => "MAD",
                "JOD" => "JOD",
                "KWD" => "KWD",
                "BHD" => "BHD",
                "OMR" => "OMR",
                "QAR" => "QAR",
                _ => "---",
            }
        }
    }
}

/// Convert Western Arabic numerals to Eastern Arabic numerals.
///
/// Eastern Arabic numerals: ٠١٢٣٤٥٦٧٨٩
/// Note: This is optional and not used by default since Libyan business
/// context typically uses Western Arabic numerals (0-9).
///
/// # Arguments
///
/// * `input` - String containing Western Arabic numerals
///
/// # Returns
///
/// String with numerals converted to Eastern Arabic
#[allow(dead_code)]
pub fn to_eastern_arabic(input: &str) -> String {
    input
        .chars()
        .map(|c| match c {
            '0' => '٠',
            '1' => '١',
            '2' => '٢',
            '3' => '٣',
            '4' => '٤',
            '5' => '٥',
            '6' => '٦',
            '7' => '٧',
            '8' => '٨',
            '9' => '٩',
            _ => c,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_number_basic() {
        assert_eq!(
            format_number(Decimal::new(1234567, 3), 3, "en"),
            "1,234.567"
        );
        assert_eq!(format_number(Decimal::new(1234567, 3), 2, "en"), "1,234.57");
        assert_eq!(format_number(Decimal::new(1234567, 3), 0, "en"), "1,235");
    }

    #[test]
    fn test_format_number_arabic() {
        // Arabic uses Western Arabic numerals with comma separators
        assert_eq!(
            format_number(Decimal::new(1234567, 3), 3, "ar"),
            "1,234.567"
        );
    }

    #[test]
    fn test_format_number_french() {
        // French uses space as thousand separator and comma as decimal
        assert_eq!(
            format_number(Decimal::new(1234567, 3), 3, "fr"),
            "1 234,567"
        );
    }

    #[test]
    fn test_format_number_negative() {
        assert_eq!(
            format_number(Decimal::new(-1234567, 3), 3, "en"),
            "-1,234.567"
        );
    }

    #[test]
    fn test_format_number_small() {
        assert_eq!(format_number(Decimal::new(12345, 2), 2, "en"), "123.45");
        assert_eq!(format_number(Decimal::new(123, 1), 2, "en"), "12.30");
        assert_eq!(format_number(Decimal::ONE, 2, "en"), "1.00");
    }

    #[test]
    fn test_format_number_large() {
        assert_eq!(
            format_number(Decimal::new(1234567890123i64, 3), 3, "en"),
            "1,234,567,890.123"
        );
    }

    #[test]
    fn test_format_currency_lyd() {
        assert_eq!(
            format_currency(Decimal::new(1234567, 3), "LYD", "ar"),
            "1,234.567 د.ل"
        );
        assert_eq!(
            format_currency(Decimal::new(1234567, 3), "LYD", "en"),
            "1,234.567 LYD"
        );
    }

    #[test]
    fn test_format_currency_usd() {
        assert_eq!(
            format_currency(Decimal::new(123456, 2), "USD", "ar"),
            "1,234.56 دولار"
        );
        assert_eq!(
            format_currency(Decimal::new(123456, 2), "USD", "en"),
            "1,234.56 USD"
        );
    }

    #[test]
    fn test_format_percentage() {
        assert_eq!(format_percentage(Decimal::new(155, 1), 1, "en"), "15.5%");
        assert_eq!(format_percentage(Decimal::new(155, 1), 1, "ar"), "15.5%");
        assert_eq!(format_percentage(Decimal::from(100), 0, "en"), "100%");
    }

    #[test]
    fn test_format_quantity() {
        assert_eq!(format_quantity(1234, "en"), "1,234");
        assert_eq!(format_quantity(1000000, "en"), "1,000,000");
    }

    #[test]
    fn test_to_eastern_arabic() {
        assert_eq!(to_eastern_arabic("0123456789"), "٠١٢٣٤٥٦٧٨٩");
        assert_eq!(to_eastern_arabic("1,234.567"), "١,٢٣٤.٥٦٧");
    }

    #[test]
    fn test_currency_decimals() {
        assert_eq!(currency_decimals("LYD"), 3);
        assert_eq!(currency_decimals("USD"), 2);
        assert_eq!(currency_decimals("JPY"), 0);
        assert_eq!(currency_decimals("KWD"), 3);
    }

    #[test]
    fn test_format_number_zero() {
        assert_eq!(format_number(Decimal::ZERO, 3, "en"), "0.000");
        assert_eq!(format_number(Decimal::ZERO, 0, "en"), "0");
    }

    #[test]
    fn test_format_number_exact_decimal() {
        // Verify Decimal precision: 0.1 + 0.2 == 0.3 exactly
        let a = Decimal::new(1, 1); // 0.1
        let b = Decimal::new(2, 1); // 0.2
        let sum = a + b;
        assert_eq!(format_number(sum, 1, "en"), "0.3");
    }
}
