# Building — the two modes, and the file cargo always reads

This repo has **two dependency resolutions**, and which one you get depends on a flag. Most of
the pain here comes from a claim that is true in one mode being stated as a fact about the repo.


## `.cargo/config.toml` is read on every invocation; `vendor.toml` only when asked

Cargo reads `.cargo/config.toml` automatically and **nothing else in that directory**. The
offline build is opted into per invocation:

```bash
cargo build --config .cargo/vendor.toml --offline
```

This split is not stylistic. `vendor/` is **1.1 GB, gitignored, and carried by no ref — a clone
does not have it.** So a registry replacement pointing at it, placed in the file cargo always
reads, breaks *every fresh clone at dependency resolution, before compiling a line.*

That happened. The fix is pinned by a test, not by a comment:
`tests/guards.rs:982::the_config_cargo_reads_by_default_needs_nothing_a_clone_lacks` fails the
build if a `[source.*]` section comes back to `.cargo/config.toml`.

**Why a test and not a review rule:** the broken state is invisible to everyone who already has
`vendor/`. The people who can see it are the ones who cannot build, and by then it is committed.
A guard is the only reader that is always in the clone-shaped position.


## The two modes resolve to different versions, and switching rebuilds everything

The vendored tree is an **older snapshot than crates.io**, so the two modes resolve different
dependency versions. `Cargo.lock` is gitignored, so **neither mode is pinned.** Consequences:

- Switching between them re-resolves and **rebuilds the lot** — in a shared `target/`, that is
  every other session's build too. This is why verification is scoped
  ([`verification-and-false-greens.md`](./verification-and-false-greens.md)) and why `cargo`
  is a declared `lane-lock` resource ([`shared-checkout.md`](./shared-checkout.md)).
- **Adding a dependency requires re-running `cargo vendor`** before the offline build sees it.
- A "works on my machine" report has to say **which mode**, or it is not a report.

`scripts/audit-vendor.py` verifies the tree against each crate's `.cargo-checksum.json`. Run it
deliberately: a warm `target/` hides a corrupt vendor tree until the next cold build, which is
the worst possible moment to discover it.

**`git clean -fd` is forbidden in this repo**, and `vendor/` is the reason — 1.1 GB, gitignored,
reproducible only by a long `cargo vendor` run, and taken by exactly that command.


## The two excluded crates, and why each is excluded

`Cargo.toml:35` — `exclude = ["crates/pos-updater", "crates/pos-contract"]`. Both exclusions are
**measured, not assumed**, and both are correct:

| Crate | Why it cannot be a workspace member |
| --- | --- |
| `pos-updater` | Pulls reqwest 0.11 with default features, so it links native-tls and needs system OpenSSL headers nothing else here requires. Builds the `pos-launcher` executable. |
| `pos-contract` | `pact_consumer` resolves **80 crates the rest of the till does not vendor — a 32% increase on a 253-crate tree** — including `onig_sys`, which compiles Oniguruma from C source. `pact_matching` and `pact_models` both depend on it and neither makes it optional. |

Excluding them keeps `cargo test --workspace` and the offline-build discipline intact. The cost
is that **no workspace command can see them**, which has already been paid once — see
[`verification-and-false-greens.md`](./verification-and-false-greens.md#--workspace-does-not-mean-everything--2026-08-23).

`pos-contract` also **commits its `Cargo.lock`, against the repo-wide rule**, because its
artifact embeds resolver versions and regeneration has to be byte-stable. That exception is
load-bearing: without it, a non-empty diff on the pact would stop meaning "an expectation
changed" — see [`the-platform-contract.md`](./the-platform-contract.md).
