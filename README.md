# E2Manage POS Terminal

Open-source Point of Sale terminal built with **Rust**.

Works with [E2Manage ERP](https://jooher.app) backend via HTTP APIs.

## Features

- **Offline-first** - Works without internet, syncs when connected
- **Multi-business type** - Restaurant, Retail, Supermarket, Distributor
- **JSON-driven UI** - Screens defined by backend, no app update needed
- **RTL support** - Full Arabic/Hebrew support
- **Lightweight** - Native binary, no browser runtime
- **Cross-platform** - Windows, Linux, embedded devices

## Architecture

```
┌─────────────────────────────────────┐
│  E2Manage POS Terminal (this app)   │
│  - Rust                             │
│  - Local SQLite cache               │
│  - HTTP polling (10 min interval)   │
└──────────────┬──────────────────────┘
               │ REST APIs
               ▼
┌─────────────────────────────────────┐
│  E2Manage Backend                   │
│  - Node.js + Express + Prisma       │
│  - PostgreSQL                       │
└─────────────────────────────────────┘
```

## Quick Start

### Prerequisites

- Rust 1.70+
- `clang` and `mold` — `.cargo/config.toml` selects them as the linker, so a
  build fails at the first build script without them.
  `sudo dnf install clang mold` (Fedora) or `sudo apt install clang mold` (Ubuntu)

No GTK or other GUI library is needed: the Slint view layer was removed and its
egui replacement has not landed yet, so nothing in the dependency graph draws.

### Build

```bash
git clone https://github.com/abdu-benayad/e2manage-pos-terminal.git
cd e2manage-pos-terminal
cargo build --release
cargo test
```

There is no POS binary yet — the terminal's entry point goes away with the view
layer and returns with the egui rewrite. The one executable the workspace builds
today is `pos-launcher`, the auto-updater in `crates/pos-updater`.

### Building offline

`vendor/` holds every crate source for network-free builds. It is not in the
repository (1.1 GB, produced by `cargo vendor`), and the registry replacement
that points at it is opt-in so that a clone without it still builds:

```bash
cargo vendor                                        # once, with network
cargo build --config .cargo/vendor.toml --offline   # thereafter, without
```

## API Endpoints

| Endpoint | Purpose |
|----------|---------|
| `GET /api/pos/screens` | UI screen definitions |
| `GET /api/tenant-preferences` | Config & settings |
| `GET /api/inventory/products` | Product catalog |
| `POST /api/pricing/calculate` | Real-time pricing |
| `POST /api/pos/transactions` | Submit sales |
| `POST /api/pos/sync/upload` | Sync offline queue |

## License

MIT License - see [LICENSE](LICENSE)
