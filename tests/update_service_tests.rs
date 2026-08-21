//! Update Service Unit Tests
//!
//! Tests for version checking, comparison, and update handling.

use e2manage_pos_terminal::services::update_service::{ReleaseNotes, UpdateService};
use e2manage_pos_terminal::ApiClient;
use std::sync::Arc;

// ============================================================================
// UpdateService Initialization Tests
// ============================================================================

#[test]
fn test_update_service_new() {
    let api = Arc::new(ApiClient::new("http://localhost:3000"));
    let service = UpdateService::new(api);
    assert!(!service.current_version().is_empty());
}

#[test]
fn test_update_service_current_version() {
    let api = Arc::new(ApiClient::new("http://test.com"));
    let service = UpdateService::new(api);
    let version = service.current_version();

    // Version should be in semantic format (x.y.z)
    assert!(version.contains('.'));
    assert!(!version.is_empty());
}

// ============================================================================
// Version Comparison Tests
// ============================================================================

#[test]
fn test_compare_versions_equal() {
    assert_eq!(UpdateService::compare_versions("1.0.0", "1.0.0"), 0);
    assert_eq!(UpdateService::compare_versions("2.5.3", "2.5.3"), 0);
    assert_eq!(UpdateService::compare_versions("0.0.1", "0.0.1"), 0);
}

#[test]
fn test_compare_versions_less_than() {
    assert_eq!(UpdateService::compare_versions("1.0.0", "1.0.1"), -1);
    assert_eq!(UpdateService::compare_versions("1.0.0", "1.1.0"), -1);
    assert_eq!(UpdateService::compare_versions("1.0.0", "2.0.0"), -1);
}

#[test]
fn test_compare_versions_greater_than() {
    assert_eq!(UpdateService::compare_versions("1.0.1", "1.0.0"), 1);
    assert_eq!(UpdateService::compare_versions("1.1.0", "1.0.0"), 1);
    assert_eq!(UpdateService::compare_versions("2.0.0", "1.0.0"), 1);
}

#[test]
fn test_compare_versions_different_lengths() {
    // Missing patch is treated as 0
    assert_eq!(UpdateService::compare_versions("1.0", "1.0.0"), 0);
    assert_eq!(UpdateService::compare_versions("1.0.0", "1.0"), 0);

    // Missing minor and patch
    assert_eq!(UpdateService::compare_versions("1", "1.0.0"), 0);
}

#[test]
fn test_compare_versions_large_numbers() {
    assert_eq!(UpdateService::compare_versions("10.0.0", "9.9.9"), 1);
    assert_eq!(UpdateService::compare_versions("1.100.0", "1.99.0"), 1);
    assert_eq!(UpdateService::compare_versions("1.0.100", "1.0.99"), 1);
}

#[test]
fn test_compare_versions_empty() {
    assert_eq!(UpdateService::compare_versions("", ""), 0);
    assert_eq!(UpdateService::compare_versions("1.0.0", ""), 1);
    assert_eq!(UpdateService::compare_versions("", "1.0.0"), -1);
}

#[test]
fn test_compare_versions_realistic() {
    assert_eq!(UpdateService::compare_versions("0.1.0", "0.1.1"), -1);
    assert_eq!(UpdateService::compare_versions("1.2.3", "1.2.4"), -1);
    assert_eq!(UpdateService::compare_versions("2.0.0", "1.9.9"), 1);
}

// ============================================================================
// ReleaseNotes Tests
// ============================================================================

#[test]
fn test_release_notes_from_string() {
    let notes = ReleaseNotes::from_string("Bug fixes and improvements");
    assert_eq!(notes.en, "Bug fixes and improvements");
    assert_eq!(notes.ar, "Bug fixes and improvements");
}

#[test]
fn test_release_notes_get_english() {
    let notes = ReleaseNotes {
        en: "English text".to_string(),
        ar: "نص عربي".to_string(),
    };
    assert_eq!(notes.get("en"), "English text");
    assert_eq!(notes.get("en-US"), "English text");
}

#[test]
fn test_release_notes_get_arabic() {
    let notes = ReleaseNotes {
        en: "English text".to_string(),
        ar: "نص عربي".to_string(),
    };
    assert_eq!(notes.get("ar"), "نص عربي");
    assert_eq!(notes.get("ar-SA"), "نص عربي");
}

#[test]
fn test_release_notes_default_to_english() {
    let notes = ReleaseNotes {
        en: "English".to_string(),
        ar: "عربي".to_string(),
    };
    // Unknown locales should fall back to English
    assert_eq!(notes.get("fr"), "English");
    assert_eq!(notes.get("de"), "English");
}

#[test]
fn test_release_notes_deserialize() {
    let json = r#"{"en": "English", "ar": "عربي"}"#;
    let notes: ReleaseNotes = serde_json::from_str(json).unwrap();

    assert_eq!(notes.en, "English");
    assert_eq!(notes.ar, "عربي");
}

// ============================================================================
// Version Checking Tests
// ============================================================================

#[test]
fn test_is_newer() {
    let api = Arc::new(ApiClient::new("http://test.com"));
    let service = UpdateService::new(api);

    // A very high version number should always be newer
    assert!(service.is_newer("99.0.0"));

    // Version 0.0.0 should not be newer than current
    assert!(!service.is_newer("0.0.0"));
}

// ============================================================================
// Edge Case Tests
// ============================================================================

#[test]
fn test_version_with_leading_zeros() {
    // Leading zeros in version components
    assert_eq!(UpdateService::compare_versions("1.01.0", "1.1.0"), 0);
    assert_eq!(UpdateService::compare_versions("01.0.0", "1.0.0"), 0);
}

#[test]
fn test_version_with_many_components() {
    // More than 3 components (e.g., 1.2.3.4)
    assert_eq!(UpdateService::compare_versions("1.0.0.0", "1.0.0.0"), 0);
    assert_eq!(UpdateService::compare_versions("1.0.0.1", "1.0.0.0"), 1);
}

#[test]
fn test_version_with_non_numeric() {
    // Non-numeric parts should be ignored
    assert_eq!(UpdateService::compare_versions("1.0.0-beta", "1.0.0"), 0);
    assert_eq!(UpdateService::compare_versions("1.0.0", "1.0.0-alpha"), 0);
}
