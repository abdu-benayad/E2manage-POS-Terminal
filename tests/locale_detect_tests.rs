//! Unit tests for src/locale_detect.rs. Pure-function tests — no env mutation
//! relied upon between cases (each case passes the env value explicitly).

use e2manage_pos_terminal::locale_detect::detect_from_env;

#[test]
fn arabic_libya_is_rtl() {
    assert_eq!(detect_from_env(Some("ar_LY.UTF-8"), None), ("ar", true));
}

#[test]
fn arabic_saudi_is_rtl() {
    assert_eq!(detect_from_env(Some("ar_SA.UTF-8"), None), ("ar", true));
}

#[test]
fn english_us_is_ltr() {
    assert_eq!(detect_from_env(Some("en_US.UTF-8"), None), ("en", false));
}

#[test]
fn french_is_ltr() {
    assert_eq!(detect_from_env(Some("fr_FR.UTF-8"), None), ("fr", false));
}

#[test]
fn unset_defaults_to_english_ltr() {
    assert_eq!(detect_from_env(None, None), ("en", false));
}

#[test]
fn lc_all_takes_precedence_over_lang() {
    assert_eq!(
        detect_from_env(Some("en_US.UTF-8"), Some("ar_LY.UTF-8")),
        ("ar", true)
    );
}

#[test]
fn empty_string_treated_as_unset() {
    assert_eq!(detect_from_env(Some(""), None), ("en", false));
}

#[test]
fn unknown_locale_falls_back_to_english() {
    assert_eq!(detect_from_env(Some("ja_JP.UTF-8"), None), ("en", false));
}
