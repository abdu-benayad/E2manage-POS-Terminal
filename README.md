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
- Linux: `sudo dnf install gtk3-devel` (Fedora) or `sudo apt install libgtk-3-dev` (Ubuntu)

### Build & Run

```bash
git clone https://github.com/abdu-benayad/e2manage-pos-terminal.git
cd e2manage-pos-terminal
cargo build --release
./target/release/e2manage-pos-terminal
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
