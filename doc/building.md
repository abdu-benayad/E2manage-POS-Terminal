# Building the till

```bash
cargo build                    # dev
cargo run --bin e2manage-pos   # the binary this repo ships
cargo build --release          # thin LTO, ~5 min
```

## The binary needs a sibling checkout, and will not build without it

`e2manage-pos` depends on **`abdu-egui-ui`** by path:

```toml
abdu-egui-ui = { path = "../abdu-egui-ui" }
```

so it builds only where that repository sits **beside** this one:

```
Downloads/
├── abdu-egui-ui/            <- required
└── e2manage-pos-terminal/
```

Without it, `cargo build` fails at manifest resolution with `failed to load source for
dependency` before compiling a line — loud, and it names the path, which is the good failure
mode. Every other target in the workspace still builds: the library is a dependency of the root
package only, so `cargo test -p pos-services` and friends are unaffected.

**This is a path dependency because the library is `publish = false` in a separate private
repository.** It is not a preference and there is no workaround inside this repo.

**The one condition that retires it is the library publishing.** When `abdu-egui-ui` is on a
registry, this line becomes `abdu-egui-ui = "0.1"` and the sibling-checkout requirement
disappears. Nothing else retires it — not vendoring, which cannot reach an unpublished crate,
and not a git dependency, which trades this constraint for credentials on every clone.

## Why the binary lives on the root package

Decided in `egui-auth-screen` task 11. The alternative was a new crate in `[workspace] exclude`,
which loses on three counts: an excluded crate is invisible to every `--workspace` command, it
does not inherit the root `[profile.*]` table, and it needs its own `.gitignore` and
verification wiring. `crates/pos-contract` sat red through five consecutive task verifications
for the first of those reasons alone.

## `default-features = false` on `egui` and `eframe` is load-bearing

Cargo unions features across the single resolved `egui`, so defaults anywhere in the graph
switch `default_fonts` back on for **everything** — including `abdu-egui-ui`'s own compilation,
silently undoing its font decision and adding 1,414,020 bytes of faces nothing draws.

`accesskit` is taken back explicitly. Without the adapter the build constructs the egui
accessibility tree and pushes it nowhere: silent to a screen reader, and silent to this screen's
AccessKit test contract, which would keep passing.

Verify with the command, and **read it** — its absence looks identical to its passing:

```bash
cargo tree -e features -i egui | grep -c default_fonts   # must be 0
cargo tree -i egui --depth 0   | grep -c '^egui v'       # must be 1
```

Control, so the reading can come out differently: set `eframe = { version = "0.34" }` and the
first command returns **4**, naming `egui feature "default_fonts"` and attributing it to eframe.
Measured 2026-08-24.

A `0.x` version mismatch against the library's `0.34` is *not* unified by cargo and yields two
incompatible `egui` copies, which surface as type errors passing `&mut Ui` across the boundary —
so the second command is the one that catches it.

## The root package names `rusqlite` directly, and the pin is not decoration

`src/driver.rs` reports startup failures as a `thiserror` enum, and the database arm carries the
underlying error as a `#[source]` rather than a string. That means naming its type.

`pos-db` exposes `SqliteResult<T> = rusqlite::Result<T>` and re-exports no part of `rusqlite`, so
every caller that handles one of its failures has to depend on the crate itself. The root manifest
therefore declares:

```toml
rusqlite = { version = "0.32", features = ["bundled", "chrono"] }
```

**Pinned identically to `crates/pos-db/Cargo.toml`, on purpose.** `crates/pos-db/Cargo.toml:41-44`
already records the reason for its own dev-dependency copy: a bare `rusqlite = "0.32"` resolves
*without* `bundled`, and cargo unions features across one resolved version — so a mismatched
declaration does not fail, it changes what everything else links against. Verify with:

```bash
cargo tree -p e2manage-pos-terminal -i rusqlite --depth 0     # must print exactly one version
```

Measured 2026-08-24: `rusqlite v0.32.1`, one line. The control on that reading is that the command
prints every version it finds, so two would be two lines rather than a silent pick.

The alternative — a `pub use rusqlite::Error` from `pos-db` — is the better long-term shape, since
a database crate whose errors cannot be named without its implementation crate is leaking. It was
not taken here only because `pos-db` was mid-refactor in another lane at the time.

## Snapshot tests

```bash
./scripts/verify.sh snapshots        # the lane; asserts a non-zero test count
```

The lane wraps this, and the wrapper is the part that matters:

```bash
cargo test --features image-snapshots --test sign_in_both_directions -- snapshot_
```

A cargo test filter is a literal substring with no alternation, and **one that selects nothing
exits 0 and prints `ok`**. So the lane sums the `N passed` across the run and fails if it is zero;
a renamed test or a `#[cfg]` that stopped matching turns this into a green over nothing otherwise.

The feature is declared on **this** crate as well as on the library, because a feature is
selected on the crate under test: without the local declaration the command fails with *"none of
the selected packages contains these features"* and never reaches a test. It forwards to
`abdu-egui-ui-testing/image-snapshots`, which pulls the wgpu stack and needs a Vulkan rasterizer
present — lavapipe (`mesa-vulkan-drivers`) is what the committed references were rendered with.
The lane treats a missing ICD as a named skip rather than a failure; nothing else is tolerated.

### The test vocabulary is a dev-dependency, not a local helper

```toml
[dev-dependencies]
abdu-egui-ui-testing = { path = "../abdu-egui-ui/abdu-egui-ui-testing" }
```

`egui_kittest` stays declared directly beside it, because the tests call its API themselves — the
vocabulary crate deliberately wraps no query, click or key verb. What it adds is the two things
kittest cannot know: it calls `TextEngine::validate` at harness construction and panics if no face
resolved (without which a suite goes green over a screen with no visible text), and `wgpu_builder`
keeps one renderer resident, which avoids a SIGSEGV from racing the Vulkan loader's ICD unload —
the crash that otherwise gets filed as a lavapipe flake and is not one.

Its guide is `abdu-egui-ui/docs/testing-consumers.md`.

### `kittest.toml` is calibrated here, and must not be replaced with the library's

It was a byte-identical copy of the sibling's until 2026-08-25 — including its threshold of 50,
calibrated against ~300 text-heavy references, and prose citing two source files that have never
existed in this repository. This corpus has six references. Measured fresh:

| bracket | counted pixels | how |
| --- | --- | --- |
| floor | **0** | two renders byte-identical; re-comparing at `failed_pixel_count_threshold = 0` passes, so the zero is the comparator's and not `md5sum`'s |
| smallest real break | **244** | `Reading::is_rtl` forced to `false` — the layout mirror stops while the text stays Arabic. The other two RTL references moved 81,546 and 234,107; the three LTR ones correctly did not move |

Set to **20**. The floor is same-machine, same-driver, and one machine cannot sample the
cross-Mesa-version drift this knob exists to absorb — the file records that as a borrowed
assumption rather than a reading taken here.

Keep the file even though it holds one line of configuration: kittest's config search walks to the
filesystem root **with no workspace boundary**, so deleting it silently inherits whichever ancestor
directory happens to hold one, with nothing reporting which.

### Open every PNG before committing it

`UPDATE_SNAPSHOTS=1` has no oracle and prints `ok` over whatever it rendered. Both defects found on
2026-08-25 — egui's dark chrome behind light library surfaces, and a wire token (`SUPERVISOR`)
rendered under an Arabic name — were found by looking at the pictures. Neither was visible to any
assertion in the suite, and the second one the assertions actively *agreed with*.

## `Cargo.lock` is not committed, and that is a decision awaiting Abdu

The lock is still in `.gitignore`, so **neither build mode is pinned** — as
[`CLAUDE.md`](../CLAUDE.md)'s *Vendor Directory* section says today. Task 11 deliberately did not
change that: the lock pins resolution, it does not gate compilation, so the binary lands complete
without it.

Committing it is a one-line reversal of a documented repo-wide rule, which is Abdu's to make and
not a lane's. If he takes it, it is **one commit**:

```bash
git add -f Cargo.lock
# and remove the `Cargo.lock` line from .gitignore
```

and the `CLAUDE.md` passage ending *"`Cargo.lock` is gitignored, so neither mode is pinned"*
becomes false in the same instant, so amend it in that commit. Replacement wording, ready to
lift:

> - The vendored tree is an older snapshot than crates.io, so the two modes resolve to different
>   dependency versions. `Cargo.lock` is committed, which pins **both** modes to the vendored
>   snapshot — builds no longer drift between modes, and that is the point.
> - The cost is that a dependency bump stops happening by resolution and becomes a deliberate
>   two-step act: re-run `cargo vendor` **and** update the lock, in the same commit.

Verified 2026-08-24 before this was written: the on-disk lock already resolves to versions the
vendored tree holds — `serde 1.0.228`, `tokio 1.48.0`, `rusqlite 0.32.1`, `reqwest 0.12.25`,
`rust_decimal 1.39.0`, `anyhow 1.0.100`, 6 of 6 matching across 302 vendored directories — so
committing it pins the status quo and forces no rebuild.
