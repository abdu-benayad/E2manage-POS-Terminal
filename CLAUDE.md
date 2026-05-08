# CLAUDE.md

E2Manage POS Terminal — offline-first Point of Sale built with **Rust 1.92** (edition 2021) and **Slint 1.8**.
Connects to E2Manage ERP backend via REST APIs, stores data locally in SQLite, supports Arabic/RTL.

---

## Commands

```bash
cargo build                              # Dev build (mold linker)
cargo build --release                    # Release (~5 min, thin LTO)
cargo build --profile release-prod       # Production (~40+ min, full LTO)
cargo run                                # Run app
cargo check                              # Type check only
cargo test                               # All tests
cargo test --test cart_tests             # Specific test file
cargo test test_add_item_to_cart         # Single test by name
cargo test -- --nocapture                # Tests with stdout
cargo fmt                                # Format
cargo clippy                             # Lint
```

## Build Profiles

| Profile | LTO | Codegen Units | Use Case |
|---------|-----|---------------|----------|
| `dev` | off | incremental | Local development |
| `release` | thin | 16 | Fast release builds |
| `release-prod` | full | 1 | Production binaries |
| `release-small` | full + opt="z" | 1 | Size-optimized builds |

## Feature Flags

```bash
cargo build --features hardware          # Enable serial port for printers/scanners
cargo build --features encrypted-db      # SQLite encryption
cargo build --features face-recognition  # Facial recognition support
```

## Environment Variables

| Variable | Default | Purpose |
|----------|---------|---------|
| `E2M_API_URL` | `http://178.156.135.235:3000` | Backend API URL |
| `RUST_LOG` | `info` | Log level (`debug`, `trace` for verbose) |
| `RUST_BACKTRACE` | `0` | Set to `1` for full backtraces on panic |
| `CARGO_BUILD_JOBS` | auto | Set to `1` on low-memory systems |

## E2E Testing

```bash
# Run all E2E API tests (requires backend running)
./scripts/run-e2e-tests.sh

# Run specific E2E test category
./scripts/run-e2e-tests.sh terminal   # Terminal management
./scripts/run-e2e-tests.sh sync       # Sync API
./scripts/run-e2e-tests.sh shift      # Shift management
./scripts/run-e2e-tests.sh transaction
./scripts/run-e2e-tests.sh return
./scripts/run-e2e-tests.sh offline

# Set custom backend URL
BACKEND_URL=http://localhost:3000 ./scripts/run-e2e-tests.sh
```

## Architecture

### Workspace Crates

The codebase uses a Cargo workspace with these crates in `crates/`:

| Crate | Purpose |
|-------|---------|
| `pos-models` | Domain models: Product, Cart, Transaction, Shift, ZReport |
| `pos-db` | SQLite database with FTS5 search, migrations, offline queue |
| `pos-api` | HTTP client for E2Manage backend (auth, sync, transactions) |
| `pos-escpos` | ESC/POS thermal printer command builder |
| `pos-printing` | Receipt and report printing logic |
| `pos-services` | Business logic: auth, cart, sync, payments, shifts, returns |

### Main Binary (`src/`)

- `main.rs` - Entry point, initializes Slint UI, handles startup sequence
- `lib.rs` - Re-exports from workspace crates for convenience
- `ui/` - Slint UI bridges and helpers
- `hardware/` - Hardware abstraction (printers, scanners)
- `utils/` - Utility functions

### UI Layer (`ui/`)

Uses Slint declarative UI framework:
- `main.slint` - Root window, imports all screens
- `theme.slint` - Colors, typography, spacing, RTL support
- `components/` - Reusable UI components (buttons, inputs, dialogs, etc.)
- `screens/` - Application screens organized by feature (auth, checkout, payment, receipt, etc.)

### Key Services (in `pos-services`)

| Service | Purpose |
|---------|---------|
| `AuthService` | Terminal/operator authentication, PIN verification |
| `SyncService` | Background polling, catalog sync |
| `ProductService` | Product search with FTS5 |
| `CartService` | Shopping cart with discounts, customer support |
| `TransactionService` | Transaction processing and completion |
| `ShiftService` | Shift open/close, cash counting |
| `OfflineService` | Offline transaction queue and sync |
| `EmvService` | Card payment terminal integration |
| `QrService` | QR/mobile wallet payments |
| `PrintService` | Receipt/report printing |
| `DraftService` | Held order management |
| `ReturnService` | Returns and refunds |

### Data Flow

1. **Startup**: Check registration → Show pairing or login screen
2. **Sync**: Poll backend every 10 minutes for catalog/operator updates
3. **Offline**: All operations work offline, queue transactions for sync
4. **Transactions**: Create from cart → Add payments → Complete → Print receipt

## Configuration

- `config/default.toml` - Default settings (API URL, sync interval, currency)
- `.cargo/config.toml` - Build config (mold linker, single job)

## Testing Patterns

Tests use in-memory SQLite databases:

```rust
mod common;
use common::*;

#[test]
fn test_example() {
    let db = setup_test_db();           // In-memory DB with migrations
    let product = sample_product();      // Test product fixture
    let operator = sample_operator();    // Test operator fixture
    // ...
}
```

Test files are in `tests/`:
- `*_tests.rs` - Unit/integration tests
- `e2e_*.rs` - End-to-end workflow tests
- `common/` - Shared test utilities and fixtures

## Key Patterns

- **Arc wrapping**: Services take `Arc<Database>` and `Arc<ApiClient>` for thread safety
- **Result types**: Each service defines its own error enum + result alias (e.g., `CartError`, `type CartResult<T> = Result<T, CartError>`)
- **Error enums**: Use `thiserror::Error` derive macro; chain with `.context()` / `.with_context()`
- **Sync primitives**: `parking_lot::RwLock` / `Mutex` (not std) for performance
- **Decimal math**: `rust_decimal::Decimal` for all currency — never use `f64` for money
- **Tracing**: `tracing::{info, debug, warn, error}` macros; file rotation via `tracing-appender`
- **Event broadcast**: `tokio::sync::broadcast::channel(16)` for inter-service events

## Critical Rules

1. **Never use `f64` for money** — always `rust_decimal::Decimal`
2. **All services must be `Send + Sync`** — wrap in `Arc`, use `parking_lot` locks
3. **Offline-first** — every operation must work without network; queue for later sync
4. **RTL support** — UI must work in Arabic/Hebrew; test both LTR and RTL layouts
5. **Thread safety** — `Arc<Service>` pattern; never hold locks across `.await` points

## Debugging

```bash
RUST_LOG=debug cargo run                 # Verbose logging
RUST_LOG=pos_services=trace cargo run    # Trace a specific crate
RUST_BACKTRACE=1 cargo test              # Full backtraces on panic
RUST_BACKTRACE=full cargo test           # Even more detail
```

## Vendor Directory

`vendor/` contains bundled dependencies for offline/reproducible builds. Do not modify directly.
