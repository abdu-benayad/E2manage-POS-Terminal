//! Detects user locale + RTL flag from LC_ALL / LANG environment variables.
//! Pure function; the env-var read happens at the call site so unit tests can
//! pass values directly without mutating process state.

/// Pure detector. Pass `(lang, lc_all)` from the caller. `LC_ALL` wins.
/// Returns `(locale_code, rtl)` where `locale_code` is one of `"ar"`,
/// `"en"`, `"fr"` (extend the match arm as new locales are supported).
pub fn detect_from_env(lang: Option<&str>, lc_all: Option<&str>) -> (&'static str, bool) {
    let raw = lc_all
        .filter(|s| !s.is_empty())
        .or(lang.filter(|s| !s.is_empty()));

    match raw {
        Some(s) if s.to_ascii_lowercase().starts_with("ar") => ("ar", true),
        Some(s) if s.to_ascii_lowercase().starts_with("fr") => ("fr", false),
        _ => ("en", false),
    }
}

/// Convenience entry point that reads the actual env vars. Used from `main.rs`.
pub fn detect_locale() -> (&'static str, bool) {
    let lc_all = std::env::var("LC_ALL").ok();
    let lang = std::env::var("LANG").ok();
    detect_from_env(lang.as_deref(), lc_all.as_deref())
}
