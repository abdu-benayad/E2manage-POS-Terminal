# E2Manage POS Terminal - Architecture

> **Native POS Terminal built with Rust and Slint UI Framework**
>
> **Version**: 1.0
> **Target Platforms**: Linux (x86_64, aarch64, armv7), Windows, macOS
> **Backend**: E2Manage API (wadi-dms-api)

---

## Overview

The E2Manage POS Terminal is a **native, offline-first POS application** built with Rust and Slint. It supports multiple business sectors (Supermarket, Restaurant, Retail, Distributor) through JSON-driven screen configurations fetched from the backend.

### Key Architecture Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Language | Rust | Memory-safe, fast boot, low footprint |
| UI Framework | Slint | Native UI, RTL support, declarative |
| Communication | HTTP Polling | Simple, reliable, scales to 10K terminals |
| Sync Interval | 10 minutes | Acceptable latency for all operations |
| UI System | JSON-driven screens | No app updates for screen changes |
| Local Storage | SQLite | Reliable, embedded, zero dependencies |
| Offline Mode | Full 24h+ operation | Transactions queue locally |

---

## Project Structure

```
e2manage-pos-terminal/
├── Cargo.toml                    # Dependencies and build config
├── Cargo.lock                    # Locked dependency versions
├── build.rs                      # Slint build script
├── README.md                     # Project overview
├── LICENSE                       # MIT License
│
├── config/
│   ├── default.toml              # Default configuration
│   ├── development.toml          # Development overrides
│   └── production.toml           # Production settings
│
├── assets/
│   ├── fonts/                    # Custom fonts (Arabic support)
│   │   └── NotoSansArabic.ttf
│   ├── icons/                    # SVG/PNG icons
│   │   ├── menu.svg
│   │   ├── settings.svg
│   │   ├── search.svg
│   │   └── ...
│   └── images/
│       └── logo.png              # App logo
│
├── src/
│   ├── main.rs                   # Entry point, app initialization
│   ├── app.rs                    # Application state management
│   ├── config.rs                 # Configuration loading (TOML)
│   ├── error.rs                  # Error types (thiserror)
│   ├── lib.rs                    # Library exports
│   │
│   ├── api/                      # Backend API client
│   │   ├── mod.rs                # Module exports
│   │   ├── client.rs             # HTTP client (reqwest)
│   │   ├── auth.rs               # Terminal authentication
│   │   ├── sync.rs               # Polling service
│   │   ├── transactions.rs       # Transaction endpoints
│   │   ├── shifts.rs             # Shift endpoints
│   │   ├── offline.rs            # Offline queue upload
│   │   └── types.rs              # API request/response DTOs
│   │
│   ├── db/                       # Local SQLite database
│   │   ├── mod.rs                # Module exports
│   │   ├── connection.rs         # Connection pool (rusqlite)
│   │   ├── schema.rs             # Table definitions
│   │   ├── migrations.rs         # Schema migrations
│   │   ├── products.rs           # Product cache queries
│   │   ├── transactions.rs       # Offline transaction storage
│   │   ├── shifts.rs             # Shift data storage
│   │   └── sync_state.rs         # Version tracking
│   │
│   ├── models/                   # Domain models
│   │   ├── mod.rs                # Module exports
│   │   ├── terminal.rs           # Terminal state
│   │   ├── shift.rs              # Shift data
│   │   ├── transaction.rs        # Transaction/cart
│   │   ├── product.rs            # Product/catalog
│   │   ├── payment.rs            # Payment methods
│   │   ├── user.rs               # Cashier/operator
│   │   ├── cart.rs               # Cart management
│   │   └── config.rs             # Terminal configuration
│   │
│   ├── services/                 # Business logic
│   │   ├── mod.rs                # Module exports
│   │   ├── auth_service.rs       # Login, PIN verification
│   │   ├── sync_service.rs       # Background polling
│   │   ├── cart_service.rs       # Cart management
│   │   ├── transaction_service.rs # Transaction processing
│   │   ├── payment_service.rs    # Payment handling
│   │   ├── shift_service.rs      # Shift lifecycle
│   │   ├── offline_service.rs    # Offline queue management
│   │   ├── print_service.rs      # Receipt printing
│   │   └── report_service.rs     # X/Z report generation
│   │
│   ├── hardware/                 # Hardware integration
│   │   ├── mod.rs                # Module exports
│   │   ├── printer.rs            # ESC/POS receipt printer
│   │   ├── scanner.rs            # Barcode scanner
│   │   ├── cash_drawer.rs        # Cash drawer
│   │   ├── card_reader.rs        # EMV terminal integration
│   │   └── scale.rs              # Weighing scale
│   │
│   └── utils/                    # Utilities
│       ├── mod.rs                # Module exports
│       ├── money.rs              # Currency formatting
│       ├── date.rs               # Date/time helpers
│       ├── i18n.rs               # Internationalization
│       └── crypto.rs             # PIN hashing, signatures
│
├── ui/                           # Slint UI files
│   ├── main.slint                # Root window, screen routing
│   ├── theme.slint               # Design tokens (colors, fonts, spacing)
│   │
│   ├── components/               # Reusable components
│   │   ├── mod.slint             # Component exports
│   │   ├── button.slint          # Button variants
│   │   ├── input.slint           # Text input
│   │   ├── numeric_keypad.slint  # PIN/amount entry
│   │   ├── progress_bar.slint    # Progress indicator
│   │   ├── status_bar.slint      # Bottom status bar
│   │   ├── header.slint          # Top header bar
│   │   ├── card.slint            # Card container
│   │   ├── list_item.slint       # List row
│   │   ├── dialog.slint          # Modal dialog
│   │   ├── toast.slint           # Toast notifications
│   │   ├── avatar.slint          # User avatar
│   │   ├── status_dot.slint      # Connection status dot
│   │   ├── cashier_tile.slint    # Cashier selection tile
│   │   ├── category_tile.slint   # Category grid tile
│   │   ├── product_row.slint     # Product list row
│   │   ├── cart_item.slint       # Cart line item
│   │   ├── payment_tile.slint    # Payment method tile
│   │   └── icon.slint            # Icon component
│   │
│   └── screens/                  # Screen implementations
│       ├── mod.slint             # Screen exports
│       │
│       │── auth/                 # Authentication screens (1.x)
│       │   ├── splash.slint      # 1.1 Splash screen
│       │   ├── login_select.slint # 1.2 Cashier selection
│       │   ├── login_pin.slint   # 1.3 PIN entry
│       │   ├── login_face.slint  # 1.4 Face recognition
│       │   └── shift_start.slint # 1.5 Opening float
│       │
│       ├── checkout/             # Checkout screens (2.x)
│       │   ├── main.slint        # 2.1 Main checkout
│       │   ├── product_search.slint # 2.2 Product search
│       │   └── item_edit.slint   # 2.3 Item detail/edit
│       │
│       ├── payment/              # Payment screens (3.x)
│       │   ├── methods.slint     # 3.1 Payment methods
│       │   ├── cash.slint        # 3.2 Cash payment
│       │   ├── card.slint        # 3.3 Card payment (EMV)
│       │   ├── split.slint       # 3.4 Split payment
│       │   └── qr.slint          # 3.5 Mobile wallet/QR
│       │
│       ├── receipt/              # Receipt screens (4.x)
│       │   ├── complete.slint    # 4.1 Transaction complete
│       │   └── reprint.slint     # 4.2 Reprint receipt
│       │
│       ├── draft/                # Draft screens (5.x)
│       │   ├── save.slint        # 5.1 Save draft
│       │   ├── recall.slint      # 5.2 Recall orders
│       │   └── transfer.slint    # 5.3 Order transfer
│       │
│       ├── return/               # Return screens (6.x)
│       │   ├── entry.slint       # 6.1 Return entry mode
│       │   ├── items.slint       # 6.2 Select items
│       │   └── refund.slint      # 6.3 Refund method
│       │
│       ├── shift/                # Shift screens (7.x)
│       │   ├── end.slint         # 7.2 Cash count
│       │   ├── x_report.slint    # 7.3 X-Report
│       │   └── z_report.slint    # 7.4 Z-Report
│       │
│       └── settings/             # Settings screens (8.x)
│           ├── home.slint        # 8.1 Settings home
│           ├── general.slint     # 8.2 General settings
│           ├── receipt.slint     # 8.3 Receipt & printing
│           ├── payments.slint    # 8.4 Payments config
│           ├── users.slint       # 8.5 Users & roles
│           ├── hardware.slint    # 8.6 Terminal & hardware
│           ├── security.slint    # 8.7 Security
│           ├── backup.slint      # 8.8 Data & backup
│           ├── reports.slint     # 8.9 Reports & audit
│           ├── about.slint       # 8.10 About
│           ├── system_info.slint # 8.11 System information
│           ├── network_diag.slint # 8.12 Network diagnostics
│           └── hardware_diag.slint # 8.13 Hardware diagnostics
│
├── tests/                        # Test files
│   ├── integration/              # Integration tests
│   │   ├── api_tests.rs
│   │   ├── sync_tests.rs
│   │   └── transaction_tests.rs
│   └── unit/                     # Unit tests
│       ├── cart_tests.rs
│       ├── payment_tests.rs
│       └── money_tests.rs
│
└── docs/                         # Documentation
    ├── DESIGN-SYSTEM.md          # Design tokens
    ├── ARCHITECTURE.md           # This file
    └── IMPLEMENTATION-PLAN.md    # Phase-by-phase plan
```

---

## Data Flow

```
┌─────────────────────────────────────────────────────────────────────┐
│                         Slint UI Layer                               │
│  (screens/*.slint, components/*.slint)                               │
│  - Declarative UI definitions                                        │
│  - Bindings to AppState properties                                   │
│  - Callbacks for user interactions                                   │
└─────────────────────────────────┬───────────────────────────────────┘
                                  │ Callbacks & Properties
                                  ▼
┌─────────────────────────────────────────────────────────────────────┐
│                      Application State                               │
│  (app.rs - AppState struct with Arc<Mutex<...>>)                    │
│  - current_screen, current_user, cart, shift                        │
│  - Bridges Slint UI ↔ Services                                      │
│  - Manages screen navigation                                         │
└─────────────┬───────────────────────────────────────┬───────────────┘
              │                                       │
              ▼                                       ▼
┌─────────────────────────────┐       ┌─────────────────────────────────┐
│      Services Layer         │       │      Local Database             │
│  - auth_service             │       │  (SQLite via rusqlite)          │
│  - cart_service             │◄─────►│  - products cache               │
│  - transaction_service      │       │  - offline transactions         │
│  - sync_service             │       │  - shifts, operators            │
│  - payment_service          │       │  - config/state                 │
│  - shift_service            │       │  - sync versions                │
└─────────────┬───────────────┘       └─────────────────────────────────┘
              │
              ▼
┌─────────────────────────────────────────────────────────────────────┐
│                        API Client                                    │
│  (reqwest HTTP client)                                               │
│  - POST /api/pos/terminals/authenticate                              │
│  - GET  /api/pos/sync/catalog (with ETag)                           │
│  - GET  /api/pos/sync/screens (JSON-driven UI)                      │
│  - GET  /api/pos/sync/tenant-config                                 │
│  - POST /api/pos/transactions                                        │
│  - POST /api/pos/shifts/start, /api/pos/shifts/:id/end              │
│  - POST /api/pos/fleet/heartbeat                                    │
│  - POST /api/pos/offline/upload                                     │
└─────────────────────────────────┬───────────────────────────────────┘
                                  │ HTTPS
                                  ▼
┌─────────────────────────────────────────────────────────────────────┐
│                    E2Manage Backend API                              │
│  (Node.js + Express + Prisma + PostgreSQL)                          │
│  - Multi-tenant, multi-currency, multi-language                     │
│  - POS module: /src/modules/pos/                                    │
└─────────────────────────────────────────────────────────────────────┘
```

---

## State Management

### AppState Structure

```rust
pub struct AppState {
    // Terminal identity
    pub terminal_id: String,
    pub terminal_code: String,
    pub hardware_id: String,
    pub session_token: Option<String>,

    // Current user/shift
    pub current_user: Option<User>,
    pub current_shift: Option<Shift>,

    // Navigation
    pub current_screen: Screen,
    pub screen_stack: Vec<Screen>,

    // Cart state
    pub cart: Cart,

    // Connection status
    pub is_online: bool,
    pub last_sync: Option<DateTime<Utc>>,
    pub pending_sync_count: u32,

    // Config
    pub config: TerminalConfig,
    pub tenant_config: TenantConfig,
    pub locale: String, // "ar", "en", "fr"

    // Versions (for ETag checking)
    pub catalog_version: String,
    pub screens_version: String,
    pub config_version: String,
}

pub enum Screen {
    Splash,
    LoginSelect,
    LoginPin { cashier_id: String },
    LoginFace,
    ShiftStart,
    Checkout,
    ProductSearch,
    ItemEdit { item_index: usize },
    PaymentMethods,
    PaymentCash,
    PaymentCard,
    PaymentSplit,
    PaymentQR,
    TransactionComplete { transaction_id: String },
    SaveDraft,
    RecallDraft,
    ReturnEntry,
    ReturnItems,
    ReturnRefund,
    ShiftEnd,
    XReport,
    ZReport,
    Settings,
    SettingsGeneral,
    // ... more screens
}
```

### Screen Navigation

```rust
impl AppState {
    pub fn navigate_to(&mut self, screen: Screen) {
        self.screen_stack.push(self.current_screen.clone());
        self.current_screen = screen;
    }

    pub fn navigate_back(&mut self) {
        if let Some(previous) = self.screen_stack.pop() {
            self.current_screen = previous;
        }
    }

    pub fn navigate_replace(&mut self, screen: Screen) {
        self.current_screen = screen;
    }
}
```

---

## Polling Strategy

### Sync Service

| Endpoint | Interval | Caching Method | Purpose |
|----------|----------|----------------|---------|
| `/api/pos/sync/catalog` | 10 min | ETag/If-None-Match | Product catalog |
| `/api/pos/sync/screens` | 10 min | ETag/If-None-Match | JSON screen definitions |
| `/api/pos/sync/tenant-config` | 10 min | Version hash | Tenant settings |
| `/api/pos/fleet/heartbeat` | 1 min | No cache | Terminal health |
| `/api/pos/ota/check` | 60 min | No cache | App updates |

### ETag Flow

```rust
pub async fn sync_catalog(&self) -> Result<bool> {
    let mut headers = HeaderMap::new();

    if let Some(etag) = self.db.get_catalog_etag()? {
        headers.insert(IF_NONE_MATCH, etag.parse()?);
    }

    let response = self.client
        .get(&format!("{}/api/pos/sync/catalog", self.base_url))
        .headers(headers)
        .header("X-Terminal-Token", &self.session_token)
        .send()
        .await?;

    match response.status() {
        StatusCode::NOT_MODIFIED => {
            // No changes, data is current
            Ok(false)
        }
        StatusCode::OK => {
            let etag = response.headers()
                .get(ETAG)
                .and_then(|v| v.to_str().ok())
                .map(String::from);

            let catalog: CatalogResponse = response.json().await?;
            self.db.save_catalog(&catalog, etag)?;
            Ok(true)
        }
        _ => Err(ApiError::SyncFailed.into())
    }
}
```

---

## Offline Handling

### Offline Queue Strategy

1. **All transactions saved to SQLite first**
2. **If online**: Sync immediately to backend
3. **If offline**: Queue for later sync
4. **On reconnect**: Process offline queue (FIFO)
5. **Conflict resolution**: Server wins, log conflicts

### Offline Transaction Storage

```rust
pub struct OfflineTransaction {
    pub offline_id: Uuid,
    pub transaction_type: TransactionType,
    pub items: Vec<TransactionItem>,
    pub payments: Vec<Payment>,
    pub total: Decimal,
    pub created_at: DateTime<Utc>,
    pub sync_status: SyncStatus,
    pub retry_count: u32,
    pub last_error: Option<String>,
}

pub enum SyncStatus {
    Pending,
    Syncing,
    Synced,
    Failed,
    Conflict,
}
```

### Offline Queue Processing

```rust
impl OfflineService {
    pub async fn process_queue(&self) -> Result<SyncResult> {
        let pending = self.db.get_pending_transactions()?;
        let mut results = Vec::new();

        for txn in pending {
            match self.sync_transaction(&txn).await {
                Ok(server_id) => {
                    self.db.mark_synced(&txn.offline_id, &server_id)?;
                    results.push(SyncItemResult::Success(txn.offline_id));
                }
                Err(e) => {
                    self.db.increment_retry(&txn.offline_id, &e.to_string())?;
                    results.push(SyncItemResult::Failed(txn.offline_id, e));
                }
            }
        }

        Ok(SyncResult { items: results })
    }
}
```

---

## Security

### Terminal Authentication

```rust
// Terminal registration (first time)
POST /api/pos/terminals/register
{
    "hardwareId": "unique-device-id",
    "secret": "min-32-char-secret",
    "name": "POS-001",
    "businessSector": "SUPERMARKET"
}

// Terminal login (subsequent)
POST /api/pos/terminals/login
{
    "hardwareId": "unique-device-id",
    "secret": "min-32-char-secret"
}
```

### Session Token Management

- Terminal secret stored securely (keyring or encrypted file)
- Session tokens with 24h TTL
- Automatic refresh on API calls
- Token stored in memory, not on disk

### PIN Security

- PIN stored as bcrypt hash locally
- Max 5 failed attempts → 30 minute lockout
- PIN verification happens locally (offline capable)
- Manager PIN required for overrides

### Encryption

- All API calls over HTTPS
- Local SQLite can be encrypted (SQLCipher)
- Sensitive config encrypted at rest

---

## Hardware Integration

### Supported Hardware

| Device | Protocol | Implementation |
|--------|----------|----------------|
| Receipt Printer | ESC/POS | USB/Serial |
| Barcode Scanner | HID Keyboard | USB |
| Cash Drawer | Printer-triggered | USB/Serial |
| EMV Terminal | SDK-specific | USB/Serial/Network |
| Weighing Scale | Serial protocol | Serial |

### Printer Service

```rust
pub trait ReceiptPrinter {
    fn print(&self, receipt: &Receipt) -> Result<()>;
    fn open_drawer(&self) -> Result<()>;
    fn cut_paper(&self) -> Result<()>;
    fn get_status(&self) -> PrinterStatus;
}

pub struct EscPosPrinter {
    port: Box<dyn SerialPort>,
    config: PrinterConfig,
}

impl ReceiptPrinter for EscPosPrinter {
    fn print(&self, receipt: &Receipt) -> Result<()> {
        self.write_header(&receipt.header)?;
        self.write_items(&receipt.items)?;
        self.write_totals(&receipt.totals)?;
        self.write_footer(&receipt.footer)?;
        self.cut_paper()?;
        Ok(())
    }
}
```

---

## Configuration

### Default Configuration (config/default.toml)

```toml
[terminal]
name = "POS-001"
locale = "ar"
currency = "LYD"

[api]
base_url = "https://jooher.app"
timeout_seconds = 30
retry_attempts = 3

[sync]
catalog_interval_minutes = 10
heartbeat_interval_seconds = 60
ota_check_interval_minutes = 60

[offline]
max_queue_size = 1000
retry_interval_seconds = 300
max_retry_attempts = 10

[security]
max_pin_attempts = 5
lockout_minutes = 30
session_ttl_hours = 24

[hardware]
printer_enabled = true
printer_port = "/dev/usb/lp0"
scanner_enabled = true
drawer_enabled = true

[ui]
theme = "default"
rtl = true
show_prices_with_tax = true
```

---

## Testing Strategy

### Unit Tests

- Location: `src/*/tests.rs` modules
- Coverage: Cart logic, payment calculations, money formatting
- Run: `cargo test`

### Integration Tests

- Location: `tests/integration/`
- Coverage: API sync, offline queue, database operations
- Run: `cargo test --test integration`

### UI Tests (Future)

- Location: `tests/ui/`
- Framework: Slint testing utilities
- Coverage: Screen rendering, component behavior

---

## Build & Deployment

### Development Build

```bash
cargo build
cargo run
```

### Release Build

```bash
cargo build --release
# Binary at: target/release/e2manage-pos-terminal
```

### Cross-Compilation

```bash
# ARM Linux (Raspberry Pi)
cargo build --release --target aarch64-unknown-linux-gnu

# Windows
cargo build --release --target x86_64-pc-windows-gnu
```

### Distribution

- Binary size: ~50-100MB
- No external dependencies (statically linked)
- OTA updates via `/api/pos/ota/check` endpoint

---

## Related Documentation

- [DESIGN-SYSTEM.md](./DESIGN-SYSTEM.md) - Design tokens and specs
- [IMPLEMENTATION-PLAN.md](./IMPLEMENTATION-PLAN.md) - Phase-by-phase guide
- [Backend API Reference](/docs/POS-module-plan/E2MANAGE-POS-TERMINAL-V2/API-REFERENCE.md)
- [Research Documents](/docs/POS-module-plan/pos-terminam-researchs/README.md)

---

**Document Version**: 1.0
**Last Updated**: 2025-12-12
