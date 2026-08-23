//! Database Schema Definitions
//!
//! Contains all table definitions for the POS terminal local database.
//! Includes FTS5 virtual table for fast product search.

/// Version 1 Schema - Initial tables
pub const SCHEMA_V1: &str = r#"
-- ============================================================================
-- TERMINAL CONFIGURATION
-- ============================================================================

-- Terminal configuration and authentication
CREATE TABLE IF NOT EXISTS terminal_config (
    id INTEGER PRIMARY KEY CHECK (id = 1),  -- Ensure only one row
    terminal_id TEXT NOT NULL,
    terminal_code TEXT NOT NULL,
    hardware_id TEXT NOT NULL,
    session_token TEXT,
    company_id TEXT,
    branch_id TEXT,
    locale TEXT DEFAULT 'ar',
    currency TEXT DEFAULT 'LYD',
    sector TEXT DEFAULT 'RETAIL',
    created_at TEXT DEFAULT (datetime('now')),
    updated_at TEXT DEFAULT (datetime('now'))
);

-- ============================================================================
-- SYNC STATE TRACKING
-- ============================================================================

-- Track sync state for each resource type
CREATE TABLE IF NOT EXISTS sync_state (
    resource TEXT PRIMARY KEY,
    etag TEXT,
    version TEXT,
    last_sync TEXT,
    record_count INTEGER DEFAULT 0
);

-- ============================================================================
-- OPERATORS (CASHIERS)
-- ============================================================================

-- Operators who can use this terminal
CREATE TABLE IF NOT EXISTS operators (
    id TEXT PRIMARY KEY,
    code TEXT NOT NULL,
    name TEXT NOT NULL,
    name_ar TEXT,
    pin_hash TEXT NOT NULL,
    role TEXT DEFAULT 'CASHIER',
    avatar_url TEXT,
    permissions_json TEXT,
    is_active INTEGER DEFAULT 1,
    updated_at TEXT DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_operators_code ON operators(code);
CREATE INDEX IF NOT EXISTS idx_operators_active ON operators(is_active);

-- ============================================================================
-- CATEGORIES
-- ============================================================================

-- Product categories cache
CREATE TABLE IF NOT EXISTS categories (
    id TEXT PRIMARY KEY,
    parent_id TEXT,
    name TEXT NOT NULL,
    name_ar TEXT,
    color TEXT,
    icon TEXT,
    image_url TEXT,
    display_order INTEGER DEFAULT 0,
    is_active INTEGER DEFAULT 1,
    updated_at TEXT DEFAULT (datetime('now')),
    FOREIGN KEY (parent_id) REFERENCES categories(id)
);

CREATE INDEX IF NOT EXISTS idx_categories_parent ON categories(parent_id);
CREATE INDEX IF NOT EXISTS idx_categories_order ON categories(display_order);
CREATE INDEX IF NOT EXISTS idx_categories_active ON categories(is_active);

-- ============================================================================
-- PRODUCTS
-- ============================================================================

-- Products cache
CREATE TABLE IF NOT EXISTS products (
    id TEXT PRIMARY KEY,
    sku TEXT NOT NULL,
    barcode TEXT,
    name TEXT NOT NULL,
    name_ar TEXT,
    description TEXT,
    description_ar TEXT,
    price REAL NOT NULL,
    cost REAL DEFAULT 0,
    tax_rate REAL DEFAULT 0,
    tax_inclusive INTEGER DEFAULT 0,
    category_id TEXT,
    category_name TEXT,
    unit TEXT DEFAULT 'UNIT',
    stock_qty INTEGER DEFAULT 0,
    min_stock INTEGER DEFAULT 0,
    allow_negative_stock INTEGER DEFAULT 0,
    image_url TEXT,
    is_weighable INTEGER DEFAULT 0,
    is_serialized INTEGER DEFAULT 0,
    is_active INTEGER DEFAULT 1,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (category_id) REFERENCES categories(id)
);

CREATE INDEX IF NOT EXISTS idx_products_sku ON products(sku);
CREATE INDEX IF NOT EXISTS idx_products_barcode ON products(barcode);
CREATE INDEX IF NOT EXISTS idx_products_category ON products(category_id);
CREATE INDEX IF NOT EXISTS idx_products_active ON products(is_active);

-- ============================================================================
-- PRODUCT FULL-TEXT SEARCH (FTS5)
-- ============================================================================

-- FTS5 virtual table for fast product search
CREATE VIRTUAL TABLE IF NOT EXISTS products_fts USING fts5(
    id,
    sku,
    barcode,
    name,
    name_ar,
    content='products',
    content_rowid='rowid'
);

-- Triggers to keep FTS in sync with products table

-- Insert trigger
CREATE TRIGGER IF NOT EXISTS products_fts_insert AFTER INSERT ON products BEGIN
    INSERT INTO products_fts(rowid, id, sku, barcode, name, name_ar)
    VALUES (NEW.rowid, NEW.id, NEW.sku, NEW.barcode, NEW.name, NEW.name_ar);
END;

-- Delete trigger
CREATE TRIGGER IF NOT EXISTS products_fts_delete AFTER DELETE ON products BEGIN
    INSERT INTO products_fts(products_fts, rowid, id, sku, barcode, name, name_ar)
    VALUES ('delete', OLD.rowid, OLD.id, OLD.sku, OLD.barcode, OLD.name, OLD.name_ar);
END;

-- Update trigger
CREATE TRIGGER IF NOT EXISTS products_fts_update AFTER UPDATE ON products BEGIN
    INSERT INTO products_fts(products_fts, rowid, id, sku, barcode, name, name_ar)
    VALUES ('delete', OLD.rowid, OLD.id, OLD.sku, OLD.barcode, OLD.name, OLD.name_ar);
    INSERT INTO products_fts(rowid, id, sku, barcode, name, name_ar)
    VALUES (NEW.rowid, NEW.id, NEW.sku, NEW.barcode, NEW.name, NEW.name_ar);
END;

-- ============================================================================
-- SHIFTS
-- ============================================================================

-- Cashier shifts
CREATE TABLE IF NOT EXISTS shifts (
    id TEXT PRIMARY KEY,
    shift_number TEXT NOT NULL,
    operator_id TEXT NOT NULL,
    terminal_id TEXT,
    opening_cash REAL NOT NULL DEFAULT 0,
    closing_cash REAL,
    expected_cash REAL,
    variance REAL,
    started_at TEXT NOT NULL,
    ended_at TEXT,
    status TEXT DEFAULT 'ACTIVE',  -- ACTIVE, CLOSED, SUSPENDED
    sync_status TEXT DEFAULT 'PENDING',  -- PENDING, SYNCING, SYNCED, FAILED
    server_id TEXT,
    notes TEXT,
    FOREIGN KEY (operator_id) REFERENCES operators(id)
);

CREATE INDEX IF NOT EXISTS idx_shifts_operator ON shifts(operator_id);
CREATE INDEX IF NOT EXISTS idx_shifts_status ON shifts(status);
CREATE INDEX IF NOT EXISTS idx_shifts_sync ON shifts(sync_status);

-- ============================================================================
-- OFFLINE TRANSACTIONS
-- ============================================================================

-- Transactions created offline, pending sync
CREATE TABLE IF NOT EXISTS offline_transactions (
    offline_id TEXT PRIMARY KEY,
    transaction_number TEXT,
    transaction_type TEXT NOT NULL,  -- SALE, RETURN, EXCHANGE, VOID
    items_json TEXT NOT NULL,
    payments_json TEXT NOT NULL,
    subtotal REAL NOT NULL,
    tax_total REAL NOT NULL,
    discount_total REAL DEFAULT 0,
    grand_total REAL NOT NULL,
    customer_id TEXT,
    customer_name TEXT,
    shift_id TEXT,
    operator_id TEXT,
    terminal_id TEXT,
    receipt_number TEXT,
    notes TEXT,
    created_at TEXT NOT NULL,
    sync_status TEXT DEFAULT 'PENDING',  -- PENDING, SYNCING, SYNCED, FAILED, CONFLICT
    server_id TEXT,
    retry_count INTEGER DEFAULT 0,
    last_error TEXT,
    last_retry_at TEXT,
    FOREIGN KEY (shift_id) REFERENCES shifts(id),
    FOREIGN KEY (operator_id) REFERENCES operators(id)
);

CREATE INDEX IF NOT EXISTS idx_txn_sync_status ON offline_transactions(sync_status);
CREATE INDEX IF NOT EXISTS idx_txn_created ON offline_transactions(created_at);
CREATE INDEX IF NOT EXISTS idx_txn_shift ON offline_transactions(shift_id);
CREATE INDEX IF NOT EXISTS idx_txn_operator ON offline_transactions(operator_id);

-- ============================================================================
-- DRAFTS (HELD ORDERS)
-- ============================================================================

-- Held/draft orders that can be recalled
CREATE TABLE IF NOT EXISTS drafts (
    id TEXT PRIMARY KEY,
    name TEXT,
    items_json TEXT NOT NULL,
    customer_id TEXT,
    customer_name TEXT,
    discount_json TEXT,
    notes TEXT,
    created_at TEXT NOT NULL,
    expires_at TEXT,
    operator_id TEXT,
    shift_id TEXT,
    FOREIGN KEY (operator_id) REFERENCES operators(id)
);

CREATE INDEX IF NOT EXISTS idx_drafts_operator ON drafts(operator_id);
CREATE INDEX IF NOT EXISTS idx_drafts_expires ON drafts(expires_at);

-- ============================================================================
-- CUSTOMERS (CACHED)
-- ============================================================================

-- Cached customer data for offline lookup
CREATE TABLE IF NOT EXISTS customers (
    id TEXT PRIMARY KEY,
    code TEXT,
    name TEXT NOT NULL,
    name_ar TEXT,
    phone TEXT,
    email TEXT,
    tax_number TEXT,
    credit_limit REAL DEFAULT 0,
    current_balance REAL DEFAULT 0,
    price_list_id TEXT,
    discount_percent REAL DEFAULT 0,
    is_active INTEGER DEFAULT 1,
    updated_at TEXT DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_customers_code ON customers(code);
CREATE INDEX IF NOT EXISTS idx_customers_phone ON customers(phone);
CREATE INDEX IF NOT EXISTS idx_customers_active ON customers(is_active);

-- ============================================================================
-- PAYMENT METHODS
-- ============================================================================

-- Available payment methods for this terminal
CREATE TABLE IF NOT EXISTS payment_methods (
    id TEXT PRIMARY KEY,
    code TEXT NOT NULL,
    name TEXT NOT NULL,
    name_ar TEXT,
    method_type TEXT NOT NULL,  -- CASH, CARD, WALLET, CREDIT, CHECK
    is_enabled INTEGER DEFAULT 1,
    opens_drawer INTEGER DEFAULT 0,
    requires_reference INTEGER DEFAULT 0,
    display_order INTEGER DEFAULT 0,
    icon TEXT,
    config_json TEXT
);

CREATE INDEX IF NOT EXISTS idx_payment_methods_enabled ON payment_methods(is_enabled);
CREATE INDEX IF NOT EXISTS idx_payment_methods_order ON payment_methods(display_order);

-- ============================================================================
-- SCREEN DEFINITIONS (JSON-DRIVEN UI)
-- ============================================================================

-- Dynamic screen definitions from server
CREATE TABLE IF NOT EXISTS screens (
    screen_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    version INTEGER DEFAULT 1,
    definition_json TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- ============================================================================
-- SETTINGS
-- ============================================================================

-- Local settings storage
CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT DEFAULT (datetime('now'))
);

-- ============================================================================
-- PRINT QUEUE
-- ============================================================================

-- Pending print jobs
CREATE TABLE IF NOT EXISTS print_queue (
    id TEXT PRIMARY KEY,
    printer_type TEXT NOT NULL,  -- RECEIPT, KITCHEN, LABEL
    content_type TEXT NOT NULL,  -- ESCPOS, TEXT, HTML
    content BLOB NOT NULL,
    status TEXT DEFAULT 'PENDING',  -- PENDING, PRINTING, PRINTED, FAILED
    created_at TEXT NOT NULL,
    printed_at TEXT,
    retry_count INTEGER DEFAULT 0,
    error TEXT
);

CREATE INDEX IF NOT EXISTS idx_print_queue_status ON print_queue(status);
"#;

/// Version 2 Schema - Z-Reports and day closures
pub const SCHEMA_V2: &str = r#"
-- ============================================================================
-- Z-REPORTS (END OF DAY)
-- ============================================================================

-- Z-Report (end of day clearing report)
CREATE TABLE IF NOT EXISTS z_reports (
    report_number TEXT PRIMARY KEY,
    report_date TEXT NOT NULL,
    terminal_id TEXT NOT NULL,
    currency TEXT DEFAULT 'LYD',
    total_shifts INTEGER NOT NULL DEFAULT 0,
    total_transactions INTEGER NOT NULL DEFAULT 0,
    gross_sales REAL NOT NULL DEFAULT 0,
    discounts REAL NOT NULL DEFAULT 0,
    returns REAL NOT NULL DEFAULT 0,
    net_sales REAL NOT NULL DEFAULT 0,
    tax_collected REAL NOT NULL DEFAULT 0,
    cash_total REAL NOT NULL DEFAULT 0,
    card_total REAL NOT NULL DEFAULT 0,
    wallet_total REAL NOT NULL DEFAULT 0,
    credit_total REAL NOT NULL DEFAULT 0,
    opening_float REAL NOT NULL DEFAULT 0,
    expected_cash REAL NOT NULL DEFAULT 0,
    actual_cash REAL NOT NULL DEFAULT 0,
    variance REAL NOT NULL DEFAULT 0,
    variance_status TEXT DEFAULT 'balanced',
    generated_at TEXT NOT NULL,
    synced INTEGER DEFAULT 0,
    server_id TEXT
);

CREATE INDEX IF NOT EXISTS idx_z_reports_date ON z_reports(report_date);
CREATE INDEX IF NOT EXISTS idx_z_reports_terminal ON z_reports(terminal_id);
CREATE INDEX IF NOT EXISTS idx_z_reports_synced ON z_reports(synced);

-- ============================================================================
-- DAY CLOSURES TRACKING
-- ============================================================================

-- Track closed days to prevent duplicate Z-Reports
CREATE TABLE IF NOT EXISTS day_closures (
    terminal_id TEXT NOT NULL,
    date TEXT NOT NULL,
    closed_at TEXT NOT NULL,
    PRIMARY KEY (terminal_id, date)
);

CREATE INDEX IF NOT EXISTS idx_day_closures_date ON day_closures(date);

-- ============================================================================
-- ADD FIELDS TO OFFLINE_TRANSACTIONS FOR Z-REPORT AGGREGATION
-- ============================================================================

-- Add type/status/payment_method columns if they don't exist (for aggregation queries)
-- Note: SQLite doesn't support ALTER TABLE ADD IF NOT EXISTS, so we use error suppression
-- These columns may already exist from V1, this is a safety measure
"#;

/// Version 3 Schema - Terminal registration/pairing support
pub const SCHEMA_V3: &str = r#"
-- ============================================================================
-- TERMINAL REGISTRATION (FOR PAIRING)
-- ============================================================================

-- Store terminal registration credentials separately from session
-- The secret is needed for re-authentication after app restart
CREATE TABLE IF NOT EXISTS terminal_registration (
    id INTEGER PRIMARY KEY CHECK (id = 1),  -- Ensure only one row
    hardware_id TEXT NOT NULL,
    terminal_id TEXT,
    terminal_code TEXT,
    secret TEXT,                           -- Terminal secret for login
    company_name TEXT,                     -- Company name for display
    registered_at TEXT,
    is_registered INTEGER DEFAULT 0        -- 0 = not registered, 1 = registered
);

-- Insert default empty registration
INSERT OR IGNORE INTO terminal_registration (id, hardware_id, is_registered)
VALUES (1, '', 0);
"#;

/// Version 4 Schema - Feature library for dynamic screen enablement
pub const SCHEMA_V4: &str = r#"
-- ============================================================================
-- FEATURES (FEATURE LIBRARY)
-- ============================================================================

-- Features that can be enabled/disabled per terminal
CREATE TABLE IF NOT EXISTS features (
    feature_id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    name_ar TEXT,
    config_key TEXT,
    is_core INTEGER DEFAULT 0,
    is_enabled INTEGER DEFAULT 1,
    icon TEXT,
    display_order INTEGER DEFAULT 100,
    updated_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_features_enabled ON features(is_enabled);
CREATE INDEX IF NOT EXISTS idx_features_order ON features(display_order);

-- ============================================================================
-- FEATURE SCREENS
-- ============================================================================

-- Screens that belong to each feature
CREATE TABLE IF NOT EXISTS feature_screens (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    feature_id TEXT NOT NULL,
    screen_id TEXT NOT NULL,
    name TEXT NOT NULL,
    name_ar TEXT,
    is_entry_point INTEGER DEFAULT 0,
    next_screen TEXT,
    display_order INTEGER DEFAULT 100,
    FOREIGN KEY (feature_id) REFERENCES features(feature_id) ON DELETE CASCADE,
    UNIQUE(feature_id, screen_id)
);

CREATE INDEX IF NOT EXISTS idx_feature_screens_feature ON feature_screens(feature_id);
CREATE INDEX IF NOT EXISTS idx_feature_screens_screen_id ON feature_screens(screen_id);
CREATE INDEX IF NOT EXISTS idx_feature_screens_entry ON feature_screens(is_entry_point);
"#;

/// Version 5 Schema - No-op (company_name already added in V3)
pub const SCHEMA_V5: &str = r#"
-- ============================================================================
-- TERMINAL REGISTRATION - COMPANY NAME (already in V3, kept for version tracking)
-- ============================================================================
SELECT 1;
"#;

/// Version 6 Schema - Active cart persistence for crash recovery
pub const SCHEMA_V6: &str = r#"
-- ============================================================================
-- ACTIVE CART (CRASH RECOVERY)
-- ============================================================================

-- Persists the current active cart to prevent data loss on app crash.
-- Only one active cart per terminal (id=1 constraint).
CREATE TABLE IF NOT EXISTS active_cart (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    operator_id TEXT,
    cart_json TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
"#;

/// Version 7 Schema - Price version tracking for offline transactions
///
/// Tracks the catalog version (ETag) when transactions are created offline.
/// This allows the backend to detect and flag transactions made with stale prices.
pub const SCHEMA_V7: &str = r#"
-- ============================================================================
-- OFFLINE TRANSACTIONS - PRICE VERSION TRACKING
-- ============================================================================

-- Add catalog version tracking to offline transactions.
-- catalog_etag: The ETag of the catalog at the time of transaction creation
-- This helps track transactions made with potentially outdated prices.
ALTER TABLE offline_transactions ADD COLUMN catalog_etag TEXT;
"#;

/// Version 8 Schema - Platform license key for platform registry
///
/// Stores the license key assigned by the platform registry for device
/// monitoring and management.
pub const SCHEMA_V8: &str = r#"
-- ============================================================================
-- TERMINAL REGISTRATION - PLATFORM LICENSE KEY
-- ============================================================================

-- Add license_key to terminal_registration for platform registry integration.
-- This key is assigned when the device is registered with the platform.
ALTER TABLE terminal_registration ADD COLUMN license_key TEXT;
"#;

/// Version 9 Schema - Shared drafts for cloud-synced draft support
///
/// Enables drafts to be shared across terminals in the same company/warehouse.
/// When a POS terminal has issues, customers can continue their transaction
/// on another terminal by entering the draft token.
pub const SCHEMA_V9: &str = r#"
-- ============================================================================
-- SHARED DRAFTS (CLOUD CACHE)
-- ============================================================================

-- Cache for shared drafts fetched from backend
-- These are drafts that can be accessed by any terminal in the same warehouse
CREATE TABLE IF NOT EXISTS shared_drafts (
    id TEXT PRIMARY KEY,              -- Backend cart UUID
    token TEXT UNIQUE NOT NULL,       -- 6-char token (e.g., "A1B2C3")
    name TEXT,
    items_json TEXT NOT NULL,
    customer_id TEXT,
    customer_name TEXT,
    discount_json TEXT,
    notes TEXT,
    item_count INTEGER DEFAULT 0,
    total_amount REAL DEFAULT 0,
    currency TEXT DEFAULT 'LYD',
    warehouse_id TEXT NOT NULL,
    device_id TEXT,
    operator_id TEXT,
    operator_name TEXT,
    created_at TEXT NOT NULL,
    expires_at TEXT,
    fetched_at TEXT NOT NULL,         -- When we last fetched from server
    sync_status TEXT DEFAULT 'SYNCED' -- SYNCED, PENDING_CONVERT, PENDING_DELETE
);

CREATE INDEX IF NOT EXISTS idx_shared_drafts_token ON shared_drafts(token);
CREATE INDEX IF NOT EXISTS idx_shared_drafts_warehouse ON shared_drafts(warehouse_id);
CREATE INDEX IF NOT EXISTS idx_shared_drafts_status ON shared_drafts(sync_status);

-- ============================================================================
-- DRAFT SYNC QUEUE (OFFLINE SUPPORT)
-- ============================================================================

-- Queue for draft operations that need to be synced to backend
-- Used when terminal is offline - operations are queued here and synced later
CREATE TABLE IF NOT EXISTS draft_sync_queue (
    id TEXT PRIMARY KEY,
    local_draft_id TEXT NOT NULL,     -- Reference to local drafts table
    operation TEXT NOT NULL,          -- CREATE, CONVERT, DELETE
    payload_json TEXT NOT NULL,       -- Serialized request data
    server_id TEXT,                   -- Backend cart ID (set after sync)
    server_token TEXT,                -- Backend token (set after sync)
    transaction_id TEXT,              -- For CONVERT operation
    sync_status TEXT DEFAULT 'PENDING', -- PENDING, SYNCING, SYNCED, FAILED
    retry_count INTEGER DEFAULT 0,
    last_error TEXT,
    created_at TEXT NOT NULL,
    last_attempt_at TEXT
);

CREATE INDEX IF NOT EXISTS idx_draft_sync_queue_status ON draft_sync_queue(sync_status);
CREATE INDEX IF NOT EXISTS idx_draft_sync_queue_local_draft ON draft_sync_queue(local_draft_id);
"#;

/// Version 10 Schema - HR Employee integration for operators
pub const SCHEMA_V10: &str = r#"
-- ============================================================================
-- HR EMPLOYEE INTEGRATION FOR OPERATORS
-- ============================================================================

-- Add HR Employee fields to operators table
ALTER TABLE operators ADD COLUMN employee_id TEXT;
ALTER TABLE operators ADD COLUMN employee_number TEXT;
ALTER TABLE operators ADD COLUMN department TEXT;
ALTER TABLE operators ADD COLUMN position TEXT;

-- Remove obsolete columns (SQLite doesn't support DROP COLUMN before 3.35)
-- Instead we'll just stop using them: code, avatar_url

-- Create index on employee_number for lookup
CREATE INDEX IF NOT EXISTS idx_operators_employee_number ON operators(employee_number);
CREATE INDEX IF NOT EXISTS idx_operators_employee_id ON operators(employee_id);
"#;

/// Version 11 Schema - Tax config columns on terminal_config
///
/// Stores the default tax rate and tax-inclusive flag from backend
/// so the UI can display currency/tax dynamically instead of hardcoding.
pub const SCHEMA_V11: &str = r#"
-- ============================================================================
-- TERMINAL CONFIG - TAX CONFIGURATION COLUMNS
-- ============================================================================

-- Add tax_rate and tax_inclusive to terminal_config for backend-driven config
ALTER TABLE terminal_config ADD COLUMN tax_rate REAL DEFAULT 0;
ALTER TABLE terminal_config ADD COLUMN tax_inclusive INTEGER DEFAULT 0;
"#;

/// Version 12 Schema - Product type awareness (Phase 3 Track H)
///
/// Adds product_type, track_inventory, and product_nature columns to
/// support non-inventory products (services, fees, labor) on the POS terminal.
pub const SCHEMA_V12: &str = r#"
-- ============================================================================
-- PRODUCTS - PRODUCT TYPE AWARENESS (Phase 3 Track H)
-- ============================================================================

-- Add product type classification
ALTER TABLE products ADD COLUMN product_type TEXT DEFAULT 'PHYSICAL_GOOD';
-- Whether this product tracks inventory (services/fees = false)
ALTER TABLE products ADD COLUMN track_inventory INTEGER DEFAULT 1;
-- Product nature: TANGIBLE, INTANGIBLE, HYBRID
ALTER TABLE products ADD COLUMN product_nature TEXT DEFAULT 'TANGIBLE';

-- Index for fast filtering by product type
CREATE INDEX IF NOT EXISTS idx_products_type ON products(product_type);
"#;

/// Returns the current schema version
pub const CURRENT_SCHEMA_VERSION: i32 = 12;
