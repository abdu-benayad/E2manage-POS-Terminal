# CLAUDE.md

E2Manage POS Terminal — offline-first Point of Sale built with **Rust 1.92** (edition 2021).
The package currently ships no binary; the egui view layer is tracked by the
`egui-auth-screen` issue.
Connects to E2Manage ERP backend via REST APIs, stores data locally in SQLite, supports Arabic/RTL.

**[`architecture/LESSONS.md`](architecture/LESSONS.md) is required reading before you
verify anything, run a git command in this checkout, or touch the pact.** This file carries
the rules; the lessons carry what each one cost and why the weaker version of it does not
work. Five themes: several sessions one checkout, what a green does not mean, the two build
modes, the platform contract, and how a wrong claim gets made here. Scan the index, then read
the group doc for the theme you are working in.

---

## Commands

```bash
cargo build                              # Dev build (mold linker)
cargo build --release                    # Release (~5 min, thin LTO)
cargo build --profile release-prod       # Production (~40+ min, full LTO)
# (no `cargo run`: the package has no binary target until `egui-auth-screen` lands)
cargo check                              # Type check only
cargo test                               # All tests
cargo test --test cart_tests             # Specific test file
cargo test test_add_item_to_cart         # Single test by name
cargo test -- --nocapture                # Tests with stdout
cargo fmt                                # Format
cargo clippy                             # Lint

# Offline build from the vendored tree (see Vendor Directory below)
cargo build --config .cargo/vendor.toml --offline

# Regenerate the pact this till publishes (see The contract against the platform)
cd crates/pos-contract && cargo test
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

`crates/pos-updater` is a seventh directory that is **not** a workspace member —
see `exclude` in the root `Cargo.toml`. It builds the `pos-launcher` executable,
and it pulls reqwest 0.11 with default features, so it links native-tls and needs
system OpenSSL headers nothing else here requires. `cargo check --workspace` does
not cover it; build and check it with `cd crates/pos-updater && cargo check`.

`crates/pos-contract` is an eighth directory and also **not** a workspace member,
for the same kind of reason: `pact_consumer` pulls `onig`/`onig_sys`, which needs
a C toolchain and is not in the vendored tree. Excluding it keeps
`cargo test --workspace` and the offline build discipline untouched. See
**The contract against the platform** below.

### Root package (`src/`)

- `lib.rs` - Re-exports from workspace crates for convenience
- `ui/` - view-model bridges: flatten service types into render-ready shapes.
  These hold no toolkit dependency and must not acquire one — the view layer
  imports them, never the reverse.
- `hardware/` - Hardware abstraction (printers, scanners)
- `utils/` - Utility functions

There is no `[[bin]]` target: `src/main.rs`, `build.rs` and the old `ui/` tree went
with the previous view layer. Do not add a UI dependency here without the
`egui-auth-screen` issue.

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

## The contract against the platform

`crates/pos-contract` publishes a **pact** — a machine-readable record of what this
till reads from the E2Manage API. The platform replays it against its real app and
a real database, so a change there that moves a shape this till depends on fails
*that* repository's suite, in the pull request making the change.

It pins **seven interactions**. The till calls **41 distinct route templates**, measured
2026-08-24 across the three crates the route guard allows: **36 under `/api/pos/`** (35 in
`pos-api`, plus `/api/pos/version/check` in the excluded `pos-updater`), **4 under
`/api/carts/`**, and `/api/health` (`client.rs:592`). Two traps in that number — a fresh count
of `pos-api` alone returns 35 and looks like a correction; and counting only `/api/pos/`
understates the till's surface by five, because `/api/carts/*` is a different mount the till
also calls. The seven are: the
nested error envelope, two terminal-auth refusals, the pairing-request 200, the
pairing-status miss, and the login / heartbeat / token-refresh trio.
Coverage is small on purpose — a surface where the two sides already disagree
cannot be pinned without failing the platform's suite for a change it made
correctly. Coverage grows one interaction per repaired surface.

### Working with it

```bash
cd crates/pos-contract
cargo test                    # regenerates pacts/e2manage-pos-terminal-wadi-dms-api.json
```

Regeneration is **byte-stable only while nothing changes.** `pact_consumer`
**merges** into the artifact on disk rather than replacing it, keyed on
`(description, providerState)` — so editing an existing interaction's description
or `given` leaves the old one behind and the count silently grows. Adding a new
interaction is safe; changing one is not. **`rm crates/pos-contract/pacts/*.json`
before `cargo test` whenever you edit an existing interaction**, then count the
`interactions` array out of the JSON rather than trusting the green.

**The copy to the platform is still a deliberate act, but it is no longer an
unaided one.** `scripts/till-pact.mjs` at the platform monorepo root: `sync`
performs the copy, `check` detects the forgetting via `pact-provenance.json`.
Neither runs on its own, and `ci.sh` renders `check`'s exit 3 as "could not
check" — never green, never gating. So until someone runs `sync`, the platform is
verifying the till's *previous* expectations, and nothing goes red to say so.

**Do not copy an interaction the platform has not satisfied.** An expectation it
has not met fails *that* repository's suite for a change it made correctly, which
is the one thing this contract must never do. The way to find out without
committing anything: ask a platform lane to drop the artifact in **locally and
uncommitted** and run `npm run test:contracts:till`. Their verifier also refuses
any artifact naming a `given` with no handler in `provider-states.ts`
(`till.provider.pact.e2e.test.ts:114-123`) — loudly, which is the good failure
mode, but it means a new provider state is a platform-side change and not
something the till can land alone.

### Three rules that are not obvious

- **Never declare an empty JSON request body in an interaction.**
  `json_body(json_pattern!({}))` records `"body": {}` plus a content-type and
  deadlocks verification for 30 s with "error sending request", while the same
  route answers in milliseconds otherwise. There is a note at the top of
  `tests/contract.rs`.
- **Deserialise with the till's real types** (`pos_api::ApiErrorResponse` and
  friends), never a restatement of them. A contract test that restates the
  consumer's types tests itself.
- **A pact detects a field *moving*, never one *appearing*.** It cannot police data
  exposure. Expressing absence needs a V4 `eachKey` whitelist, and defining an
  each-key matcher at a node disables missing-key detection at that same node —
  trading away removal detection, which is the pact's primary job here.

### The interface with the platform is a document, not an issue board

`e2manage/doc/pos-till-server-contract` (taskum) is the contract of record: what is
pinned, what is excluded and why, and every till-facing surface. **Neither side
reads the other's issue board to learn a contract fact.** An issue that changes a
till-facing surface updates that document in the same issue, and amends the pact
interaction if the surface is pinned.

**And that rule assumes the two records disagree when one is wrong. They can agree and both be
wrong.** Measured 2026-08-24: the till's design doc claimed the pairing-*request* 200 carries an
additive `kind: "RE_PAIR"`; `pos-till-server-contract` v36 claimed the same route returns
`isRePair`. Two spellings of one false belief — that the *request* response discriminates a re-pair
— each sitting in its own side's record of authority. Neither side could catch the other by reading
across, because each was reading a different spelling of its own error, and the agreement read as
corroboration. What settled it was neither document: opening `pairing.handler.ts`, reading the
declared result type, and counting the `return` statements. The response is byte-identical for a
first enrolment and a re-pair; the real flag is on the pairing-*status* response.

> **Two records agreeing is not corroboration when both were written from the same belief rather
> than from the source.** Convergence is evidence only when the derivations are independent, and two
> documents written by people talking to each other are not.

`till/doc/till-consumer-surface-audit` (taskum) carries the per-endpoint verdicts —
`accurate` / `drifted` / `no route` / `unverified` — measured 2026-08-23. Read it
before assuming an endpoint works: several do not, and the open ones are on the
till roadmap.

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
RUST_LOG=debug cargo test                # Verbose logging
RUST_LOG=pos_services=trace cargo test   # Trace a specific crate
RUST_BACKTRACE=1 cargo test              # Full backtraces on panic
RUST_BACKTRACE=full cargo test           # Even more detail
```

## Committing — the index is shared with other sessions

**Several Claude sessions share this checkout, and `git add <explicit paths>` does not protect
you.** Never `git add -A`/`.` — but that rule is strictly weaker than the hazard. **The index is
shared state.** A bare `git commit` commits *the whole index*, not the paths you added, so anything
another session has staged rides along under your message.

Measured 2026-08-23 in the sibling platform repo: a session staged three explicit paths, ran
`git commit`, and committed three file deletions belonging to another lane. It had followed the
`git add` rule exactly.

**Commit with a pathspec, always, and read the resulting stat before moving on:**

```bash
git commit --only -F msg -- crates/pos-api/src/client.rs crates/pos-api/src/failure.rs
```

`--only` is load-bearing: it leaves everything else in the index staged and untouched, so another
session's work survives.

**Name files, never directories — `--only` with a directory pathspec silently drops new files.**
Measured 2026-08-24 in a throwaway repo, all three cases:

| pathspec | new untracked file | result |
| --- | --- | --- |
| `-- src` (a directory) | **omitted** | exit 0, stat says "1 file changed", no warning |
| `-- src/new.txt` (explicit) | — | **refuses the whole commit**: `pathspec … did not match any file(s) known to git` |
| `git add src/new.txt` first, then explicit paths | committed | correct |

The flag protects you exactly when you name files and fails you exactly when you name a directory,
and the directory failure is the silent one. A platform lane lost a 260-line property test to it on
2026-08-24: the production change and three modified tests landed, both *new* test files did not,
and the only tell was a stat printing 6 files where 8 were intended — the evidence for the fix,
missing from the commit that claimed it. **`git add` every new file before committing it, and read
the stat's file count against the number you meant.**

**Row 2 is not a safety net for row 1 — they are disjoint.** A pathspec mixing an explicit new-file
path with a directory takes the refusal from the *explicit* one, which names only that file. The
author `git add`s it, re-runs, and the commit succeeds — while the directory silently keeps dropping
everything else new underneath it. Measured: `-- explicit_new.txt dir` refuses naming only
`explicit_new.txt`; after adding that one, the commit lands 2 files and leaves `dir/dir_new.txt`
untracked with no warning. The loud failure and the silent one do not overlap, so surviving a
refusal tells you nothing about what a directory in the same pathspec left behind.

Recovering after the fact, without disturbing their staging:

```bash
git log -1 --format=%B > msg     # keep the message
git reset --soft HEAD~1          # HEAD back, index untouched
git commit --only -F msg -- <your paths>
```

**The recovery above assumes the bad commit is HEAD, and that window is short.** Once anyone has
committed on top of it — minutes, on a shared trunk with concurrent writers — it stops applying:
`reset --soft HEAD~N` rewrites history under everyone still working in the checkout, to fix an
attribution error in a message. Do not. At that point the swept work is **committed, not lost**;
only the authorship and the message are wrong. Say so, name the commit so its owner can find their
work, and leave it. Measured 2026-08-23: one such commit sits four back with three sessions' work
on top of it.

This is timing, not discipline — the same commands are clean whenever nothing else happens to be
staged, which is why it survives review and bites later.

## Verifying while other sessions edit the same tree

**Verify what you touched.** `cargo test --workspace` and `cargo clippy --workspace -- -D warnings`
share one `target/` with every other session here, and switching between the vendored and crates.io
modes re-resolves and rebuilds the lot (see Vendor Directory). Prefer `cargo test -p pos-services`,
`cargo clippy -p pos-api`. The full sweep runs once, against a known tree, before a push — and push
is Abdu's lever, so that gate already exists.

**Two holes in that rule, both measured, both the "the command cannot observe what I am claiming"
shape:**

- **`--workspace` does not mean everything.** `Cargo.toml:35` excludes `crates/pos-updater` **and
  `crates/pos-contract`**, so no workspace command can see either. `pos-contract` was red from
  `040d0c1` and stayed invisible through **five consecutive task verifications**. Any change to
  `ServerErrorCode`, `RefusalDetails`, or a shared DTO must also run
  `cd crates/pos-contract && cargo test`.
- **`-p <crate>` does not build the root package's `tests/`**, which is where `tests/guards.rs`
  lives — and `guards.rs` scans `crates/`. So a green per-crate run can bless a change the guards
  refuse. That has fired twice. Run `cargo test -p e2manage-pos-terminal` too when you touch
  anything the guards inspect.

**A failure in a crate you did not touch is probably not yours.** The tree changes while you verify
it. Re-run and `stat -c %y` the file before investigating, and check `git status` for another lane's
in-flight work.

**Do not conclude from an exit code or an absence.** A run that prints nothing and exits non-zero is
consistent with OOM, a bad path, and a real failure. The only reliable signal is a plausible error
*total*. Measured 2026-08-23 across both repos: three separate wrong diagnoses in one day, each from
a check that returned a plausible answer for the wrong question.

Cross-repo, the broker that would make this mechanical is
`e2manage/issue/every-session-runs-the-whole-suite-against-a-tree-others-are-editing`. **`git stash` is forbidden here for the
same reason**, and so is `git clean -fd` (see Vendor Directory below).

**`git checkout -- <path>` is forbidden for a stronger reason: it is not "undo my change", it is
"undo everyone's uncommitted work under this path".** Measured 2026-08-23 in the sibling platform
repo: a session ran a tree-wide probe, reverted it with `git checkout -- src`, and took another
session's unstaged edit with it. What makes this worse than the staging hazard is how it surfaces.
It does not print anything. It reappears as a **runtime error in a different session's process, in
a file the reverting session never touched** — there, a constant vanished mid-edit and the app
began answering `500 Cannot read properties of undefined` on a route that had been fine. The
session hit by it diagnosed a peer deliberately backing out a change, and wrote that wrong
conclusion into a message.

**To undo your own work on a shared checkout: commit first, then revert to your own SHA.** That
scopes the undo to what you actually changed and stamps ownership on it. Copy-then-restore also
works but fails silently into a *stale* version, which is the one failure mode nothing catches.

## Vendor Directory

`vendor/` contains bundled crate sources for offline builds. It is **531 MB across 302 crate
directories** (re-measured 2026-08-24; the long-standing 1.1 GB figure is stale), produced by
`cargo vendor`, gitignored, and carried by no ref — **a clone does not have it**. Do not
modify it directly; `scripts/audit-vendor.py` verifies it against each crate's
`.cargo-checksum.json`, which a warm `target/` otherwise hides until the next cold build.

Because a clone does not have it, the registry replacement that points at it lives in
`.cargo/vendor.toml`, which cargo reads **only when asked**:

```bash
cargo build --config .cargo/vendor.toml --offline
```

`.cargo/config.toml` — the file cargo reads on every invocation, including a fresh clone's —
carries build settings only, and must never gain a `[source.*]` section. It did carry one, and
every clone failed at dependency resolution before compiling a line.
`tests/guards.rs::the_config_cargo_reads_by_default_needs_nothing_a_clone_lacks` fails the build
if it comes back.

Two consequences worth knowing:

- The vendored tree is an older snapshot than crates.io, so the two modes resolve to different
  dependency versions and switching between them re-resolves and rebuilds. `Cargo.lock` is
  gitignored, so neither mode is pinned.
- Adding a dependency requires re-running `cargo vendor` before the offline build sees it.
