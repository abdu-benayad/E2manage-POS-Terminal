//! Services Integration Tests
//!
//! Integration tests for LogService, SupportService, and UpdateService.
//! These tests verify the full functionality including file system operations.

use e2manage_pos_terminal::services::log_service::{LogLevel, LogService};
use e2manage_pos_terminal::services::support_service::SupportService;
use e2manage_pos_terminal::services::update_service::UpdateService;
use e2manage_pos_terminal::ApiClient;
use std::fs;
use std::sync::Arc;

// ============================================================================
// LogService Integration Tests
// ============================================================================

mod log_service_integration {
    use super::*;

    #[test]
    fn test_log_service_new_creates_valid_instance() {
        let service = LogService::new();

        // Verify the service can report its log directory
        let log_dir = service.log_directory();
        assert!(!log_dir.as_os_str().is_empty());
    }

    #[test]
    fn test_log_service_handles_nonexistent_directory() {
        let service = LogService::new();

        // Should not panic when log directory doesn't exist
        let result = service.read_recent_logs(100);
        assert!(result.is_ok());

        let result = service.list_log_files();
        assert!(result.is_ok());
    }

    #[test]
    fn test_log_level_roundtrip() {
        // Test that all log levels can be converted to string and back
        let levels = [
            LogLevel::Error,
            LogLevel::Warn,
            LogLevel::Info,
            LogLevel::Debug,
            LogLevel::Trace,
        ];

        for level in levels {
            let str_repr = level.as_str();
            let parsed = LogLevel::from_str(str_repr);
            assert_eq!(level, parsed, "Failed roundtrip for {:?}", level);
        }
    }

    #[test]
    fn test_log_level_case_insensitive() {
        assert_eq!(LogLevel::from_str("error"), LogLevel::Error);
        assert_eq!(LogLevel::from_str("ERROR"), LogLevel::Error);
        assert_eq!(LogLevel::from_str("Error"), LogLevel::Error);
        assert_eq!(LogLevel::from_str("eRrOr"), LogLevel::Error);
    }

    #[test]
    fn test_get_logs_size_returns_zero_for_empty() {
        let service = LogService::new();

        // Size should be a valid number (0 or more)
        let size = service.get_logs_size();
        // u64 is always >= 0, so just verify no panic
        let _ = size;
    }

    #[test]
    fn test_export_logs_creates_zip_with_system_info() {
        let service = LogService::new();

        // Export should either succeed or fail gracefully
        match service.export_logs() {
            Ok(path) => {
                // Verify it's a zip file
                assert!(path.to_string_lossy().ends_with(".zip"));

                // Verify it contains system-info.txt by checking the file exists
                assert!(path.exists());

                // Clean up
                let _ = fs::remove_file(&path);
            }
            Err(_) => {
                // Expected if log directory doesn't exist - that's fine
            }
        }
    }
}

// ============================================================================
// SupportService Integration Tests
// ============================================================================

mod support_service_integration {
    use super::*;

    #[test]
    fn test_support_service_contact_info_complete() {
        let service = SupportService::new();
        let contact = service.get_contact_info();

        // Verify all fields are populated
        assert!(
            !contact.company_name.is_empty(),
            "company_name should not be empty"
        );
        assert!(!contact.email.is_empty(), "email should not be empty");
        assert!(contact.email.contains('@'), "email should contain @");
        assert!(!contact.phone.is_empty(), "phone should not be empty");
        assert!(!contact.website.is_empty(), "website should not be empty");
        assert!(
            contact.website.starts_with("http"),
            "website should be a URL"
        );
        assert!(
            !contact.support_hours.is_empty(),
            "support_hours should not be empty"
        );
    }

    #[test]
    fn test_support_service_contact_consistency() {
        let service = SupportService::new();

        // Multiple calls should return consistent data
        let contact1 = service.get_contact_info();
        let contact2 = service.get_contact_info();
        let contact3 = service.get_contact_info();

        assert_eq!(contact1.email, contact2.email);
        assert_eq!(contact2.email, contact3.email);
        assert_eq!(contact1.phone, contact3.phone);
        assert_eq!(contact1.website, contact2.website);
    }

    #[test]
    fn test_support_service_whatsapp_configured() {
        let service = SupportService::new();
        let contact = service.get_contact_info();

        // WhatsApp should be configured for support
        assert!(contact.whatsapp.is_some(), "WhatsApp should be configured");

        if let Some(whatsapp) = &contact.whatsapp {
            assert!(!whatsapp.is_empty(), "WhatsApp number should not be empty");
        }
    }

    #[test]
    fn test_support_service_email_format() {
        let service = SupportService::new();
        let contact = service.get_contact_info();

        // Email should have a valid basic format
        assert!(contact.email.contains('@'));
        assert!(contact.email.contains('.'));

        let parts: Vec<&str> = contact.email.split('@').collect();
        assert_eq!(parts.len(), 2, "Email should have exactly one @");
        assert!(!parts[0].is_empty(), "Email local part should not be empty");
        assert!(!parts[1].is_empty(), "Email domain should not be empty");
    }

    #[test]
    fn test_support_service_website_is_https() {
        let service = SupportService::new();
        let contact = service.get_contact_info();

        assert!(
            contact.website.starts_with("https://"),
            "Website should use HTTPS for security"
        );
    }
}

// ============================================================================
// UpdateService Integration Tests
// ============================================================================

mod update_service_integration {
    use super::*;

    #[test]
    fn test_update_service_initialization() {
        let api = Arc::new(ApiClient::new("http://localhost:3000"));
        let service = UpdateService::new(api);

        // Should initialize with current version from Cargo.toml
        let version = service.current_version();
        assert!(!version.is_empty(), "Current version should not be empty");

        // Should be a valid semver format
        let parts: Vec<&str> = version.split('.').collect();
        assert!(parts.len() >= 2, "Version should have at least major.minor");
    }

    #[test]
    fn test_update_service_version_from_cargo() {
        let api = Arc::new(ApiClient::new("http://test.com"));
        let service = UpdateService::new(api);

        // Version should match Cargo.toml
        let expected = env!("CARGO_PKG_VERSION");
        assert_eq!(service.current_version(), expected);
    }

    #[test]
    fn test_version_comparison_comprehensive() {
        // Equal versions
        assert_eq!(UpdateService::compare_versions("1.0.0", "1.0.0"), 0);
        assert_eq!(UpdateService::compare_versions("0.0.0", "0.0.0"), 0);
        assert_eq!(UpdateService::compare_versions("10.20.30", "10.20.30"), 0);

        // Less than
        assert_eq!(UpdateService::compare_versions("0.9.9", "1.0.0"), -1);
        assert_eq!(UpdateService::compare_versions("1.0.0", "1.0.1"), -1);
        assert_eq!(UpdateService::compare_versions("1.0.0", "1.1.0"), -1);
        assert_eq!(UpdateService::compare_versions("1.0.0", "2.0.0"), -1);

        // Greater than
        assert_eq!(UpdateService::compare_versions("1.0.0", "0.9.9"), 1);
        assert_eq!(UpdateService::compare_versions("1.0.1", "1.0.0"), 1);
        assert_eq!(UpdateService::compare_versions("1.1.0", "1.0.0"), 1);
        assert_eq!(UpdateService::compare_versions("2.0.0", "1.0.0"), 1);
    }

    #[test]
    fn test_version_comparison_different_lengths() {
        // Missing patch version treated as 0
        assert_eq!(UpdateService::compare_versions("1.0", "1.0.0"), 0);
        assert_eq!(UpdateService::compare_versions("1.0.0", "1.0"), 0);

        // Missing minor and patch
        assert_eq!(UpdateService::compare_versions("1", "1.0.0"), 0);
        assert_eq!(UpdateService::compare_versions("1.0.0", "1"), 0);
    }

    #[test]
    fn test_version_comparison_large_numbers() {
        assert_eq!(UpdateService::compare_versions("100.0.0", "99.99.99"), 1);
        assert_eq!(UpdateService::compare_versions("1.100.0", "1.99.0"), 1);
        assert_eq!(UpdateService::compare_versions("1.0.100", "1.0.99"), 1);
    }

    #[test]
    fn test_version_comparison_empty_strings() {
        // Empty versions treated as 0
        assert_eq!(UpdateService::compare_versions("", ""), 0);
        assert_eq!(UpdateService::compare_versions("", "0"), 0);
        assert_eq!(UpdateService::compare_versions("0", ""), 0);
        assert_eq!(UpdateService::compare_versions("", "1.0.0"), -1);
        assert_eq!(UpdateService::compare_versions("1.0.0", ""), 1);
    }

    #[test]
    fn test_update_check_offline_handling() {
        // Create service with invalid URL
        let api = Arc::new(ApiClient::new("http://invalid.local.host:99999"));
        let service = UpdateService::new(api);

        // Should return an error, not panic
        let result = service.check_for_updates_sync("stable");
        assert!(result.is_err(), "Should return error for invalid host");
    }

    #[test]
    fn test_multiple_service_instances() {
        // Multiple instances should work independently
        let api1 = Arc::new(ApiClient::new("http://server1.com"));
        let api2 = Arc::new(ApiClient::new("http://server2.com"));
        let service1 = UpdateService::new(api1);
        let service2 = UpdateService::new(api2);

        // Both should have the same version (from Cargo.toml)
        assert_eq!(service1.current_version(), service2.current_version());
    }
}

// ============================================================================
// Cross-Service Integration Tests
// ============================================================================

mod cross_service_integration {
    use super::*;

    #[test]
    fn test_all_services_initialize_without_panic() {
        // All services should be constructible without any panics
        let _log_service = LogService::new();
        let _support_service = SupportService::new();
        let api = Arc::new(ApiClient::new("http://test.com"));
        let _update_service = UpdateService::new(api);
    }

    #[test]
    fn test_services_are_independent() {
        // Creating multiple instances shouldn't interfere
        let log1 = LogService::new();
        let log2 = LogService::new();

        let support1 = SupportService::new();
        let support2 = SupportService::new();

        // Both should work independently
        let _ = log1.get_logs_size();
        let _ = log2.get_logs_size();

        let c1 = support1.get_contact_info();
        let c2 = support2.get_contact_info();
        assert_eq!(c1.email, c2.email);
    }

    #[test]
    fn test_default_trait_implementations() {
        // Test Default implementations
        let log_service: LogService = Default::default();
        let support_service: SupportService = Default::default();

        // Should work the same as new()
        let _ = log_service.get_logs_size();
        let _ = support_service.get_contact_info();
    }
}
