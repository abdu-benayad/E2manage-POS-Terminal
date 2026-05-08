//! ESC/POS Command Builder
//!
//! This module provides a fluent builder for constructing ESC/POS commands
//! for thermal receipt printers. ESC/POS is the industry standard protocol
//! for POS receipt printers by Epson and compatible brands.
//!
//! ## Features
//!
//! - Text formatting (bold, underline, alignment, size)
//! - Barcodes (Code39, Code128, EAN-13)
//! - QR codes
//! - Cash drawer control
//! - Paper cutting
//! - Two-column printing for receipts
//!
//! ## Usage
//!
//! ```rust,ignore
//! use crate::services::escpos::{EscPos, Alignment, BarcodeFormat};
//!
//! let commands = EscPos::new()
//!     .init()
//!     .align(Alignment::Center)
//!     .text_size(2, 2)
//!     .bold(true)
//!     .line("STORE NAME")
//!     .text_size(1, 1)
//!     .bold(false)
//!     .line("123 Main Street")
//!     .hr(42)
//!     .align(Alignment::Left)
//!     .two_col("Item", "10.000", 42)
//!     .hr(42)
//!     .bold(true)
//!     .two_col("TOTAL", "10.000", 42)
//!     .feed(3)
//!     .cut()
//!     .open_drawer()
//!     .build();
//!
//! // Send commands to printer
//! std::fs::write("/dev/usb/lp0", &commands)?;
//! ```

/// ESC/POS command builder
#[derive(Clone, Debug)]
pub struct EscPos {
    buffer: Vec<u8>,
    /// Current paper width in characters (for formatting)
    paper_width: usize,
}

impl EscPos {
    /// Creates a new ESC/POS command builder
    pub fn new() -> Self {
        Self {
            buffer: Vec::with_capacity(4096),
            paper_width: 42, // Default for 80mm paper
        }
    }

    /// Creates a builder with custom paper width
    pub fn with_paper_width(width: usize) -> Self {
        Self {
            buffer: Vec::with_capacity(4096),
            paper_width: width,
        }
    }

    /// Sets the paper width (character columns)
    pub fn set_paper_width(&mut self, width: usize) -> &mut Self {
        self.paper_width = width;
        self
    }

    /// Returns the current paper width
    pub fn paper_width(&self) -> usize {
        self.paper_width
    }

    // ============================================
    // Initialization & Reset
    // ============================================

    /// Initialize printer (reset to default settings)
    /// ESC @ (0x1B 0x40)
    pub fn init(&mut self) -> &mut Self {
        self.buffer.extend_from_slice(&[0x1B, 0x40]);
        self
    }

    /// Reset printer (same as init)
    pub fn reset(&mut self) -> &mut Self {
        self.init()
    }

    // ============================================
    // Text Alignment
    // ============================================

    /// Set text alignment
    /// ESC a n (0x1B 0x61 n)
    pub fn align(&mut self, alignment: Alignment) -> &mut Self {
        self.buffer.extend_from_slice(&[0x1B, 0x61, alignment as u8]);
        self
    }

    /// Align left
    pub fn align_left(&mut self) -> &mut Self {
        self.align(Alignment::Left)
    }

    /// Align center
    pub fn align_center(&mut self) -> &mut Self {
        self.align(Alignment::Center)
    }

    /// Align right
    pub fn align_right(&mut self) -> &mut Self {
        self.align(Alignment::Right)
    }

    // ============================================
    // Text Size
    // ============================================

    /// Set text size (width and height multiplier)
    /// GS ! n (0x1D 0x21 n)
    /// Width: 1-8, Height: 1-8
    pub fn text_size(&mut self, width: u8, height: u8) -> &mut Self {
        let w = (width.clamp(1, 8) - 1) & 0x07;
        let h = (height.clamp(1, 8) - 1) & 0x07;
        let size = (w << 4) | h;
        self.buffer.extend_from_slice(&[0x1D, 0x21, size]);
        self
    }

    /// Set normal text size (1x1)
    pub fn normal(&mut self) -> &mut Self {
        self.text_size(1, 1)
    }

    /// Set double-width text
    pub fn double_width(&mut self) -> &mut Self {
        self.text_size(2, 1)
    }

    /// Set double-height text
    pub fn double_height(&mut self) -> &mut Self {
        self.text_size(1, 2)
    }

    /// Set double-width and double-height text
    pub fn double_size(&mut self) -> &mut Self {
        self.text_size(2, 2)
    }

    // ============================================
    // Text Formatting
    // ============================================

    /// Set bold text on/off
    /// ESC E n (0x1B 0x45 n)
    pub fn bold(&mut self, on: bool) -> &mut Self {
        self.buffer.extend_from_slice(&[0x1B, 0x45, if on { 1 } else { 0 }]);
        self
    }

    /// Enable bold
    pub fn bold_on(&mut self) -> &mut Self {
        self.bold(true)
    }

    /// Disable bold
    pub fn bold_off(&mut self) -> &mut Self {
        self.bold(false)
    }

    /// Set underline on/off
    /// ESC - n (0x1B 0x2D n)
    /// 0 = off, 1 = single, 2 = double
    pub fn underline(&mut self, mode: u8) -> &mut Self {
        let m = mode.min(2);
        self.buffer.extend_from_slice(&[0x1B, 0x2D, m]);
        self
    }

    /// Enable single underline
    pub fn underline_on(&mut self) -> &mut Self {
        self.underline(1)
    }

    /// Enable double underline
    pub fn underline_double(&mut self) -> &mut Self {
        self.underline(2)
    }

    /// Disable underline
    pub fn underline_off(&mut self) -> &mut Self {
        self.underline(0)
    }

    /// Set inverse (white on black) printing
    /// GS B n (0x1D 0x42 n)
    pub fn inverse(&mut self, on: bool) -> &mut Self {
        self.buffer.extend_from_slice(&[0x1D, 0x42, if on { 1 } else { 0 }]);
        self
    }

    /// Set upside-down mode
    /// ESC { n (0x1B 0x7B n)
    pub fn upside_down(&mut self, on: bool) -> &mut Self {
        self.buffer.extend_from_slice(&[0x1B, 0x7B, if on { 1 } else { 0 }]);
        self
    }

    // ============================================
    // Character Set / Code Page
    // ============================================

    /// Select character code table
    /// ESC t n (0x1B 0x74 n)
    pub fn code_page(&mut self, page: CodePage) -> &mut Self {
        self.buffer.extend_from_slice(&[0x1B, 0x74, page as u8]);
        self
    }

    /// Select international character set
    /// ESC R n (0x1B 0x52 n)
    pub fn international_charset(&mut self, charset: u8) -> &mut Self {
        self.buffer.extend_from_slice(&[0x1B, 0x52, charset.min(15)]);
        self
    }

    // ============================================
    // Text Output
    // ============================================

    /// Print text (without newline)
    pub fn text(&mut self, text: &str) -> &mut Self {
        self.buffer.extend_from_slice(text.as_bytes());
        self
    }

    /// Print text with newline (line feed)
    pub fn line(&mut self, text: &str) -> &mut Self {
        self.text(text);
        self.newline()
    }

    /// Print newline (line feed)
    /// LF (0x0A)
    pub fn newline(&mut self) -> &mut Self {
        self.buffer.push(0x0A);
        self
    }

    /// Print multiple newlines
    pub fn newlines(&mut self, count: u8) -> &mut Self {
        for _ in 0..count {
            self.newline();
        }
        self
    }

    /// Feed n lines
    /// ESC d n (0x1B 0x64 n)
    pub fn feed(&mut self, lines: u8) -> &mut Self {
        self.buffer.extend_from_slice(&[0x1B, 0x64, lines]);
        self
    }

    /// Feed n dots (1/8mm units typically)
    /// ESC J n (0x1B 0x4A n)
    pub fn feed_dots(&mut self, dots: u8) -> &mut Self {
        self.buffer.extend_from_slice(&[0x1B, 0x4A, dots]);
        self
    }

    // ============================================
    // Receipt Formatting Helpers
    // ============================================

    /// Print horizontal rule (divider line)
    pub fn hr(&mut self, width: usize) -> &mut Self {
        self.line(&"-".repeat(width.min(self.paper_width)))
    }

    /// Print a dashed horizontal rule
    pub fn hr_dash(&mut self, width: usize) -> &mut Self {
        let w = width.min(self.paper_width);
        let pattern: String = (0..w / 2).map(|_| "- ").collect();
        self.line(pattern.trim_end())
    }

    /// Print a double horizontal rule
    pub fn hr_double(&mut self, width: usize) -> &mut Self {
        self.line(&"=".repeat(width.min(self.paper_width)))
    }

    /// Print a solid horizontal rule
    pub fn hr_solid(&mut self, width: usize) -> &mut Self {
        self.line(&"_".repeat(width.min(self.paper_width)))
    }

    /// Print two-column line (left-right justified)
    pub fn two_col(&mut self, left: &str, right: &str, width: usize) -> &mut Self {
        let w = width.min(self.paper_width);
        let left_len = self.display_width(left);
        let right_len = self.display_width(right);

        if left_len + right_len >= w {
            // Text is too wide, just concatenate
            self.line(&format!("{} {}", left, right))
        } else {
            let padding = w - left_len - right_len;
            self.line(&format!("{}{}{}", left, " ".repeat(padding), right))
        }
    }

    /// Print three-column line (left-center-right)
    pub fn three_col(&mut self, left: &str, center: &str, right: &str, width: usize) -> &mut Self {
        let w = width.min(self.paper_width);
        let left_len = self.display_width(left);
        let center_len = self.display_width(center);
        let right_len = self.display_width(right);
        let total_len = left_len + center_len + right_len;

        if total_len >= w {
            self.line(&format!("{} {} {}", left, center, right))
        } else {
            let available = w - total_len;
            let left_pad = available / 2;
            let right_pad = available - left_pad;
            self.line(&format!(
                "{}{}{}{}{}",
                left,
                " ".repeat(left_pad),
                center,
                " ".repeat(right_pad),
                right
            ))
        }
    }

    /// Print a row with label and value (common in receipts)
    pub fn row(&mut self, label: &str, value: &str) -> &mut Self {
        self.two_col(label, value, self.paper_width)
    }

    /// Print a row with formatted currency value
    pub fn row_amount(&mut self, label: &str, amount: f64, currency: &str) -> &mut Self {
        let value = format!("{} {:.3}", currency, amount);
        self.two_col(label, &value, self.paper_width)
    }

    /// Calculate display width (accounting for wide characters)
    fn display_width(&self, text: &str) -> usize {
        // Simple approximation - count characters
        // For accurate width, would need unicode width calculation
        text.chars().count()
    }

    // ============================================
    // Barcodes
    // ============================================

    /// Set barcode height (in dots)
    /// GS h n (0x1D 0x68 n)
    pub fn barcode_height(&mut self, height: u8) -> &mut Self {
        self.buffer.extend_from_slice(&[0x1D, 0x68, height.max(1)]);
        self
    }

    /// Set barcode width
    /// GS w n (0x1D 0x77 n) - 2 to 6
    pub fn barcode_width(&mut self, width: u8) -> &mut Self {
        self.buffer.extend_from_slice(&[0x1D, 0x77, width.clamp(2, 6)]);
        self
    }

    /// Set barcode text position
    /// GS H n (0x1D 0x48 n)
    pub fn barcode_text_position(&mut self, position: BarcodeTextPosition) -> &mut Self {
        self.buffer.extend_from_slice(&[0x1D, 0x48, position as u8]);
        self
    }

    /// Print barcode
    /// GS k m n d1...dn (0x1D 0x6B m)
    pub fn barcode(&mut self, data: &str, format: BarcodeFormat) -> &mut Self {
        match format {
            BarcodeFormat::Code39 | BarcodeFormat::Code93 | BarcodeFormat::Code128 => {
                // Format B: GS k m n d1...dn
                self.buffer.extend_from_slice(&[0x1D, 0x6B, format as u8, data.len() as u8]);
                self.buffer.extend_from_slice(data.as_bytes());
            }
            BarcodeFormat::Ean13 | BarcodeFormat::Ean8 | BarcodeFormat::UpcA | BarcodeFormat::UpcE => {
                // Format A: GS k m d1...dk NUL
                self.buffer.extend_from_slice(&[0x1D, 0x6B, format as u8]);
                self.buffer.extend_from_slice(data.as_bytes());
                self.buffer.push(0x00); // NUL terminator
            }
        }
        self
    }

    /// Print Code128 barcode with default settings
    pub fn barcode_code128(&mut self, data: &str) -> &mut Self {
        self.barcode_height(60)
            .barcode_width(3)
            .barcode_text_position(BarcodeTextPosition::Below)
            .barcode(data, BarcodeFormat::Code128)
    }

    // ============================================
    // QR Code
    // ============================================

    /// Print QR code
    /// Uses GS ( k commands (model 2)
    pub fn qr_code(&mut self, data: &str, size: u8) -> &mut Self {
        let data_bytes = data.as_bytes();
        let len = data_bytes.len() + 3;
        let pl = (len % 256) as u8;
        let ph = (len / 256) as u8;

        // Select model (Model 2)
        // GS ( k pL pH cn fn n1 n2
        self.buffer.extend_from_slice(&[0x1D, 0x28, 0x6B, 0x04, 0x00, 0x31, 0x41, 0x32, 0x00]);

        // Set size (1-16)
        // GS ( k pL pH cn fn n
        self.buffer.extend_from_slice(&[0x1D, 0x28, 0x6B, 0x03, 0x00, 0x31, 0x43, size.clamp(1, 16)]);

        // Set error correction level (L=48, M=49, Q=50, H=51)
        // GS ( k pL pH cn fn n
        self.buffer.extend_from_slice(&[0x1D, 0x28, 0x6B, 0x03, 0x00, 0x31, 0x45, 0x31]); // M

        // Store QR code data
        // GS ( k pL pH cn fn m d1...dk
        self.buffer.extend_from_slice(&[0x1D, 0x28, 0x6B, pl, ph, 0x31, 0x50, 0x30]);
        self.buffer.extend_from_slice(data_bytes);

        // Print QR code
        // GS ( k pL pH cn fn m
        self.buffer.extend_from_slice(&[0x1D, 0x28, 0x6B, 0x03, 0x00, 0x31, 0x51, 0x30]);

        self
    }

    /// Print QR code with default size (5)
    pub fn qr(&mut self, data: &str) -> &mut Self {
        self.qr_code(data, 5)
    }

    // ============================================
    // Paper Control
    // ============================================

    /// Full paper cut
    /// GS V n (0x1D 0x56 n)
    pub fn cut(&mut self) -> &mut Self {
        self.buffer.extend_from_slice(&[0x1D, 0x56, 0x00]);
        self
    }

    /// Partial paper cut (leaves small attachment)
    pub fn partial_cut(&mut self) -> &mut Self {
        self.buffer.extend_from_slice(&[0x1D, 0x56, 0x01]);
        self
    }

    /// Cut with feed
    /// GS V m n (0x1D 0x56 m n)
    pub fn cut_with_feed(&mut self, feed_lines: u8) -> &mut Self {
        self.buffer.extend_from_slice(&[0x1D, 0x56, 0x42, feed_lines]);
        self
    }

    // ============================================
    // Cash Drawer
    // ============================================

    /// Open cash drawer
    /// ESC p m t1 t2 (0x1B 0x70 m t1 t2)
    /// Pin 2: m=0, Pin 5: m=1
    pub fn open_drawer(&mut self) -> &mut Self {
        // Pin 2, pulse on-time 25*2ms, pulse off-time 250*2ms
        self.buffer.extend_from_slice(&[0x1B, 0x70, 0x00, 0x19, 0xFA]);
        self
    }

    /// Open cash drawer (pin 5)
    pub fn open_drawer_pin5(&mut self) -> &mut Self {
        self.buffer.extend_from_slice(&[0x1B, 0x70, 0x01, 0x19, 0xFA]);
        self
    }

    // ============================================
    // Images
    // ============================================

    /// Print raster image
    /// GS v 0 m xL xH yL yH d1...dk
    /// This is a simplified implementation for monochrome bitmaps
    pub fn image(&mut self, width: u16, height: u16, data: &[u8]) -> &mut Self {
        let bytes_per_row = (width + 7) / 8;
        let xl = (bytes_per_row % 256) as u8;
        let xh = (bytes_per_row / 256) as u8;
        let yl = (height % 256) as u8;
        let yh = (height / 256) as u8;

        // GS v 0 m=0 (normal mode)
        self.buffer.extend_from_slice(&[0x1D, 0x76, 0x30, 0x00, xl, xh, yl, yh]);
        self.buffer.extend_from_slice(data);
        self
    }

    // ============================================
    // Raw Commands
    // ============================================

    /// Write raw bytes to buffer
    pub fn raw(&mut self, bytes: &[u8]) -> &mut Self {
        self.buffer.extend_from_slice(bytes);
        self
    }

    /// Write a single raw byte
    pub fn raw_byte(&mut self, byte: u8) -> &mut Self {
        self.buffer.push(byte);
        self
    }

    // ============================================
    // Build
    // ============================================

    /// Get the built command buffer
    pub fn build(&self) -> Vec<u8> {
        self.buffer.clone()
    }

    /// Get the buffer length
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// Check if buffer is empty
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Clear the buffer
    pub fn clear(&mut self) -> &mut Self {
        self.buffer.clear();
        self
    }
}

impl Default for EscPos {
    fn default() -> Self {
        Self::new()
    }
}

/// Text alignment options
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Alignment {
    /// Left alignment (default)
    Left = 0,
    /// Center alignment
    Center = 1,
    /// Right alignment
    Right = 2,
}

impl Default for Alignment {
    fn default() -> Self {
        Self::Left
    }
}

/// Barcode format types
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum BarcodeFormat {
    /// UPC-A barcode (11-12 digits)
    UpcA = 0,
    /// UPC-E barcode (6-8 digits)
    UpcE = 1,
    /// EAN-13 barcode (12-13 digits)
    Ean13 = 2,
    /// EAN-8 barcode (7-8 digits)
    Ean8 = 3,
    /// Code 39 barcode (alphanumeric)
    Code39 = 4,
    /// Code 93 barcode (alphanumeric)
    Code93 = 72,
    /// Code 128 barcode (alphanumeric, most flexible)
    Code128 = 73,
}

impl Default for BarcodeFormat {
    fn default() -> Self {
        Self::Code128
    }
}

/// Barcode text position
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum BarcodeTextPosition {
    /// No text printed
    None = 0,
    /// Text printed above barcode
    Above = 1,
    /// Text printed below barcode
    Below = 2,
    /// Text printed above and below
    Both = 3,
}

impl Default for BarcodeTextPosition {
    fn default() -> Self {
        Self::Below
    }
}

/// Character code page
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum CodePage {
    /// CP437 (US/Europe, default)
    Cp437 = 0,
    /// Katakana
    Katakana = 1,
    /// CP850 (Multilingual)
    Cp850 = 2,
    /// CP860 (Portuguese)
    Cp860 = 3,
    /// CP863 (Canadian-French)
    Cp863 = 4,
    /// CP865 (Nordic)
    Cp865 = 5,
    /// WCP1252 (Latin I)
    Wcp1252 = 16,
    /// CP866 (Cyrillic)
    Cp866 = 17,
    /// WCP1250 (Central Europe)
    Wcp1250 = 18,
    /// WCP1251 (Cyrillic)
    Wcp1251 = 19,
    /// WCP1253 (Greek)
    Wcp1253 = 20,
    /// WCP1254 (Turkish)
    Wcp1254 = 21,
    /// WCP1255 (Hebrew)
    Wcp1255 = 22,
    /// WCP1256 (Arabic)
    Wcp1256 = 23,
    /// WCP1257 (Baltic)
    Wcp1257 = 24,
    /// WCP1258 (Vietnamese)
    Wcp1258 = 25,
    /// UTF-8 (if supported by printer)
    Utf8 = 255,
}

impl Default for CodePage {
    fn default() -> Self {
        Self::Cp437
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escpos_new() {
        let esc = EscPos::new();
        assert!(esc.is_empty());
        assert_eq!(esc.paper_width(), 42);
    }

    #[test]
    fn test_escpos_with_paper_width() {
        let esc = EscPos::with_paper_width(32);
        assert_eq!(esc.paper_width(), 32);
    }

    #[test]
    fn test_init_command() {
        let commands = EscPos::new().init().build();
        assert_eq!(commands, vec![0x1B, 0x40]);
    }

    #[test]
    fn test_alignment() {
        let commands = EscPos::new()
            .align(Alignment::Left)
            .align(Alignment::Center)
            .align(Alignment::Right)
            .build();

        assert_eq!(commands, vec![
            0x1B, 0x61, 0x00, // Left
            0x1B, 0x61, 0x01, // Center
            0x1B, 0x61, 0x02, // Right
        ]);
    }

    #[test]
    fn test_text_size() {
        let commands = EscPos::new()
            .text_size(1, 1)
            .text_size(2, 2)
            .text_size(3, 1)
            .build();

        assert_eq!(commands, vec![
            0x1D, 0x21, 0x00, // 1x1
            0x1D, 0x21, 0x11, // 2x2
            0x1D, 0x21, 0x20, // 3x1
        ]);
    }

    #[test]
    fn test_text_size_clamp() {
        let commands = EscPos::new()
            .text_size(0, 0)  // Should clamp to 1,1
            .text_size(10, 10) // Should clamp to 8,8
            .build();

        assert_eq!(commands, vec![
            0x1D, 0x21, 0x00, // 1x1 (clamped from 0)
            0x1D, 0x21, 0x77, // 8x8 (clamped from 10)
        ]);
    }

    #[test]
    fn test_bold() {
        let commands = EscPos::new()
            .bold(true)
            .bold(false)
            .build();

        assert_eq!(commands, vec![
            0x1B, 0x45, 0x01, // Bold on
            0x1B, 0x45, 0x00, // Bold off
        ]);
    }

    #[test]
    fn test_underline() {
        let commands = EscPos::new()
            .underline(0)
            .underline(1)
            .underline(2)
            .build();

        assert_eq!(commands, vec![
            0x1B, 0x2D, 0x00, // Off
            0x1B, 0x2D, 0x01, // Single
            0x1B, 0x2D, 0x02, // Double
        ]);
    }

    #[test]
    fn test_text_output() {
        let commands = EscPos::new()
            .text("Hello")
            .newline()
            .build();

        assert_eq!(commands, vec![
            b'H', b'e', b'l', b'l', b'o', // Text
            0x0A, // LF
        ]);
    }

    #[test]
    fn test_line() {
        let commands = EscPos::new()
            .line("Test")
            .build();

        assert_eq!(commands, vec![b'T', b'e', b's', b't', 0x0A]);
    }

    #[test]
    fn test_feed() {
        let commands = EscPos::new()
            .feed(3)
            .build();

        assert_eq!(commands, vec![0x1B, 0x64, 0x03]);
    }

    #[test]
    fn test_horizontal_rule() {
        let commands = EscPos::new()
            .hr(10)
            .build();

        assert_eq!(commands, "----------\n".as_bytes());
    }

    #[test]
    fn test_two_col() {
        let mut esc = EscPos::with_paper_width(20);
        let commands = esc.two_col("Item", "10.00", 20).build();

        // "Item" (4) + spaces (11) + "10.00" (5) + newline = 20 chars + \n
        // 20 - 4 - 5 = 11 spaces
        assert_eq!(commands, b"Item           10.00\n");
    }

    #[test]
    fn test_cut() {
        let commands = EscPos::new()
            .cut()
            .build();

        assert_eq!(commands, vec![0x1D, 0x56, 0x00]);
    }

    #[test]
    fn test_partial_cut() {
        let commands = EscPos::new()
            .partial_cut()
            .build();

        assert_eq!(commands, vec![0x1D, 0x56, 0x01]);
    }

    #[test]
    fn test_open_drawer() {
        let commands = EscPos::new()
            .open_drawer()
            .build();

        assert_eq!(commands, vec![0x1B, 0x70, 0x00, 0x19, 0xFA]);
    }

    #[test]
    fn test_barcode_settings() {
        let commands = EscPos::new()
            .barcode_height(80)
            .barcode_width(3)
            .barcode_text_position(BarcodeTextPosition::Below)
            .build();

        assert_eq!(commands, vec![
            0x1D, 0x68, 80,   // Height
            0x1D, 0x77, 3,    // Width
            0x1D, 0x48, 0x02, // Text below
        ]);
    }

    #[test]
    fn test_barcode_ean13() {
        let commands = EscPos::new()
            .barcode("4006381333931", BarcodeFormat::Ean13)
            .build();

        // GS k 2 + data + NUL
        assert!(commands.starts_with(&[0x1D, 0x6B, 0x02]));
        assert_eq!(*commands.last().unwrap(), 0x00);
    }

    #[test]
    fn test_barcode_code128() {
        let commands = EscPos::new()
            .barcode("ABC123", BarcodeFormat::Code128)
            .build();

        // GS k 73 6 + data
        assert_eq!(&commands[0..4], &[0x1D, 0x6B, 73, 6]);
        assert_eq!(&commands[4..], b"ABC123");
    }

    #[test]
    fn test_qr_code() {
        let commands = EscPos::new()
            .qr_code("https://example.com", 5)
            .build();

        // Should contain QR code commands
        assert!(!commands.is_empty());
        // Check for model selection command
        assert!(commands.windows(4).any(|w| w == &[0x1D, 0x28, 0x6B, 0x04]));
    }

    #[test]
    fn test_inverse() {
        let commands = EscPos::new()
            .inverse(true)
            .inverse(false)
            .build();

        assert_eq!(commands, vec![
            0x1D, 0x42, 0x01,
            0x1D, 0x42, 0x00,
        ]);
    }

    #[test]
    fn test_code_page() {
        let commands = EscPos::new()
            .code_page(CodePage::Wcp1256)
            .build();

        assert_eq!(commands, vec![0x1B, 0x74, 23]);
    }

    #[test]
    fn test_raw() {
        let commands = EscPos::new()
            .raw(&[0x1B, 0x40])
            .raw_byte(0x0A)
            .build();

        assert_eq!(commands, vec![0x1B, 0x40, 0x0A]);
    }

    #[test]
    fn test_clear() {
        let mut esc = EscPos::new();
        esc.text("Hello");
        assert!(!esc.is_empty());

        esc.clear();
        assert!(esc.is_empty());
    }

    #[test]
    fn test_chaining() {
        let commands = EscPos::new()
            .init()
            .align_center()
            .double_size()
            .bold_on()
            .line("RECEIPT")
            .normal()
            .bold_off()
            .align_left()
            .hr(42)
            .feed(3)
            .cut()
            .build();

        assert!(!commands.is_empty());
        // Verify init at start
        assert_eq!(&commands[0..2], &[0x1B, 0x40]);
        // Verify cut at end
        let len = commands.len();
        assert_eq!(&commands[len - 3..], &[0x1D, 0x56, 0x00]);
    }

    #[test]
    fn test_receipt_example() {
        let commands = EscPos::new()
            .init()
            .align_center()
            .double_size()
            .bold_on()
            .line("STORE NAME")
            .normal()
            .bold_off()
            .line("123 Main St")
            .hr(42)
            .align_left()
            .two_col("Item 1", "10.000", 42)
            .two_col("Item 2", "15.000", 42)
            .hr(42)
            .bold_on()
            .two_col("TOTAL", "25.000", 42)
            .bold_off()
            .feed(3)
            .cut()
            .open_drawer()
            .build();

        // Just verify it builds without panic and has reasonable length
        assert!(commands.len() > 100);
    }

    #[test]
    fn test_alignment_enum_default() {
        assert_eq!(Alignment::default(), Alignment::Left);
    }

    #[test]
    fn test_barcode_format_default() {
        assert_eq!(BarcodeFormat::default(), BarcodeFormat::Code128);
    }

    #[test]
    fn test_barcode_text_position_default() {
        assert_eq!(BarcodeTextPosition::default(), BarcodeTextPosition::Below);
    }

    #[test]
    fn test_code_page_default() {
        assert_eq!(CodePage::default(), CodePage::Cp437);
    }
}
