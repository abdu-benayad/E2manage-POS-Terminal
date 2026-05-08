//! Support Service - Contact information and support actions
//!
//! Provides functionality to:
//! - Get support contact information
//! - Open email client with pre-filled support request
//! - Open phone dialer
//! - Open WhatsApp chat
//! - Open support website

use std::process::Command;

/// Support contact information
#[derive(Debug, Clone)]
pub struct SupportContact {
    pub company_name: String,
    pub email: String,
    pub phone: String,
    pub whatsapp: Option<String>,
    pub website: String,
    pub support_hours: String,
}

impl Default for SupportContact {
    fn default() -> Self {
        Self {
            company_name: "E2Manage Support".to_string(),
            email: "support@e2manage.com".to_string(),
            phone: "+966-XXX-XXX-XXXX".to_string(),
            whatsapp: Some("+966XXXXXXXXX".to_string()),
            website: "https://e2manage.com/support".to_string(),
            support_hours: "Sun-Thu 9:00 AM - 6:00 PM (AST)".to_string(),
        }
    }
}

pub struct SupportService;

impl SupportService {
    pub fn new() -> Self {
        Self
    }

    /// Get support contact information
    pub fn get_contact_info(&self) -> SupportContact {
        // In future, this could fetch from backend or config
        SupportContact::default()
    }

    /// Open email client with pre-filled support email
    pub fn open_email(&self, terminal_id: &str, app_version: &str) {
        let contact = self.get_contact_info();
        let subject = format!("POS Support Request - Terminal {}", terminal_id);
        let body = format!(
            "Terminal ID: {}\nApp Version: {}\n\nPlease describe your issue:\n\n",
            terminal_id, app_version
        );

        let mailto = format!(
            "mailto:{}?subject={}&body={}",
            contact.email,
            urlencoding::encode(&subject),
            urlencoding::encode(&body)
        );

        Self::open_url(&mailto);
    }

    /// Open phone dialer
    pub fn open_phone(&self) {
        let contact = self.get_contact_info();
        let tel = format!("tel:{}", contact.phone.replace(['-', ' '], ""));
        Self::open_url(&tel);
    }

    /// Open WhatsApp chat
    pub fn open_whatsapp(&self, terminal_id: &str) {
        let contact = self.get_contact_info();
        if let Some(whatsapp) = contact.whatsapp {
            let message = format!("Hello, I need support for POS Terminal {}", terminal_id);
            let url = format!(
                "https://wa.me/{}?text={}",
                whatsapp.replace(['+', '-', ' '], ""),
                urlencoding::encode(&message)
            );
            Self::open_url(&url);
        }
    }

    /// Open support website
    pub fn open_website(&self) {
        let contact = self.get_contact_info();
        Self::open_url(&contact.website);
    }

    /// Platform-specific URL opener
    fn open_url(url: &str) {
        #[cfg(target_os = "linux")]
        {
            Command::new("xdg-open").arg(url).spawn().ok();
        }

        #[cfg(target_os = "windows")]
        {
            Command::new("cmd")
                .args(["/C", "start", "", url])
                .spawn()
                .ok();
        }

        #[cfg(target_os = "macos")]
        {
            Command::new("open").arg(url).spawn().ok();
        }
    }

    /// Open a folder in the system file manager
    pub fn open_folder(path: &std::path::Path) {
        #[cfg(target_os = "linux")]
        {
            Command::new("xdg-open").arg(path).spawn().ok();
        }

        #[cfg(target_os = "windows")]
        {
            Command::new("explorer").arg(path).spawn().ok();
        }

        #[cfg(target_os = "macos")]
        {
            Command::new("open").arg(path).spawn().ok();
        }
    }
}

impl Default for SupportService {
    fn default() -> Self {
        Self::new()
    }
}
