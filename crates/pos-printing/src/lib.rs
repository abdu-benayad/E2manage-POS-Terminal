//! Print Service
//!
//! This module provides receipt printing functionality using ESC/POS commands.
//! It supports printing receipts, X-Reports, Z-Reports, and opening cash drawers.
//!
//! ## Features
//!
//! - Receipt printing with store info, items, payments, and barcodes
//! - X-Report (shift summary) printing
//! - Z-Report (end-of-day) printing
//! - Cash drawer control
//! - Console preview mode for development
//! - Multi-language support (Arabic/English)
//!
//! ## Usage
//!
//! ```rust,ignore
//! use crate::services::print_service::{PrintService, PrinterConfig};
//! use crate::models::transaction::Transaction;
//!
//! let config = PrinterConfig {
//!     printer_path: Some("/dev/usb/lp0".to_string()),
//!     paper_width: 42,
//!     store_name: "My Store".to_string(),
//!     store_name_ar: Some("متجري".to_string()),
//!     store_address: "123 Main St".to_string(),
//!     store_phone: Some("+218 91 1234567".to_string()),
//!     store_tax_id: Some("TAX-12345".to_string()),
//!     ..Default::default()
//! };
//!
//! let printer = PrintService::new(config);
//! printer.print_receipt(&transaction, "en")?;
//! printer.open_drawer()?;
//! ```

use pos_models::{PaymentMethod, Transaction};
use pos_escpos::{CodePage, EscPos};
use anyhow::{Context, Result};
use rust_decimal::Decimal;
use rust_decimal::prelude::*;
use tracing::info;

// warn! is used in Windows/unsupported platform code branches
#[cfg(not(target_os = "linux"))]
use tracing::warn;

/// Printer configuration
#[derive(Clone, Debug)]
pub struct PrinterConfig {
    /// Path to printer device (e.g., "/dev/usb/lp0" on Linux)
    /// If None, prints to console for debugging
    pub printer_path: Option<String>,
    /// Paper width in characters (42 for 80mm, 32 for 58mm)
    pub paper_width: usize,
    /// Store name (English)
    pub store_name: String,
    /// Store name (Arabic)
    pub store_name_ar: Option<String>,
    /// Store address
    pub store_address: String,
    /// Store address (Arabic)
    pub store_address_ar: Option<String>,
    /// Store phone number
    pub store_phone: Option<String>,
    /// Store tax registration number
    pub store_tax_id: Option<String>,
    /// Terminal ID
    pub terminal_id: String,
    /// Enable QR code printing
    pub print_qr_code: bool,
    /// Enable barcode printing
    pub print_barcode: bool,
    /// Print customer copy (duplicate receipt)
    pub print_customer_copy: bool,
    /// Footer text (English)
    pub footer_text: String,
    /// Footer text (Arabic)
    pub footer_text_ar: Option<String>,
}

impl Default for PrinterConfig {
    fn default() -> Self {
        Self {
            printer_path: None, // Console mode by default
            paper_width: 42,    // 80mm paper
            store_name: "E2Manage POS".to_string(),
            store_name_ar: Some("نقطة البيع".to_string()),
            store_address: String::new(),
            store_address_ar: None,
            store_phone: None,
            store_tax_id: None,
            terminal_id: "TERM-001".to_string(),
            print_qr_code: false,
            print_barcode: true,
            print_customer_copy: false,
            footer_text: "Thank you for your visit!".to_string(),
            footer_text_ar: Some("شكراً لزيارتكم!".to_string()),
        }
    }
}

/// Shift report data for X and Z reports
#[derive(Clone, Debug, Default)]
pub struct ShiftReport {
    /// Shift number/ID
    pub shift_number: String,
    /// Operator name
    pub operator_name: String,
    /// Terminal ID
    pub terminal_id: String,
    /// When shift started
    pub start_time: String,
    /// When shift ended (for Z-Report)
    pub end_time: Option<String>,
    /// Total number of transactions
    pub transaction_count: u32,
    /// Gross sales amount
    pub gross_sales: Decimal,
    /// Total discounts given
    pub discounts: Decimal,
    /// Total tax collected
    pub tax: Decimal,
    /// Net sales (gross - discounts + tax)
    pub net_sales: Decimal,
    /// Cash payments total
    pub cash_total: Decimal,
    /// Card payments total
    pub card_total: Decimal,
    /// Wallet payments total
    pub wallet_total: Decimal,
    /// Credit payments total
    pub credit_total: Decimal,
    /// Opening float (cash in drawer at start)
    pub opening_float: Decimal,
    /// Expected cash in drawer
    pub expected_cash: Decimal,
    /// Counted cash in drawer (for Z-Report)
    pub counted_cash: Option<Decimal>,
    /// Cash variance (counted - expected)
    pub cash_variance: Option<Decimal>,
    /// Number of returns processed
    pub return_count: u32,
    /// Total value of returns
    pub return_total: Decimal,
    /// Number of voided transactions
    pub void_count: u32,
    /// Currency code
    pub currency: String,
}

/// Print service for receipt and report printing
#[derive(Clone)]
pub struct PrintService {
    config: PrinterConfig,
}

impl PrintService {
    /// Creates a new print service with the given configuration
    pub fn new(config: PrinterConfig) -> Self {
        Self { config }
    }

    /// Creates a print service in console-only mode (for development)
    pub fn console_only() -> Self {
        Self::new(PrinterConfig::default())
    }

    /// Returns whether a printer is configured
    pub fn has_printer(&self) -> bool {
        self.config.printer_path.is_some()
    }

    /// Gets the current configuration
    pub fn config(&self) -> &PrinterConfig {
        &self.config
    }

    /// Updates the printer path
    pub fn set_printer_path(&mut self, path: Option<String>) {
        self.config.printer_path = path;
    }

    // ============================================
    // Receipt Printing
    // ============================================

    /// Print a sales receipt
    pub fn print_receipt(&self, txn: &Transaction, locale: &str) -> Result<()> {
        let commands = self.build_receipt(txn, locale);
        self.send_to_printer(&commands)?;

        info!(
            "Receipt printed for transaction: {} ({})",
            txn.transaction_number,
            if self.has_printer() {
                "printer"
            } else {
                "console"
            }
        );

        Ok(())
    }

    /// Build receipt ESC/POS commands
    fn build_receipt(&self, txn: &Transaction, locale: &str) -> Vec<u8> {
        let w = self.config.paper_width;
        let is_arabic = locale == "ar";
        let mut esc = EscPos::with_paper_width(w);

        // Initialize and set code page for Arabic if needed
        esc.init();
        if is_arabic {
            esc.code_page(CodePage::Wcp1256);
        }

        // ========== Header ==========
        esc.align_center();
        esc.double_size().bold_on();

        // Store name (Arabic first if Arabic locale, otherwise English)
        if is_arabic {
            if let Some(name_ar) = &self.config.store_name_ar {
                esc.line(name_ar);
            } else {
                esc.line(&self.config.store_name);
            }
        } else {
            esc.line(&self.config.store_name);
        }

        esc.normal().bold_off();

        // Store address
        if !self.config.store_address.is_empty() {
            if is_arabic {
                if let Some(addr_ar) = &self.config.store_address_ar {
                    esc.line(addr_ar);
                } else {
                    esc.line(&self.config.store_address);
                }
            } else {
                esc.line(&self.config.store_address);
            }
        }

        // Store phone
        if let Some(phone) = &self.config.store_phone {
            esc.line(phone);
        }

        // Tax ID
        if let Some(tax_id) = &self.config.store_tax_id {
            let label = if is_arabic { "رقم الضريبي" } else { "Tax ID" };
            esc.line(&format!("{}: {}", label, tax_id));
        }

        esc.newline();

        // ========== Transaction Info ==========
        esc.align_left();
        esc.hr(w);

        // Receipt number
        let receipt_label = if is_arabic { "رقم الفاتورة:" } else { "Receipt No:" };
        let receipt_num = txn.receipt_number.as_deref().unwrap_or(&txn.transaction_number);
        esc.two_col(receipt_label, receipt_num, w);

        // Date/Time
        let date_label = if is_arabic { "التاريخ:" } else { "Date:" };
        let date_str = txn.created_at.format("%Y-%m-%d %H:%M").to_string();
        esc.two_col(date_label, &date_str, w);

        // Cashier
        let cashier_label = if is_arabic { "الكاشير:" } else { "Cashier:" };
        esc.two_col(cashier_label, &txn.operator_name, w);

        // Terminal
        let terminal_label = if is_arabic { "الجهاز:" } else { "Terminal:" };
        esc.two_col(terminal_label, &txn.terminal_id, w);

        // Customer (if any)
        if let Some(customer_name) = &txn.customer_name {
            let customer_label = if is_arabic { "العميل:" } else { "Customer:" };
            esc.two_col(customer_label, customer_name, w);
        }

        esc.hr(w);

        // ========== Items ==========
        for item in &txn.items {
            // Product name — with type prefix for services/fees (Phase 3 Track H)
            let product_name = if is_arabic {
                item.product_name_ar
                    .as_deref()
                    .unwrap_or(&item.product_name)
            } else {
                &item.product_name
            };
            let type_prefix = match item.product_type.as_str() {
                "SERVICE" => if is_arabic { "[خدمة] " } else { "[SVC] " },
                "FEE" => if is_arabic { "[رسم] " } else { "[FEE] " },
                "LABOR" => if is_arabic { "[عمالة] " } else { "[LBR] " },
                "DIGITAL" => if is_arabic { "[رقمي] " } else { "[DIG] " },
                "SUBSCRIPTION" => if is_arabic { "[اشتراك] " } else { "[SUB] " },
                _ => "", // PHYSICAL_GOOD, CONSUMABLE, BUNDLE — no prefix
            };
            if type_prefix.is_empty() {
                esc.line(product_name);
            } else {
                esc.line(&format!("{}{}", type_prefix, product_name));
            }

            // Quantity x Unit Price = Line Total
            let qty_str = if item.quantity.fract().is_zero() {
                format!("{}", item.quantity.to_i32().unwrap_or(0))
            } else {
                format!("{}", item.quantity.round_dp(3))
            };
            let qty_price = format!("  {} x {}", qty_str, item.unit_price.round_dp(3));
            let line_total = format!("{}", item.line_total.round_dp(3));
            esc.two_col(&qty_price, &line_total, w);

            // Show discount if any
            if item.discount_amount > Decimal::ZERO {
                let discount_label = if is_arabic { "    خصم" } else { "    Discount" };
                let discount_str = format!("-{}", item.discount_amount.round_dp(3));
                esc.two_col(discount_label, &discount_str, w);
            }
        }

        esc.hr(w);

        // ========== Totals ==========
        // Subtotal
        let subtotal_label = if is_arabic { "المجموع الفرعي:" } else { "Subtotal:" };
        esc.two_col(subtotal_label, &format!("{}", txn.subtotal.round_dp(3)), w);

        // Discount (if any)
        if txn.discount_total > Decimal::ZERO {
            let discount_label = if is_arabic { "الخصم:" } else { "Discount:" };
            esc.two_col(discount_label, &format!("-{}", txn.discount_total.round_dp(3)), w);
        }

        // Tax
        let tax_label = if is_arabic { "الضريبة:" } else { "Tax:" };
        esc.two_col(tax_label, &format!("{}", txn.tax_total.round_dp(3)), w);

        // Grand Total (bold, larger)
        esc.bold_on();
        let total_label = if is_arabic { "الإجمالي:" } else { "TOTAL:" };
        esc.two_col(total_label, &format!("{} {}", txn.currency, txn.grand_total.round_dp(3)), w);
        esc.bold_off();

        esc.newline();

        // ========== Payments ==========
        let payments_label = if is_arabic { "المدفوعات:" } else { "Payments:" };
        esc.line(payments_label);

        for payment in &txn.payments {
            let method_name = payment.method.display_name(locale);
            let method_str = format!("  {}", method_name);

            // Show card details if card payment
            if payment.method == PaymentMethod::Card {
                if let (Some(card_type), Some(last_four)) =
                    (&payment.card_type, &payment.card_last_four)
                {
                    let card_info = format!("  {} *{}", card_type, last_four);
                    esc.two_col(&card_info, &format!("{}", payment.amount.round_dp(3)), w);
                } else {
                    esc.two_col(&method_str, &format!("{}", payment.amount.round_dp(3)), w);
                }
            } else if payment.method == PaymentMethod::Wallet {
                if let Some(wallet_type) = &payment.wallet_type {
                    let wallet_info = format!("  {}", wallet_type);
                    esc.two_col(&wallet_info, &format!("{}", payment.amount.round_dp(3)), w);
                } else {
                    esc.two_col(&method_str, &format!("{}", payment.amount.round_dp(3)), w);
                }
            } else {
                esc.two_col(&method_str, &format!("{}", payment.amount.round_dp(3)), w);
            }
        }

        // Change
        let change = txn.change_due();
        if change > Decimal::ZERO {
            esc.bold_on();
            let change_label = if is_arabic { "الباقي:" } else { "Change:" };
            esc.two_col(change_label, &format!("{}", change.round_dp(3)), w);
            esc.bold_off();
        }

        esc.newline();

        // ========== Footer ==========
        esc.hr(w);
        esc.align_center();

        // Thank you message
        if is_arabic {
            if let Some(footer_ar) = &self.config.footer_text_ar {
                esc.line(footer_ar);
            }
        } else {
            esc.line(&self.config.footer_text);
        }

        esc.newline();

        // Barcode (receipt number)
        if self.config.print_barcode {
            esc.barcode_code128(receipt_num);
            esc.newline();
        }

        // QR Code (optional - could contain receipt URL)
        if self.config.print_qr_code {
            // QR code could contain a receipt verification URL
            let qr_data = format!("RCP:{}", receipt_num);
            esc.qr_code(&qr_data, 5);
            esc.newline();
        }

        // Item count summary
        let item_count = txn.item_count().to_i32().unwrap_or(0);
        let items_text = if is_arabic {
            format!("{} سلعة", item_count)
        } else {
            format!("{} item(s)", item_count)
        };
        esc.line(&items_text);

        // Cut and open drawer
        esc.feed(3);
        esc.cut();
        esc.open_drawer();

        esc.build()
    }

    // ============================================
    // X-Report (Shift Summary)
    // ============================================

    /// Print X-Report (shift summary - non-clearing)
    pub fn print_x_report(&self, report: &ShiftReport, locale: &str) -> Result<()> {
        let commands = self.build_x_report(report, locale);
        self.send_to_printer(&commands)?;

        info!(
            "X-Report printed for shift: {} ({})",
            report.shift_number,
            if self.has_printer() {
                "printer"
            } else {
                "console"
            }
        );

        Ok(())
    }

    /// Build X-Report ESC/POS commands
    fn build_x_report(&self, report: &ShiftReport, locale: &str) -> Vec<u8> {
        let w = self.config.paper_width;
        let is_arabic = locale == "ar";
        let mut esc = EscPos::with_paper_width(w);

        esc.init();
        if is_arabic {
            esc.code_page(CodePage::Wcp1256);
        }

        // Header
        esc.align_center();
        esc.double_size().bold_on();
        let title = if is_arabic { "تقرير X" } else { "X REPORT" };
        esc.line(title);
        let subtitle = if is_arabic {
            "ملخص الوردية"
        } else {
            "Shift Summary"
        };
        esc.normal().line(subtitle);
        esc.bold_off();
        esc.newline();

        // Shift info
        esc.align_left();
        esc.hr(w);

        let shift_label = if is_arabic { "الوردية:" } else { "Shift:" };
        esc.two_col(shift_label, &report.shift_number, w);

        let operator_label = if is_arabic { "الموظف:" } else { "Operator:" };
        esc.two_col(operator_label, &report.operator_name, w);

        let terminal_label = if is_arabic { "الجهاز:" } else { "Terminal:" };
        esc.two_col(terminal_label, &report.terminal_id, w);

        let started_label = if is_arabic { "بدأت:" } else { "Started:" };
        esc.two_col(started_label, &report.start_time, w);

        esc.hr(w);

        // Sales summary
        let txn_label = if is_arabic { "المعاملات:" } else { "Transactions:" };
        esc.two_col(txn_label, &report.transaction_count.to_string(), w);

        let gross_label = if is_arabic {
            "إجمالي المبيعات:"
        } else {
            "Gross Sales:"
        };
        esc.two_col(
            gross_label,
            &format!("{} {}", report.currency, report.gross_sales.round_dp(3)),
            w,
        );

        let discount_label = if is_arabic { "الخصومات:" } else { "Discounts:" };
        esc.two_col(
            discount_label,
            &format!("-{}", report.discounts.round_dp(3)),
            w,
        );

        let tax_label = if is_arabic { "الضريبة:" } else { "Tax:" };
        esc.two_col(
            tax_label,
            &format!("{}", report.tax.round_dp(3)),
            w,
        );

        esc.bold_on();
        let net_label = if is_arabic { "صافي المبيعات:" } else { "Net Sales:" };
        esc.two_col(
            net_label,
            &format!("{} {}", report.currency, report.net_sales.round_dp(3)),
            w,
        );
        esc.bold_off();

        esc.hr(w);

        // Payment breakdown
        let by_method_label = if is_arabic {
            "حسب طريقة الدفع:"
        } else {
            "By Payment Method:"
        };
        esc.line(by_method_label);

        let cash_label = if is_arabic { "  نقدي:" } else { "  Cash:" };
        esc.two_col(cash_label, &format!("{}", report.cash_total.round_dp(3)), w);

        let card_label = if is_arabic { "  بطاقة:" } else { "  Card:" };
        esc.two_col(card_label, &format!("{}", report.card_total.round_dp(3)), w);

        let wallet_label = if is_arabic { "  محفظة:" } else { "  Wallet:" };
        esc.two_col(wallet_label, &format!("{}", report.wallet_total.round_dp(3)), w);

        let credit_label = if is_arabic { "  آجل:" } else { "  Credit:" };
        esc.two_col(credit_label, &format!("{}", report.credit_total.round_dp(3)), w);

        esc.hr(w);

        // Cash drawer
        let cash_drawer_label = if is_arabic { "الدرج النقدي:" } else { "Cash Drawer:" };
        esc.line(cash_drawer_label);

        let opening_label = if is_arabic {
            "  رصيد البداية:"
        } else {
            "  Opening Float:"
        };
        esc.two_col(opening_label, &format!("{}", report.opening_float.round_dp(3)), w);

        let expected_label = if is_arabic {
            "  المتوقع:"
        } else {
            "  Expected Cash:"
        };
        esc.two_col(expected_label, &format!("{}", report.expected_cash.round_dp(3)), w);

        // Returns/Voids
        if report.return_count > 0 || report.void_count > 0 {
            esc.hr(w);

            if report.return_count > 0 {
                let returns_label = if is_arabic { "المرتجعات:" } else { "Returns:" };
                esc.two_col(
                    returns_label,
                    &format!("{} ({})", report.return_count, report.return_total.round_dp(3)),
                    w,
                );
            }

            if report.void_count > 0 {
                let voids_label = if is_arabic { "الملغاة:" } else { "Voids:" };
                esc.two_col(voids_label, &report.void_count.to_string(), w);
            }
        }

        // Footer
        esc.newline();
        esc.align_center();
        let non_clearing = if is_arabic {
            "* تقرير غير مقاصص *"
        } else {
            "* Non-Clearing Report *"
        };
        esc.line(non_clearing);

        esc.feed(3);
        esc.cut();

        esc.build()
    }

    // ============================================
    // Z-Report (End of Day)
    // ============================================

    /// Print Z-Report (end of day - clearing report)
    pub fn print_z_report(&self, report: &ShiftReport, locale: &str) -> Result<()> {
        let commands = self.build_z_report(report, locale);
        self.send_to_printer(&commands)?;

        info!(
            "Z-Report printed for shift: {} ({})",
            report.shift_number,
            if self.has_printer() {
                "printer"
            } else {
                "console"
            }
        );

        Ok(())
    }

    /// Build Z-Report ESC/POS commands
    fn build_z_report(&self, report: &ShiftReport, locale: &str) -> Vec<u8> {
        let w = self.config.paper_width;
        let is_arabic = locale == "ar";
        let mut esc = EscPos::with_paper_width(w);

        esc.init();
        if is_arabic {
            esc.code_page(CodePage::Wcp1256);
        }

        // Header (emphasized for Z-report)
        esc.align_center();
        esc.double_size().bold_on();
        let title = if is_arabic {
            "تقرير Z - نهاية اليوم"
        } else {
            "Z REPORT - END OF DAY"
        };
        esc.line(title);
        esc.normal().bold_off();
        esc.newline();

        // Store info
        if is_arabic {
            if let Some(name_ar) = &self.config.store_name_ar {
                esc.line(name_ar);
            }
        } else {
            esc.line(&self.config.store_name);
        }
        esc.newline();

        // Shift info
        esc.align_left();
        esc.hr_double(w);

        let shift_label = if is_arabic { "الوردية:" } else { "Shift:" };
        esc.two_col(shift_label, &report.shift_number, w);

        let operator_label = if is_arabic { "الموظف:" } else { "Operator:" };
        esc.two_col(operator_label, &report.operator_name, w);

        let terminal_label = if is_arabic { "الجهاز:" } else { "Terminal:" };
        esc.two_col(terminal_label, &report.terminal_id, w);

        let started_label = if is_arabic { "بدأت:" } else { "Started:" };
        esc.two_col(started_label, &report.start_time, w);

        if let Some(end_time) = &report.end_time {
            let ended_label = if is_arabic { "انتهت:" } else { "Ended:" };
            esc.two_col(ended_label, end_time, w);
        }

        esc.hr_double(w);

        // Sales summary
        esc.bold_on();
        let sales_header = if is_arabic { "ملخص المبيعات" } else { "SALES SUMMARY" };
        esc.align_center().line(sales_header);
        esc.bold_off();
        esc.align_left();

        let txn_label = if is_arabic { "المعاملات:" } else { "Transactions:" };
        esc.two_col(txn_label, &report.transaction_count.to_string(), w);

        let gross_label = if is_arabic {
            "إجمالي المبيعات:"
        } else {
            "Gross Sales:"
        };
        esc.two_col(gross_label, &format!("{}", report.gross_sales.round_dp(3)), w);

        let discount_label = if is_arabic { "الخصومات:" } else { "Discounts:" };
        esc.two_col(discount_label, &format!("-{}", report.discounts.round_dp(3)), w);

        let tax_label = if is_arabic { "الضريبة:" } else { "Tax:" };
        esc.two_col(tax_label, &format!("{}", report.tax.round_dp(3)), w);

        esc.hr(w);
        esc.bold_on();
        let net_label = if is_arabic { "صافي المبيعات:" } else { "NET SALES:" };
        esc.two_col(
            net_label,
            &format!("{} {}", report.currency, report.net_sales.round_dp(3)),
            w,
        );
        esc.bold_off();

        esc.hr_double(w);

        // Payment breakdown
        esc.bold_on();
        let payment_header = if is_arabic {
            "تفاصيل المدفوعات"
        } else {
            "PAYMENT BREAKDOWN"
        };
        esc.align_center().line(payment_header);
        esc.bold_off();
        esc.align_left();

        let cash_label = if is_arabic { "نقدي:" } else { "Cash:" };
        esc.two_col(cash_label, &format!("{}", report.cash_total.round_dp(3)), w);

        let card_label = if is_arabic { "بطاقة:" } else { "Card:" };
        esc.two_col(card_label, &format!("{}", report.card_total.round_dp(3)), w);

        let wallet_label = if is_arabic { "محفظة:" } else { "Wallet:" };
        esc.two_col(wallet_label, &format!("{}", report.wallet_total.round_dp(3)), w);

        let credit_label = if is_arabic { "آجل:" } else { "Credit:" };
        esc.two_col(credit_label, &format!("{}", report.credit_total.round_dp(3)), w);

        esc.hr_double(w);

        // Cash reconciliation
        esc.bold_on();
        let cash_header = if is_arabic {
            "مطابقة الصندوق"
        } else {
            "CASH RECONCILIATION"
        };
        esc.align_center().line(cash_header);
        esc.bold_off();
        esc.align_left();

        let opening_label = if is_arabic {
            "رصيد البداية:"
        } else {
            "Opening Float:"
        };
        esc.two_col(opening_label, &format!("{}", report.opening_float.round_dp(3)), w);

        let expected_label = if is_arabic { "المتوقع:" } else { "Expected:" };
        esc.two_col(expected_label, &format!("{}", report.expected_cash.round_dp(3)), w);

        if let Some(counted) = report.counted_cash {
            let counted_label = if is_arabic { "المعدود:" } else { "Counted:" };
            esc.two_col(counted_label, &format!("{}", counted.round_dp(3)), w);

            if let Some(variance) = report.cash_variance {
                let variance_label = if is_arabic { "الفرق:" } else { "Variance:" };
                let variance_str = if variance.is_sign_negative() {
                    format!("-{}", variance.abs().round_dp(3))
                } else if variance > Decimal::ZERO {
                    format!("+{}", variance.round_dp(3))
                } else {
                    "0.000".to_string()
                };
                esc.bold_on();
                esc.two_col(variance_label, &variance_str, w);
                esc.bold_off();
            }
        }

        // Returns/Voids
        if report.return_count > 0 || report.void_count > 0 {
            esc.hr_double(w);
            esc.bold_on();
            let adj_header = if is_arabic { "التعديلات" } else { "ADJUSTMENTS" };
            esc.align_center().line(adj_header);
            esc.bold_off();
            esc.align_left();

            if report.return_count > 0 {
                let returns_label = if is_arabic { "المرتجعات:" } else { "Returns:" };
                esc.two_col(
                    returns_label,
                    &format!("{} ({})", report.return_count, report.return_total.round_dp(3)),
                    w,
                );
            }

            if report.void_count > 0 {
                let voids_label = if is_arabic { "الملغاة:" } else { "Voids:" };
                esc.two_col(voids_label, &report.void_count.to_string(), w);
            }
        }

        // Footer
        esc.hr_double(w);
        esc.newline();
        esc.align_center();
        esc.bold_on();
        let clearing = if is_arabic {
            "** تقرير مقاصص **"
        } else {
            "** CLEARING REPORT **"
        };
        esc.line(clearing);
        esc.bold_off();

        let signature_line = if is_arabic {
            "توقيع المشرف: ______________"
        } else {
            "Supervisor Sign: ______________"
        };
        esc.newline();
        esc.line(signature_line);

        esc.feed(4);
        esc.cut();

        esc.build()
    }

    // ============================================
    // Cash Drawer
    // ============================================

    /// Open the cash drawer
    pub fn open_drawer(&self) -> Result<()> {
        let mut esc = EscPos::new();
        esc.init().open_drawer();

        self.send_to_printer(&esc.build())?;
        info!("Cash drawer opened");

        Ok(())
    }

    // ============================================
    // Printer Communication
    // ============================================

    /// Send commands to printer or console
    fn send_to_printer(&self, commands: &[u8]) -> Result<()> {
        if let Some(path) = &self.config.printer_path {
            self.send_to_device(path, commands)
        } else {
            // Console preview mode
            self.print_to_console(commands);
            Ok(())
        }
    }

    /// Send commands to printer device
    fn send_to_device(&self, path: &str, commands: &[u8]) -> Result<()> {
        #[cfg(target_os = "linux")]
        {
            use std::fs::OpenOptions;
            use std::io::Write;

            let mut file = OpenOptions::new()
                .write(true)
                .open(path)
                .with_context(|| format!("Failed to open printer: {}", path))?;

            file.write_all(commands)
                .with_context(|| "Failed to write to printer")?;

            file.flush().with_context(|| "Failed to flush printer")?;
        }

        #[cfg(target_os = "windows")]
        {
            // Windows USB printer support would go here
            // Could use direct USB via hidapi or WinAPI
            warn!("Windows printing not fully implemented, using console output");
            self.print_to_console(commands);
        }

        #[cfg(not(any(target_os = "linux", target_os = "windows")))]
        {
            warn!("Printing not supported on this platform");
            self.print_to_console(commands);
        }

        Ok(())
    }

    /// Print preview to console (for debugging)
    fn print_to_console(&self, commands: &[u8]) {
        println!("\n╔═══════════════════════════════════════════╗");
        println!("║         RECEIPT PREVIEW (Console)         ║");
        println!("╠═══════════════════════════════════════════╣");

        // Filter out control characters and print text
        let mut in_escape = false;
        let mut escape_count = 0;
        let mut output = String::new();

        for &byte in commands {
            if in_escape {
                escape_count -= 1;
                if escape_count == 0 {
                    in_escape = false;
                }
                continue;
            }

            match byte {
                0x1B => {
                    // ESC - start of escape sequence
                    in_escape = true;
                    escape_count = 2; // Most ESC sequences are 2-3 bytes
                }
                0x1D => {
                    // GS - start of GS sequence
                    in_escape = true;
                    escape_count = 3; // Most GS sequences are longer
                }
                0x0A => {
                    // LF - newline
                    println!("║ {:<41} ║", output);
                    output.clear();
                }
                0x20..=0x7E => {
                    // Printable ASCII
                    output.push(byte as char);
                }
                _ => {
                    // Skip other control characters
                }
            }
        }

        // Print any remaining text
        if !output.is_empty() {
            println!("║ {:<41} ║", output);
        }

        println!("╚═══════════════════════════════════════════╝\n");
    }

    /// Test printer connection
    pub fn test_print(&self) -> Result<()> {
        let mut esc = EscPos::new();

        esc.init()
            .align_center()
            .double_size()
            .bold_on()
            .line("TEST PRINT")
            .normal()
            .bold_off()
            .line("Printer test successful!")
            .newline()
            .barcode_code128("TEST123")
            .feed(3)
            .cut();

        self.send_to_printer(&esc.build())?;
        info!("Test print completed");

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pos_models::{Cart, CartItem, Product, ProductUnit, Transaction};
    use rust_decimal::Decimal;

    fn create_test_product() -> Product {
        Product {
            id: "prod-001".to_string(),
            sku: "SKU001".to_string(),
            barcode: Some("1234567890123".to_string()),
            name: "Test Product".to_string(),
            name_ar: Some("منتج تجريبي".to_string()),
            price: Decimal::from(10),
            tax_rate: Decimal::from(15),
            tax_inclusive: false,
            unit: ProductUnit::Unit,
            ..Default::default()
        }
    }

    fn create_test_cart() -> Cart {
        let product = create_test_product();
        let mut cart = Cart::new();
        cart.items.push(CartItem::new(&product, Decimal::from(2)));
        cart.recalculate();
        cart
    }

    fn create_test_transaction() -> Transaction {
        let cart = create_test_cart();
        let mut txn =
            Transaction::from_cart(&cart, "LYD", "shift-1", "TERM-001", "op-1", "Ahmed Mohamed");
        txn.add_cash_payment(Decimal::from(30));
        txn.set_receipt_number("RCP-001");
        txn
    }

    fn create_test_shift_report() -> ShiftReport {
        ShiftReport {
            shift_number: "SH-2024-001".to_string(),
            operator_name: "Ahmed Mohamed".to_string(),
            terminal_id: "TERM-001".to_string(),
            start_time: "2024-12-12 08:00".to_string(),
            end_time: Some("2024-12-12 16:00".to_string()),
            transaction_count: 45,
            gross_sales: Decimal::from(1500),
            discounts: Decimal::from(50),
            tax: Decimal::new(2175, 1),
            net_sales: Decimal::new(16675, 1),
            cash_total: Decimal::from(800),
            card_total: Decimal::from(600),
            wallet_total: Decimal::from(200),
            credit_total: Decimal::new(675, 1),
            opening_float: Decimal::from(200),
            expected_cash: Decimal::from(1000),
            counted_cash: Some(Decimal::from(995)),
            cash_variance: Some(Decimal::from(-5)),
            return_count: 2,
            return_total: Decimal::from(35),
            void_count: 1,
            currency: "LYD".to_string(),
        }
    }

    #[test]
    fn test_printer_config_default() {
        let config = PrinterConfig::default();
        assert!(config.printer_path.is_none());
        assert_eq!(config.paper_width, 42);
        assert_eq!(config.store_name, "E2Manage POS");
    }

    #[test]
    fn test_print_service_new() {
        let config = PrinterConfig::default();
        let service = PrintService::new(config);
        assert!(!service.has_printer());
    }

    #[test]
    fn test_print_service_console_only() {
        let service = PrintService::console_only();
        assert!(!service.has_printer());
    }

    #[test]
    fn test_print_service_set_printer_path() {
        let mut service = PrintService::console_only();
        assert!(!service.has_printer());

        service.set_printer_path(Some("/dev/usb/lp0".to_string()));
        assert!(service.has_printer());

        service.set_printer_path(None);
        assert!(!service.has_printer());
    }

    #[test]
    fn test_build_receipt_english() {
        let config = PrinterConfig {
            store_name: "Test Store".to_string(),
            store_address: "123 Test St".to_string(),
            ..Default::default()
        };
        let service = PrintService::new(config);
        let txn = create_test_transaction();

        let commands = service.build_receipt(&txn, "en");
        assert!(!commands.is_empty());

        // Check that commands contain expected text
        let text = String::from_utf8_lossy(&commands);
        assert!(text.contains("Test Store"));
        assert!(text.contains("Receipt No:"));
        assert!(text.contains("Cashier:"));
    }

    #[test]
    fn test_build_receipt_arabic() {
        let config = PrinterConfig {
            store_name: "Test Store".to_string(),
            store_name_ar: Some("متجر التجربة".to_string()),
            ..Default::default()
        };
        let service = PrintService::new(config);
        let txn = create_test_transaction();

        let commands = service.build_receipt(&txn, "ar");
        assert!(!commands.is_empty());

        // Arabic code page should be set
        assert!(commands.contains(&23)); // Code page 1256 for Arabic
    }

    #[test]
    fn test_build_x_report() {
        let service = PrintService::console_only();
        let report = create_test_shift_report();

        let commands = service.build_x_report(&report, "en");
        assert!(!commands.is_empty());

        let text = String::from_utf8_lossy(&commands);
        assert!(text.contains("X REPORT"));
        assert!(text.contains("Non-Clearing Report"));
    }

    #[test]
    fn test_build_z_report() {
        let service = PrintService::console_only();
        let report = create_test_shift_report();

        let commands = service.build_z_report(&report, "en");
        assert!(!commands.is_empty());

        let text = String::from_utf8_lossy(&commands);
        assert!(text.contains("Z REPORT"));
        assert!(text.contains("CLEARING REPORT"));
    }

    #[test]
    fn test_print_receipt_console() {
        let service = PrintService::console_only();
        let txn = create_test_transaction();

        // Should not fail in console mode
        let result = service.print_receipt(&txn, "en");
        assert!(result.is_ok());
    }

    #[test]
    fn test_print_x_report_console() {
        let service = PrintService::console_only();
        let report = create_test_shift_report();

        let result = service.print_x_report(&report, "en");
        assert!(result.is_ok());
    }

    #[test]
    fn test_print_z_report_console() {
        let service = PrintService::console_only();
        let report = create_test_shift_report();

        let result = service.print_z_report(&report, "en");
        assert!(result.is_ok());
    }

    #[test]
    fn test_open_drawer_console() {
        let service = PrintService::console_only();

        let result = service.open_drawer();
        assert!(result.is_ok());
    }

    #[test]
    fn test_test_print_console() {
        let service = PrintService::console_only();

        let result = service.test_print();
        assert!(result.is_ok());
    }

    #[test]
    fn test_shift_report_default() {
        let report = ShiftReport::default();
        assert!(report.shift_number.is_empty());
        assert_eq!(report.transaction_count, 0);
        assert_eq!(report.gross_sales, Decimal::ZERO);
    }

    #[test]
    fn test_receipt_with_customer() {
        let cart = create_test_cart();
        let mut txn =
            Transaction::from_cart(&cart, "LYD", "shift-1", "TERM-001", "op-1", "Ahmed");
        txn.customer_id = Some("cust-001".to_string());
        txn.customer_name = Some("John Doe".to_string());
        txn.add_cash_payment(Decimal::from(30));

        let service = PrintService::console_only();
        let commands = service.build_receipt(&txn, "en");

        let text = String::from_utf8_lossy(&commands);
        assert!(text.contains("Customer:"));
        assert!(text.contains("John Doe"));
    }

    #[test]
    fn test_receipt_with_card_payment() {
        let cart = create_test_cart();
        let mut txn =
            Transaction::from_cart(&cart, "LYD", "shift-1", "TERM-001", "op-1", "Ahmed");
        txn.add_card_payment(Decimal::from(23), "1234", "VISA", "AUTH123");

        let service = PrintService::console_only();
        let commands = service.build_receipt(&txn, "en");

        let text = String::from_utf8_lossy(&commands);
        assert!(text.contains("VISA"));
        assert!(text.contains("*1234"));
    }

    #[test]
    fn test_receipt_with_discount() {
        let product = create_test_product();
        let mut cart = Cart::new();
        let mut item = CartItem::new(&product, Decimal::from(2));
        item.apply_discount_percent(Decimal::from(10));
        cart.items.push(item);
        cart.recalculate();

        let mut txn =
            Transaction::from_cart(&cart, "LYD", "shift-1", "TERM-001", "op-1", "Ahmed");
        txn.add_cash_payment(Decimal::from(30));

        let service = PrintService::console_only();
        let commands = service.build_receipt(&txn, "en");

        let text = String::from_utf8_lossy(&commands);
        assert!(text.contains("Discount"));
    }

    #[test]
    fn test_receipt_barcode_disabled() {
        let config = PrinterConfig {
            print_barcode: false,
            ..Default::default()
        };
        let service = PrintService::new(config);
        let txn = create_test_transaction();

        let commands = service.build_receipt(&txn, "en");

        // Should not contain barcode commands (GS k)
        let has_barcode = commands.windows(2).any(|w| w == [0x1D, 0x6B]);
        assert!(!has_barcode);
    }

    #[test]
    fn test_receipt_qr_enabled() {
        let config = PrinterConfig {
            print_qr_code: true,
            ..Default::default()
        };
        let service = PrintService::new(config);
        let txn = create_test_transaction();

        let commands = service.build_receipt(&txn, "en");

        // Should contain QR code commands (GS ( k)
        let has_qr = commands.windows(3).any(|w| w == [0x1D, 0x28, 0x6B]);
        assert!(has_qr);
    }
}
