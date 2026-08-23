//! The consumer-driven contract the till holds the E2Manage platform to.
//!
//! # What this crate is
//!
//! It has no library code. Everything lives in `tests/contract.rs`, which uses
//! [`pact_consumer`] to declare what the till reads from the platform and writes the
//! declaration to `pacts/e2manage-pos-terminal-wadi-dms-api.json`.
//!
//! That artifact is committed here **and** copied into the platform repository, where a
//! provider verification replays it against the real POS routes and a real database. A
//! platform change that moves a shape the till reads then fails the platform's own suite,
//! in the repository making the change.
//!
//! # It is not a workspace member, on purpose
//!
//! `pact_consumer` pulls 80 crates the rest of the till does not use, including
//! `onig_sys`, which compiles Oniguruma from C source. Keeping it out of the workspace is
//! what makes `cargo test --workspace` unaffected by any of that.
//!
//! Two consequences follow and neither is a defect:
//!
//! - **This crate needs network access to build.** It is outside the vendored-offline
//!   discipline the rest of the repository follows, and `vendor/` deliberately does not
//!   carry its dependencies. Nothing in the workspace depends on `pos-contract`, so the
//!   offline build never needs them. Do not run `cargo vendor` on its account.
//! - It resolves its own dependency graph and is not covered by `cargo check --workspace`.
//!
//! # Running it
//!
//! ```text
//! cd crates/pos-contract && cargo test
//! ```
//!
//! Regeneration must be byte-stable — the artifact is reviewed as a diff, and a diff that
//! is noisy every time is a diff nobody reads:
//!
//! ```text
//! rm -rf pacts && cargo test && git diff --stat pacts/
//! ```
//!
//! # What belongs in the contract
//!
//! Only surfaces the till reads **correctly today**. `project/till/doc/till-consumer-surface-audit`
//! carries the per-endpoint verdicts. Pinning a surface the till reads wrongly would fail the
//! platform's suite for a change the platform made correctly, which is how a contract suite
//! earns a `describe.skip` — the platform has two of those already.
