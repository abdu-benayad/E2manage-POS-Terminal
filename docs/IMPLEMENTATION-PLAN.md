# E2Manage POS Terminal - Implementation Plan

> **Phase-by-Phase Implementation Guide for Rust/Slint POS Terminal**
>
> **Total Phases**: 10
> **Backend Status**: 100% Complete (11 phases, 267 unit tests, 142 E2E tests)

---

## Overview

| Phase | Name | Description |
|-------|------|-------------|
| 0 | Project Setup | Structure, dependencies, theme |
| 1 | Core Components | Reusable Slint components |
| 2 | Auth Flow | Splash, Login, Shift Start |
| 3 | Sync Infrastructure | HTTP client, SQLite, polling |
| 4 | Checkout Core | Main checkout, cart, products |
| 5 | Payment Flow | All payment screens |
| 6 | Receipts & Drafts | Receipt preview, save/recall |
| 7 | Returns | Return flow screens |
| 8 | Shift Management | End shift, X/Z reports |
| 9 | Settings | All settings screens |
| 10 | Offline & Polish | Offline mode, RTL, error handling |

---

## Phase 0: Project Setup

### Objectives

- Set up proper project structure
- Configure all Rust dependencies
- Create `theme.slint` with design tokens from DESIGN-SYSTEM.md
- Verify Slint builds and runs

### Tasks

1. **Create directory structure** as per ARCHITECTURE.md
2. **Update Cargo.toml** with all dependencies
3. **Create build.rs** for Slint compilation
4. **Create theme.slint** with all design tokens
5. **Create basic main.slint** that imports theme and shows gradient background
6. **Create config/default.toml** with default settings
7. **Create src/config.rs** for configuration loading
8. **Create src/error.rs** with error types
9. **Verify build and run**

### Files to Create/Update

```
[x] Cargo.toml (update with full dependencies)
[ ] build.rs
[ ] config/default.toml
[ ] src/main.rs (update)
[ ] src/lib.rs
[ ] src/app.rs
[ ] src/config.rs
[ ] src/error.rs
[ ] ui/theme.slint
[ ] ui/main.slint (update)
```

### Cargo.toml Dependencies

```toml
[dependencies]
slint = "1.8"
tokio = { version = "1", features = ["full", "sync", "time", "rt-multi-thread", "macros"] }
reqwest = { version = "0.12", features = ["json", "rustls-tls"], default-features = false }
rusqlite = { version = "0.32", features = ["bundled", "chrono"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
chrono = { version = "0.4", features = ["serde"] }
uuid = { version = "1", features = ["v4", "serde"] }
bcrypt = "0.15"
thiserror = "1"
anyhow = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
directories = "5"
once_cell = "1"
parking_lot = "0.12"
rust_decimal = { version = "1", features = ["serde"] }

[build-dependencies]
slint-build = "1.8"
```

### Verification

```bash
cargo build --release
cargo run
# Should show a window with blue gradient background
# Status bar at bottom with "Terminal: POS-001 | v0.1.0 | Online"
```

---

## Phase 1: Core Components

### Objectives

- Build all reusable Slint components
- Components match design system exactly
- RTL-ready from the start
- Test each component in isolation

### Components to Build

| # | Component | File | Based On |
|---|-----------|------|----------|
| 1 | Button | `button.slint` | Primary, Secondary, Danger, Ghost, Icon |
| 2 | Input | `input.slint` | Text, Numeric, Search |
| 3 | NumericKeypad | `numeric_keypad.slint` | PIN, Amount entry |
| 4 | ProgressBar | `progress_bar.slint` | Determinate, Indeterminate |
| 5 | StatusBar | `status_bar.slint` | Bottom status with connection |
| 6 | Header | `header.slint` | Top bar with navigation |
| 7 | Card | `card.slint` | Panel container |
| 8 | ListItem | `list_item.slint` | Selectable row |
| 9 | Dialog | `dialog.slint` | Modal overlay |
| 10 | Toast | `toast.slint` | Notification popup |
| 11 | Avatar | `avatar.slint` | User avatar circle |
| 12 | StatusDot | `status_dot.slint` | Connection indicator |
| 13 | CashierTile | `cashier_tile.slint` | Login selection tile |
| 14 | CategoryTile | `category_tile.slint` | Product category |
| 15 | PaymentTile | `payment_tile.slint` | Payment method |
| 16 | CartItem | `cart_item.slint` | Cart line item |

### Files to Create

```
[ ] ui/components/mod.slint
[ ] ui/components/button.slint
[ ] ui/components/input.slint
[ ] ui/components/numeric_keypad.slint
[ ] ui/components/progress_bar.slint
[ ] ui/components/status_bar.slint
[ ] ui/components/header.slint
[ ] ui/components/card.slint
[ ] ui/components/list_item.slint
[ ] ui/components/dialog.slint
[ ] ui/components/toast.slint
[ ] ui/components/avatar.slint
[ ] ui/components/status_dot.slint
[ ] ui/components/cashier_tile.slint
[ ] ui/components/category_tile.slint
[ ] ui/components/payment_tile.slint
[ ] ui/components/cart_item.slint
```

### Component Specifications

#### Button Component

```slint
// Variants: primary, secondary, danger, ghost, icon
// States: default, hover, pressed, disabled
// Sizes: standard (48px), small (40px), icon (48x48)

export component Button inherits Rectangle {
    in property <string> text;
    in property <string> variant: "primary"; // primary, secondary, danger, ghost
    in property <bool> disabled: false;
    in property <length> icon-size: 24px;
    callback clicked;

    height: 48px;
    min-width: 120px;
    border-radius: 8px;
    // ... implementation
}
```

#### NumericKeypad Component

```slint
// 3x4 grid: 1-9, backspace, 0, confirm
// Support PIN entry (dots) and amount entry (decimal)

export component NumericKeypad inherits Rectangle {
    in property <string> mode: "pin"; // "pin" or "amount"
    in-out property <string> value;
    in property <int> max-length: 6;
    callback confirmed(string);

    // Keys 80x64px with 12px gap
    // ... implementation
}
```

### Reference Wireframes

- `Pos-splash-screen-1-1.md` - ProgressBar, StatusBar
- `login_pin_wireframe.md` - NumericKeypad, PIN dots
- `Screen-3-1-Payment-Methods.md` - PaymentTile, Card
- `Screen-8-1-Settings-Home.md` - ListItem, Header
- `Screen 1 2 Login Cashier Selection.md` - CashierTile, Avatar

### Verification

Create a test screen that displays all components:

```slint
// ui/screens/component_test.slint
export component ComponentTest inherits VerticalLayout {
    // Show one instance of each component
    // Verify visual appearance matches design system
}
```

---

## Phase 2: Auth Flow Screens

### Objectives

- Implement screens 1.1 through 1.5
- Screen navigation with state management
- Mock authentication (real API in Phase 3)

### Screens to Build

| # | Screen | File | Wireframe |
|---|--------|------|-----------|
| 1.1 | Splash | `splash.slint` | `Pos-splash-screen-1-1.md` |
| 1.2 | Login Select | `login_select.slint` | `Screen 1 2 Login Cashier Selection.md` |
| 1.3 | Login PIN | `login_pin.slint` | `login_pin_wireframe.md` |
| 1.4 | Login Face | `login_face.slint` | `Screen 1-4 – Face-Login.md` |
| 1.5 | Shift Start | `shift_start.slint` | `Screen1-5–Start-Shift-Opening Float.md` |

### Files to Create

```
[ ] ui/screens/mod.slint
[ ] ui/screens/auth/splash.slint
[ ] ui/screens/auth/login_select.slint
[ ] ui/screens/auth/login_pin.slint
[ ] ui/screens/auth/login_face.slint
[ ] ui/screens/auth/shift_start.slint
[ ] src/services/mod.rs
[ ] src/services/auth_service.rs
```

### Screen Flow

```
SPLASH (1.1)
  │ Loading complete
  ▼
LOGIN SELECT (1.2)
  │ Tap cashier tile  ──► LOGIN FACE (1.4) ──┐
  ▼                                          │
LOGIN PIN (1.3)                              │
  │ PIN verified                             │
  ▼                                          ▼
SHIFT START (1.5)
  │ Shift started
  ▼
MAIN CHECKOUT (2.1)
```

### Splash Screen Implementation

```slint
export component SplashScreen inherits Rectangle {
    in property <float> progress: 0.6;
    in property <string> status-text: "Syncing products...";
    in property <string> terminal-code: "POS-001";
    in property <string> version: "v1.0.0";
    in property <bool> is-online: true;

    // Blue gradient background
    background: @linear-gradient(180deg, #1E3A8A 0%, #2563EB 100%);

    // Center stack: Logo, App name, Progress bar, Status
    // Bottom status bar
}
```

### Rust Integration

```rust
// src/services/auth_service.rs
pub struct AuthService {
    db: Arc<Database>,
    api: Arc<ApiClient>,
}

impl AuthService {
    pub async fn get_operators(&self) -> Result<Vec<Operator>> {
        // Return cached operators from local DB
    }

    pub fn verify_pin(&self, operator_id: &str, pin: &str) -> Result<bool> {
        // Verify PIN against local hash
    }

    pub async fn start_shift(&self, operator_id: &str, opening_cash: Decimal) -> Result<Shift> {
        // Create shift locally, sync to server if online
    }
}
```

### Verification

```
1. App launches and shows splash screen with progress
2. After 2-3 seconds, navigates to login select
3. Tapping a cashier shows PIN entry
4. Entering correct PIN (mock: "1234") shows shift start
5. Confirming opening float navigates to main checkout (placeholder)
```

---

## Phase 3: Sync Infrastructure

### Objectives

- Implement HTTP client with session management
- Set up SQLite database with schema
- Build polling service for 6 endpoints
- Version tracking with ETag support

### Files to Create

```
[ ] src/api/mod.rs
[ ] src/api/client.rs
[ ] src/api/auth.rs
[ ] src/api/sync.rs
[ ] src/api/types.rs
[ ] src/db/mod.rs
[ ] src/db/connection.rs
[ ] src/db/schema.rs
[ ] src/db/migrations.rs
[ ] src/db/products.rs
[ ] src/db/transactions.rs
[ ] src/db/sync_state.rs
[ ] src/services/sync_service.rs
```

### API Client

```rust
// src/api/client.rs
pub struct ApiClient {
    client: reqwest::Client,
    base_url: String,
    session_token: Arc<RwLock<Option<String>>>,
}

impl ApiClient {
    pub async fn authenticate(&self, hardware_id: &str, secret: &str) -> Result<AuthResponse> {
        let response = self.client
            .post(&format!("{}/api/pos/terminals/login", self.base_url))
            .json(&json!({ "hardwareId": hardware_id, "secret": secret }))
            .send()
            .await?;
        // Handle response, store session token
    }

    pub async fn sync_catalog(&self, etag: Option<&str>) -> Result<Option<CatalogResponse>> {
        // Implement ETag checking, return None if 304
    }
}
```

### SQLite Schema

```sql
-- Products cache
CREATE TABLE products (
    id TEXT PRIMARY KEY,
    sku TEXT NOT NULL,
    name TEXT NOT NULL,
    name_ar TEXT,
    price REAL NOT NULL,
    tax_rate REAL DEFAULT 0,
    category_id TEXT,
    stock_qty INTEGER DEFAULT 0,
    barcode TEXT,
    image_url TEXT,
    updated_at TEXT NOT NULL
);

-- Offline transactions
CREATE TABLE offline_transactions (
    offline_id TEXT PRIMARY KEY,
    type TEXT NOT NULL,
    items_json TEXT NOT NULL,
    payments_json TEXT NOT NULL,
    total REAL NOT NULL,
    created_at TEXT NOT NULL,
    sync_status TEXT DEFAULT 'pending',
    server_id TEXT,
    retry_count INTEGER DEFAULT 0,
    last_error TEXT
);

-- Sync state
CREATE TABLE sync_state (
    resource TEXT PRIMARY KEY,
    etag TEXT,
    version TEXT,
    last_sync TEXT
);
```

### Polling Service

```rust
// src/services/sync_service.rs
pub struct SyncService {
    api: Arc<ApiClient>,
    db: Arc<Database>,
    config: SyncConfig,
}

impl SyncService {
    pub async fn start_polling(&self) {
        loop {
            self.sync_all().await;
            tokio::time::sleep(Duration::from_secs(self.config.interval_seconds)).await;
        }
    }

    async fn sync_all(&self) {
        // Parallel sync of catalog, screens, config
        let (catalog, screens, config) = tokio::join!(
            self.sync_catalog(),
            self.sync_screens(),
            self.sync_config(),
        );
        // Handle results, update UI state
    }
}
```

### Verification

```
1. Terminal authenticates with backend on startup
2. Catalog syncs and products appear in local DB
3. After 10 minutes, re-sync happens (or on demand)
4. ETag prevents unnecessary data transfer
5. Offline: app continues with cached data
```

---

## Phase 4: Checkout Core

### Objectives

- Implement main checkout screen (2.1)
- Product search (2.2) and item edit (2.3)
- Cart management with totals calculation
- Category filtering and recent items

### Screens to Build

| # | Screen | File | Wireframe |
|---|--------|------|-----------|
| 2.1 | Main Checkout | `main.slint` | `Screen2-1–MainCheckout.md` |
| 2.2 | Product Search | `product_search.slint` | `Screen-2-2–Product-Search.md` |
| 2.3 | Item Edit | `item_edit.slint` | `Screen2-3–Item-Detail-and-Edit.md` |

### Files to Create

```
[ ] ui/screens/checkout/main.slint
[ ] ui/screens/checkout/product_search.slint
[ ] ui/screens/checkout/item_edit.slint
[ ] src/models/mod.rs
[ ] src/models/cart.rs
[ ] src/models/product.rs
[ ] src/services/cart_service.rs
```

### Cart Model

```rust
// src/models/cart.rs
pub struct Cart {
    pub items: Vec<CartItem>,
    pub customer_id: Option<String>,
    pub customer_name: Option<String>,
}

pub struct CartItem {
    pub product_id: String,
    pub sku: String,
    pub name: String,
    pub quantity: u32,
    pub unit_price: Decimal,
    pub discount_amount: Decimal,
    pub tax_amount: Decimal,
    pub note: Option<String>,
}

impl Cart {
    pub fn subtotal(&self) -> Decimal { ... }
    pub fn total_discount(&self) -> Decimal { ... }
    pub fn total_tax(&self) -> Decimal { ... }
    pub fn grand_total(&self) -> Decimal { ... }
    pub fn item_count(&self) -> u32 { ... }

    pub fn add_item(&mut self, product: &Product, quantity: u32) { ... }
    pub fn update_quantity(&mut self, index: usize, quantity: u32) { ... }
    pub fn remove_item(&mut self, index: usize) { ... }
    pub fn clear(&mut self) { ... }
}
```

### Checkout Screen Layout

```
Desktop (1920x1080):
┌─────────────────────────────────────────────────────────────────┐
│ HEADER: [Menu] E2Manage POS | Ahmed • Shift #12 | 10:30 Online  │
├─────────────────────────────────────────────────────────────────┤
│ [Search bar with voice/camera]                                  │
├────────────────────────────────┬────────────────────────────────┤
│ CATEGORIES (horizontal tiles)  │ CART                           │
│ [Produce][Dairy][Bakery]...    │ ┌──────────────────────────┐   │
├────────────────────────────────┤ │ Apple 1kg  ×1   4.00     │   │
│ RECENT ITEMS                   │ │ Milk 1L    ×1   2.50     │   │
│ - Milk 1L          2.500       │ │ ...                      │   │
│ - Eggs (30)        8.000       │ ├──────────────────────────┤   │
│ - Rice 5kg        15.000       │ │ Subtotal      18.00      │   │
│ ...                            │ │ Tax 5%         0.90      │   │
│                                │ │ TOTAL         18.90      │   │
├────────────────────────────────┤ ├──────────────────────────┤   │
│ [Drafts] [Transfer] [Returns]  │ │ [Cash][Card][Split][PAY] │   │
│                                │ │ [Hold][Recall][Void]     │   │
└────────────────────────────────┴────────────────────────────────┘
```

### Verification

```
1. Main checkout shows categories and recent items
2. Tapping product adds to cart, updates totals
3. Search bar filters products
4. Tapping cart item opens edit modal
5. Quantity +/- updates totals instantly
6. PAY button navigates to payment methods
```

---

## Phase 5: Payment Flow

### Objectives

- Implement all payment screens (3.1 - 3.5)
- Cash payment with change calculation
- Card payment with EMV status
- Split payment management
- Mobile wallet QR display

### Screens to Build

| # | Screen | File | Wireframe |
|---|--------|------|-----------|
| 3.1 | Payment Methods | `methods.slint` | `Screen-3-1-Payment-Methods.md` |
| 3.2 | Cash Payment | `cash.slint` | `Screen-3-2-Cash-Payment.md` |
| 3.3 | Card Payment | `card.slint` | `Screen-3-3-Card-Payment-EMV.md` |
| 3.4 | Split Payment | `split.slint` | `Screen-3-4-SplitPayment.md` |
| 3.5 | Mobile Wallet | `qr.slint` | `Screen-3-5-Mobile-Wallet-QR-Payment.md` |

### Files to Create

```
[ ] ui/screens/payment/methods.slint
[ ] ui/screens/payment/cash.slint
[ ] ui/screens/payment/card.slint
[ ] ui/screens/payment/split.slint
[ ] ui/screens/payment/qr.slint
[ ] src/models/payment.rs
[ ] src/services/payment_service.rs
```

### Payment Model

```rust
// src/models/payment.rs
pub enum PaymentMethod {
    Cash,
    Card,
    Mobile,
    StoreCredit,
    GiftCard,
}

pub struct Payment {
    pub method: PaymentMethod,
    pub amount: Decimal,
    pub reference: Option<String>,
    pub status: PaymentStatus,
}

pub enum PaymentStatus {
    Pending,
    Processing,
    Approved,
    Declined,
    Failed,
}
```

### Cash Payment Flow

```
1. Display total due
2. Show quick amount buttons (20, 50, 100, Exact, Custom)
3. Numeric keypad for custom entry
4. Calculate and display change
5. Complete Payment → Receipt screen
```

### Card Payment States

```
Connecting → Present Card → Reading → PIN Entry → Authorizing → Approved/Declined
```

### Verification

```
1. Payment methods screen shows all 6 options
2. Cash: keypad works, change calculates correctly
3. Card: shows EMV status states (mock)
4. Split: can add multiple payments, tracks remaining
5. QR: displays code with expiry timer
```

---

## Phase 6: Receipts & Drafts

### Objectives

- Transaction complete screen with receipt preview
- Receipt printing (ESC/POS)
- Save draft functionality
- Recall/recall draft orders

### Screens to Build

| # | Screen | File | Wireframe |
|---|--------|------|-----------|
| 4.1 | Transaction Complete | `complete.slint` | `Screen4-1–Transaction-Complete.md` |
| 4.2 | Reprint Receipt | `reprint.slint` | `Screen-4-2–Reprint-Receipt.md` |
| 5.1 | Save Draft | `save.slint` | `Screen-5-1–Save-Draft.md` |
| 5.2 | Recall Draft | `recall.slint` | `Screen-5-2–Recall-Draft.md` |
| 5.3 | Order Transfer | `transfer.slint` | `Screen-5-3–Order-Transfer.md` |

### Files to Create

```
[ ] ui/screens/receipt/complete.slint
[ ] ui/screens/receipt/reprint.slint
[ ] ui/screens/draft/save.slint
[ ] ui/screens/draft/recall.slint
[ ] ui/screens/draft/transfer.slint
[ ] src/models/transaction.rs
[ ] src/services/transaction_service.rs
[ ] src/services/print_service.rs
[ ] src/hardware/printer.rs
```

### Receipt Data Model

```rust
pub struct Receipt {
    pub header: ReceiptHeader,
    pub items: Vec<ReceiptItem>,
    pub totals: ReceiptTotals,
    pub payments: Vec<ReceiptPayment>,
    pub footer: ReceiptFooter,
}

pub struct ReceiptHeader {
    pub store_name: String,
    pub store_address: String,
    pub store_phone: String,
    pub transaction_number: String,
    pub date_time: DateTime<Local>,
    pub cashier_name: String,
    pub terminal_code: String,
}
```

### Verification

```
1. Transaction complete shows receipt preview
2. Print/Email/SMS options work
3. Save draft stores cart with name/expiry
4. Recall shows list of drafts
5. Tapping draft restores cart
```

---

## Phase 7: Returns

### Objectives

- Return entry mode selection
- Item selection for return
- Refund method handling

### Screens to Build

| # | Screen | File | Wireframe |
|---|--------|------|-----------|
| 6.1 | Return Entry | `entry.slint` | `Screen-6-1–Return-Entry.md` |
| 6.2 | Select Items | `items.slint` | `Screen-6-2–Select-Items.md` |
| 6.3 | Refund Method | `refund.slint` | `Screen-6-3–Refund-Method.md` |

### Files to Create

```
[ ] ui/screens/return/entry.slint
[ ] ui/screens/return/items.slint
[ ] ui/screens/return/refund.slint
[ ] src/services/return_service.rs
```

### Return Flow

```
Return Entry Mode
├── Scan Receipt → Load original transaction
├── Search Transaction → Find by date/amount
└── No Receipt → Manual entry (requires approval)
    │
    ▼
Select Items to Return
    │ Check items, select reason
    ▼
Refund Method
├── Cash → Open drawer
├── Card Reversal → Process refund
└── Store Credit → Add to customer balance
```

### Verification

```
1. Return entry shows 3 options
2. Can search and select original transaction
3. Item selection with checkboxes
4. Refund completes and shows confirmation
```

---

## Phase 8: Shift Management

### Objectives

- End shift with cash count
- X-Report (current shift summary)
- Z-Report (end of day)

### Screens to Build

| # | Screen | File | Wireframe |
|---|--------|------|-----------|
| 7.2 | End Shift | `end.slint` | `Screen-7-2–End-Shift-Cash-Count.md` |
| 7.3 | X-Report | `x_report.slint` | `Screen-7-3–X-Report.md` |
| 7.4 | Z-Report | `z_report.slint` | `Screen-7-4–Z-Report.md` |

### Files to Create

```
[ ] ui/screens/shift/end.slint
[ ] ui/screens/shift/x_report.slint
[ ] ui/screens/shift/z_report.slint
[ ] src/models/shift.rs
[ ] src/services/shift_service.rs
[ ] src/services/report_service.rs
```

### Cash Count Screen

```
Bills: 50, 20, 10, 5, 1 → Qty input → Subtotal
Coins: 0.50, 0.25, 0.10, 0.05 → Qty input → Subtotal
────────────────────────────────────
Expected: 2,350.00 LYD
Counted:  2,340.00 LYD
Variance: -10.00 LYD (Short)
────────────────────────────────────
Note (required if variance ≠ 0): [____________]
[Save Draft] [Close Shift]
```

### Z-Report Data

```rust
pub struct ZReport {
    pub shift_number: String,
    pub terminal_code: String,
    pub operator_name: String,
    pub opened_at: DateTime<Local>,
    pub closed_at: DateTime<Local>,
    pub opening_cash: Decimal,
    pub closing_cash: Decimal,
    pub sales_summary: SalesSummary,
    pub payment_breakdown: HashMap<PaymentMethod, Decimal>,
    pub cash_reconciliation: CashReconciliation,
}
```

### Verification

```
1. Cash count with denomination input
2. Variance calculation and status
3. X-Report shows current shift totals
4. Z-Report shows full day summary
5. Close shift requires variance note if mismatch
```

---

## Phase 9: Settings

### Objectives

- Settings home with navigation
- All settings sub-screens
- Hardware diagnostics

### Screens to Build

| # | Screen | File | Wireframe |
|---|--------|------|-----------|
| 8.1 | Settings Home | `home.slint` | `Screen-8-1–Settings-Home.md` |
| 8.2 | General | `general.slint` | `Screen-8-2–General-Settings.md` |
| 8.3 | Receipt | `receipt.slint` | `Screen-8-3–Receipt-Printing.md` |
| 8.4 | Payments | `payments.slint` | `Screen-8-4–Payments-Settings.md` |
| 8.5 | Users | `users.slint` | `Screen-8-5–Users-Roles.md` |
| 8.6 | Hardware | `hardware.slint` | `Screen-8-6–Terminal-Hardware.md` |
| 8.7 | Security | `security.slint` | `Screen-8-7–Security.md` |
| 8.8 | Backup | `backup.slint` | `Screen-8-8–Data-Backup.md` |
| 8.9 | Reports | `reports.slint` | `Screen-8-9–Reports-Audit.md` |
| 8.10 | About | `about.slint` | `Screen-8-10–About.md` |
| 8.11 | System Info | `system_info.slint` | `Screen-8-11–System-Information.md` |
| 8.12 | Network Diag | `network_diag.slint` | `Screen-8-12–Network-Diagnostics.md` |
| 8.13 | Hardware Diag | `hardware_diag.slint` | `Screen-8-13–Hardware-Diagnostics.md` |

### Files to Create

```
[ ] ui/screens/settings/home.slint
[ ] ui/screens/settings/general.slint
[ ] ui/screens/settings/receipt.slint
[ ] ui/screens/settings/payments.slint
[ ] ui/screens/settings/users.slint
[ ] ui/screens/settings/hardware.slint
[ ] ui/screens/settings/security.slint
[ ] ui/screens/settings/backup.slint
[ ] ui/screens/settings/reports.slint
[ ] ui/screens/settings/about.slint
[ ] ui/screens/settings/system_info.slint
[ ] ui/screens/settings/network_diag.slint
[ ] ui/screens/settings/hardware_diag.slint
```

### Settings Layout

```
Two-pane layout:
┌──────────────────┬─────────────────────────────────────────┐
│ Left Nav (320px) │ Content                                 │
├──────────────────┤                                         │
│ 🔍 Search...     │ Settings Home                           │
│ ────────────────│ Quick actions: [Test Print] [Sync Now]  │
│ ⚙️ General       │                                         │
│ 🖨️ Receipt       │ Status: Terminal, Sync, Printer, EMV    │
│ 💳 Payments      │                                         │
│ 👥 Users         │ Common settings: Language, Currency...  │
│ 🏷️ Taxes         │                                         │
│ 📦 Inventory     │                                         │
│ ...              │                                         │
└──────────────────┴─────────────────────────────────────────┘
```

### Verification

```
1. Settings home shows nav and quick actions
2. Test Print sends receipt to printer
3. Sync Now triggers manual sync
4. Network diagnostics show connection status
5. All sub-screens accessible and functional
```

---

## Phase 10: Offline & Polish

### Objectives

- Complete offline mode handling
- RTL support verification
- Error handling and recovery
- Performance optimization
- Final testing

### Tasks

1. **Offline Queue Management**
   - Store transactions locally when offline
   - Auto-sync when connection restored
   - Handle conflicts and failures

2. **RTL Support**
   - Test all screens in Arabic
   - Verify mirroring behavior
   - Fix any layout issues

3. **Error Handling**
   - Network error recovery
   - Printer offline handling
   - Invalid data recovery

4. **Performance**
   - Optimize SQLite queries
   - Reduce UI re-renders
   - Profile memory usage

5. **Testing**
   - End-to-end flow testing
   - Offline scenario testing
   - Edge case handling

### Files to Create/Update

```
[ ] src/services/offline_service.rs
[ ] src/utils/i18n.rs
[ ] tests/integration/offline_tests.rs
[ ] tests/integration/flow_tests.rs
```

### Offline Indicator

```
When offline:
- Status bar shows "Offline" with amber dot
- Top banner: "Working offline. Transactions will sync when online."
- Transaction complete shows "Not synced" indicator
- Sync count shown in status bar
```

### Verification

```
1. Disconnect network → app shows offline state
2. Create transaction → saves locally
3. Reconnect → transaction syncs automatically
4. RTL toggle → all screens mirror correctly
5. Full checkout flow completes without errors
```

---

## Appendix: Screen Reference

### Complete Screen List

| # | Screen | Status |
|---|--------|--------|
| 1.1 | Splash | Phase 2 |
| 1.2 | Login Select | Phase 2 |
| 1.3 | Login PIN | Phase 2 |
| 1.4 | Login Face | Phase 2 |
| 1.5 | Shift Start | Phase 2 |
| 2.1 | Main Checkout | Phase 4 |
| 2.2 | Product Search | Phase 4 |
| 2.3 | Item Edit | Phase 4 |
| 3.1 | Payment Methods | Phase 5 |
| 3.2 | Cash Payment | Phase 5 |
| 3.3 | Card Payment | Phase 5 |
| 3.4 | Split Payment | Phase 5 |
| 3.5 | Mobile Wallet | Phase 5 |
| 4.1 | Transaction Complete | Phase 6 |
| 4.2 | Reprint Receipt | Phase 6 |
| 5.1 | Save Draft | Phase 6 |
| 5.2 | Recall Draft | Phase 6 |
| 5.3 | Order Transfer | Phase 6 |
| 6.1 | Return Entry | Phase 7 |
| 6.2 | Select Items | Phase 7 |
| 6.3 | Refund Method | Phase 7 |
| 7.2 | End Shift | Phase 8 |
| 7.3 | X-Report | Phase 8 |
| 7.4 | Z-Report | Phase 8 |
| 8.1-13 | Settings (13 screens) | Phase 9 |

---

**Document Version**: 1.0
**Last Updated**: 2025-12-12
