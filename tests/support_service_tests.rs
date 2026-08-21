//! Support Service Unit Tests
//!
//! Tests for support contact information and action generation.

use e2manage_pos_terminal::services::support_service::{SupportContact, SupportService};

// ============================================================================
// SupportContact Tests
// ============================================================================

#[test]
fn test_support_contact_default() {
    let contact = SupportContact::default();

    assert!(!contact.company_name.is_empty());
    assert!(!contact.email.is_empty());
    assert!(contact.email.contains('@'));
    assert!(!contact.phone.is_empty());
    assert!(!contact.website.is_empty());
    assert!(contact.website.starts_with("http"));
    assert!(!contact.support_hours.is_empty());
}

#[test]
fn test_support_contact_has_whatsapp() {
    let contact = SupportContact::default();
    assert!(contact.whatsapp.is_some());
}

#[test]
fn test_support_contact_clone() {
    let contact = SupportContact::default();
    let cloned = contact.clone();

    assert_eq!(cloned.company_name, contact.company_name);
    assert_eq!(cloned.email, contact.email);
    assert_eq!(cloned.phone, contact.phone);
    assert_eq!(cloned.whatsapp, contact.whatsapp);
    assert_eq!(cloned.website, contact.website);
    assert_eq!(cloned.support_hours, contact.support_hours);
}

#[test]
fn test_support_contact_debug() {
    let contact = SupportContact::default();
    let debug_str = format!("{:?}", contact);

    assert!(debug_str.contains("SupportContact"));
    assert!(debug_str.contains("email"));
}

#[test]
fn test_support_contact_custom() {
    let contact = SupportContact {
        company_name: "Test Company".to_string(),
        email: "test@example.com".to_string(),
        phone: "+1-555-1234".to_string(),
        whatsapp: None,
        website: "https://example.com".to_string(),
        support_hours: "24/7".to_string(),
    };

    assert_eq!(contact.company_name, "Test Company");
    assert_eq!(contact.email, "test@example.com");
    assert!(contact.whatsapp.is_none());
}

// ============================================================================
// SupportService Initialization Tests
// ============================================================================

#[test]
fn test_support_service_new() {
    let service = SupportService::new();
    // Should initialize without panicking
    let _ = service.get_contact_info();
}

#[test]
fn test_support_service_default() {
    // Constructed through the `Default` bound rather than `SupportService::default()`:
    // the direct call on a unit struct is what `clippy::default_constructed_unit_structs`
    // flags, and the generic position is the only place the impl actually earns its keep.
    fn default_of<T: Default>() -> T {
        T::default()
    }

    let service: SupportService = default_of();
    let contact = service.get_contact_info();
    assert!(!contact.email.is_empty());
}

// ============================================================================
// SupportService Contact Info Tests
// ============================================================================

#[test]
fn test_get_contact_info_returns_valid_data() {
    let service = SupportService::new();
    let contact = service.get_contact_info();

    // Verify all fields are populated
    assert!(!contact.company_name.is_empty());
    assert!(!contact.email.is_empty());
    assert!(!contact.phone.is_empty());
    assert!(!contact.website.is_empty());
    assert!(!contact.support_hours.is_empty());
}

#[test]
fn test_get_contact_info_consistent() {
    let service = SupportService::new();

    // Multiple calls should return same data
    let contact1 = service.get_contact_info();
    let contact2 = service.get_contact_info();

    assert_eq!(contact1.email, contact2.email);
    assert_eq!(contact1.phone, contact2.phone);
    assert_eq!(contact1.website, contact2.website);
}

// ============================================================================
// Email Generation Tests (test URL construction without opening)
// ============================================================================

#[test]
fn test_email_url_format() {
    // Test the expected mailto URL format
    let terminal_id = "TERM-001";
    let app_version = "1.2.0";

    let subject = format!("POS Support Request - Terminal {}", terminal_id);
    let body = format!(
        "Terminal ID: {}\nApp Version: {}\n\nPlease describe your issue:\n\n",
        terminal_id, app_version
    );

    let mailto = format!(
        "mailto:support@e2manage.com?subject={}&body={}",
        urlencoding::encode(&subject),
        urlencoding::encode(&body)
    );

    assert!(mailto.starts_with("mailto:"));
    assert!(mailto.contains("subject="));
    assert!(mailto.contains("body="));
    assert!(mailto.contains("TERM-001"));
}

// ============================================================================
// Phone URL Tests
// ============================================================================

#[test]
fn test_phone_number_formatting() {
    let phone = "+966-XXX-XXX-XXXX";
    let cleaned = phone.replace(['-', ' '], "");

    // Verify dashes and spaces are removed
    assert!(!cleaned.contains('-'));
    assert!(!cleaned.contains(' '));
    assert!(cleaned.starts_with("+966"));
}

#[test]
fn test_tel_url_format() {
    let phone = "+966123456789";
    let tel_url = format!("tel:{}", phone);

    assert!(tel_url.starts_with("tel:"));
    assert!(tel_url.contains("+966"));
}

// ============================================================================
// WhatsApp URL Tests
// ============================================================================

#[test]
fn test_whatsapp_url_format() {
    let whatsapp_number = "+966123456789";
    let terminal_id = "TERM-001";
    let message = format!("Hello, I need support for POS Terminal {}", terminal_id);

    let url = format!(
        "https://wa.me/{}?text={}",
        whatsapp_number.replace(['+', '-', ' '], ""),
        urlencoding::encode(&message)
    );

    assert!(url.starts_with("https://wa.me/"));
    assert!(url.contains("966123456789"));
    assert!(url.contains("text="));
}

#[test]
fn test_whatsapp_number_cleaning() {
    let number = "+966-123-456-789";
    let cleaned = number.replace(['+', '-', ' '], "");

    assert_eq!(cleaned, "966123456789");
    assert!(!cleaned.contains('+'));
    assert!(!cleaned.contains('-'));
}

// ============================================================================
// Website URL Tests
// ============================================================================

#[test]
fn test_website_is_https() {
    let service = SupportService::new();
    let contact = service.get_contact_info();

    assert!(contact.website.starts_with("https://"));
}

// ============================================================================
// URL Encoding Tests
// ============================================================================

#[test]
fn test_url_encoding_special_chars() {
    let text = "Hello & Welcome! (Test)";
    let encoded = urlencoding::encode(text);

    assert!(!encoded.contains(' '));
    assert!(!encoded.contains('&'));
    assert!(encoded.contains("%20") || encoded.contains("+")); // Space encoding
}

#[test]
fn test_url_encoding_arabic() {
    let text = "مرحبا";
    let encoded = urlencoding::encode(text);

    // Arabic should be percent-encoded
    assert!(encoded.contains('%'));
    assert!(!encoded.contains("مرحبا"));
}

#[test]
fn test_url_encoding_newlines() {
    let text = "Line1\nLine2";
    let encoded = urlencoding::encode(text);

    // Newlines should be encoded
    assert!(!encoded.contains('\n'));
    assert!(encoded.contains("%0A"));
}

// ============================================================================
// Edge Case Tests
// ============================================================================

#[test]
fn test_empty_terminal_id() {
    let terminal_id = "";
    let subject = format!("POS Support Request - Terminal {}", terminal_id);

    // Should still create valid URL even with empty terminal ID
    assert!(subject.contains("Terminal"));
}

#[test]
fn test_special_chars_in_terminal_id() {
    let terminal_id = "TERM/001&<>";
    let encoded = urlencoding::encode(terminal_id);

    // Special chars should be encoded
    assert!(!encoded.contains('/'));
    assert!(!encoded.contains('&'));
    assert!(!encoded.contains('<'));
    assert!(!encoded.contains('>'));
}
