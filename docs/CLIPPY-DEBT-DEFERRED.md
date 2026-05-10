# Clippy debt deferred for design review

Workers A–E (May 2026) brought the workspace to `cargo clippy --workspace --all-targets -- -D warnings` clean. Three sites carry `#[expect(...)]` annotations that document genuine contract-level decisions, and a few additional smells were spotted that clippy's defaults did not flag. Each entry below names the file:line, why the mechanical fix was inappropriate, and the proposed shape of the real fix.

This file exists so the next person to touch any of these sites can reach the design context without having to reconstruct it from commit history.

---

## 1. `Database::create_draft` — 8 arguments on a public API

**Location:** `crates/pos-db/src/drafts.rs:230`
**Lint:** `clippy::too_many_arguments`
**Annotation:** `#[expect(clippy::too_many_arguments, reason = "pub API across crate boundary; refactor is contract-breaking and out of scope for clippy cleanup")]`

The function takes 8 separately-named primitives (terminal id, operator, name, item rows, totals, …). Every external caller spells out all 8 positionally, which is brittle: argument-order errors compile cleanly and silently corrupt drafts.

**Proposed fix:** introduce `CreateDraftRequest` (a `pub struct` in the same module) carrying the same 8 fields by name, change the signature to `fn create_draft(&self, req: CreateDraftRequest) -> ...`, and update every caller site to a struct-literal. This is API-breaking — every crate depending on `pos-db` for draft creation must move from positional to named arguments — so it lands as one PR with a clean migration commit per caller.

**Why deferred:** the migration touches code outside the unbreak-workspace-clippy scope. It deserves its own focused PR with a written rationale at the top, not a side-effect of a clippy clean-up.

---

## 2. `ReturnService::process_return` — 8 arguments on a public API

**Location:** `crates/pos-services/src/return_service.rs:813`
**Lint:** `clippy::too_many_arguments`
**Annotation:** same `#[expect]` shape as #1

Same shape, same problem. Process-return takes (transaction id, items being returned, refund method, reason, operator, terminal, …) and every call site spells them positionally.

**Proposed fix:** `ProcessReturnRequest` struct carrying the same fields by name. Same migration pattern as #1.

**Why deferred:** same as #1. Probably should land in the same PR as #1 because both follow the identical refactor recipe and reviewers can vet one shape applied twice.

---

## 3. `EmvEvent::Approved` — large enum variant

**Location:** `crates/pos-services/src/emv_service.rs:124`
**Lint:** `clippy::large_enum_variant`
**Annotation:** `#[expect(clippy::large_enum_variant, reason = "Approved variant carries CardPaymentResult (~272 bytes); boxing requires design review of the broadcast channel and matching code paths, out of scope for clippy cleanup")]`

The enum has multiple variants; `Approved(CardPaymentResult)` is by far the largest at ~272 bytes (≈12 `Option<String>` fields). Every other variant pays the 272-byte cost too — `EmvEvent` is sized to its largest variant, which dominates `tokio::sync::broadcast::channel(16)` traffic for every subscriber.

**Proposed fix:** `Approved(Box<CardPaymentResult>)`. Costs a heap allocation on the success path (rare — once per card transaction); saves ~272 bytes per event on the common dispatch path.

**Why deferred:** every match arm in every subscriber must learn the new shape (`EmvEvent::Approved(result) => ...` becomes `EmvEvent::Approved(result) => ...` where `result: Box<...>`, which usually compiles unchanged but occasionally needs `*result` deref). The change touches code in pos-services, the binary's EMV event handlers, and tests. Worth doing, but as a focused payment-event refactor commit, not a bolt-on to clippy cleanup.

---

## 4. `Database::new(path: &PathBuf)` — same `ptr_arg` smell, not flagged

**Location:** `crates/pos-db/src/connection.rs:19`
**Lint that did not fire:** `clippy::ptr_arg` (took `&PathBuf` instead of `&Path`)

Worker D fixed the equivalent on `Database::exists` because clippy flagged that one. The `new` constructor next to it has the same smell — `&PathBuf` over-restricts callers — but clippy's `ptr_arg` lint is selective and didn't fire here.

**Proposed fix:** change `pub fn new(path: &PathBuf)` to `pub fn new(path: &Path)`. Trivial and consistent with the sibling fix. Likely no caller change needed because `&PathBuf` already auto-derefs to `&Path` at call sites.

**Why deferred:** out of "do only what clippy fires on" scope for Worker D. Worth picking up next time anyone is in `pos-db`.

---

## 5. Test-file unused-result patterns hidden behind helper macros

**Location:** several test files (Worker D did not enumerate)
**Lint that did not fire:** none — patterns are masked by `assert_*!` macros that consume the `Result`

Worker D noted this in passing: there are pre-existing test patterns where a fallible operation's error is swallowed by an assertion macro rather than propagated, e.g. `assert!(some_fallible_op().is_ok())`. These don't trigger clippy because the macro pattern is opaque, but they hide failure detail when tests regress.

**Proposed fix:** audit `tests/` for `assert!(*.is_ok())`, `assert!(*.is_err())`, etc. and replace with the more informative `.unwrap()` (in tests, `unwrap` is fine — its panic message includes the error) or with `let result = ...; assert!(result.is_ok(), "{:?}", result.err());`.

**Why deferred:** this is a test-quality initiative, not a workspace-correctness one. Belongs in a "test diagnostics improvement" PR.

---

## How to remove an entry from this file

When one of #1–#5 is fixed:

1. Land the fix in its own focused commit (or PR).
2. Remove the `#[expect(...)]` annotation if the fix targeted #1, #2, or #3 — the underlying lint should no longer fire and `#[expect]` will itself become a hard error if the lint doesn't fire (that's the point of `#[expect]` over `#[allow]`).
3. Verify `cargo clippy --workspace --all-targets -- -D warnings` still exits 0.
4. Delete the corresponding section from this doc in the same commit.
5. If this doc becomes empty, delete the doc.
