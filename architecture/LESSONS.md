# Lessons — the index

Every gotcha this till has paid for, recorded so the next session doesn't relearn it. Honest,
including the mistakes.

**This file is the index, not the log.** It answers *which lesson exists and where it lives*;
the lesson itself — the mechanism, the measurement, the date, the code — is in the group doc
each row links to. Grouped by **how you reach for them**: the theme you are working in, not the
task that happened to surface the lesson. A hazard hit while fixing the pact that governs every
shared-checkout command lives under *Several sessions, one checkout*, because that is where the
next person will look.

**CLAUDE.md is the rule of record; these docs are the argument.** CLAUDE.md carries what an
agent needs *before* doing anything — the commands, the forbidden git operations, the
verification scoping. Everything here is the *why*: what was measured, on what date, what it
cost, and why the obvious weaker rule does not cover it. Where the two disagree, **CLAUDE.md
wins and the lesson doc needs updating.**

**Adding one:** append it to the group doc whose theme it belongs to, and add its row here. Give
the heading a date, so the anchor is stable and the measurement carries its age. If a finding is
a general mechanism that one task merely surfaced, it belongs in a theme doc — not under the
task.

**Before you add a row, read the group doc you are adding it to.** Three separate stale
instructions survived last night's protocol change because each author added their entry without
reading what it contradicted two sections above. Adding your entry is not the whole obligation.


## Several sessions, one checkout

Commands that are correct alone and silently destructive in a shared tree; how to serialise the
scarce ones. → [`lessons/shared-checkout.md`](./lessons/shared-checkout.md)

| Lesson | What it covers |
| --- | --- |
| [The index is shared state](./lessons/shared-checkout.md#the-index-is-shared-state-and-git-add-explicit-paths-does-not-protect-you--2026-08-23) | *Never `git add -A`* is **strictly weaker than the hazard**: a bare `git commit` commits the whole index, so another session's staged work rides along under your message. Measured — three file deletions belonging to another lane, by a session that followed the `add` rule exactly. `git commit --only -F msg -- <paths>`, and read the stat. Plus: the `reset --soft` recovery, and the point where it stops being the right answer (once anyone has committed on top, the work is **committed, not lost** — say whose it is and leave it). |
| [`git checkout -- <path>` is not undo](./lessons/shared-checkout.md#git-checkout----path-is-not-undo-my-change--2026-08-23) | It is *undo everyone's uncommitted work under this path*, and it prints nothing. It reappears as a **runtime error in a different session's process, in a file the reverting session never touched** — a `500` on a route that had been fine, which the session hit by it diagnosed as a peer deliberately backing out a change, and wrote that wrong conclusion into a message. Commit first, then revert to your own SHA. `git stash` and `git clean -fd` forbidden for related reasons. |
| [Contention: a name is the entire protection](./lessons/shared-checkout.md#contention-a-name-is-the-entire-protection--2026-08-24) | The social protocol (announce / run / all-clear) failed twice in one night — a point-to-point announce, and an all-clear that never came. `lane-lock` holds **`flock` on a file descriptor**, so the kernel releases it on process death including SIGKILL: there is no all-clear to forget. Three findings that cost a defect each: the resource name is validated not trusted (two names for one resource serialise **nothing** and `--status` shows both `HELD`); **a guard whose bypass is ergonomic is a speed bump with a posted detour** (so `--new-resource` does not license a Damerau-1 near-miss); and a refusal with a real false positive (`db2`) must name its escape. Lock machine-wide, names per-repo via `.lane-lock`. |
| [A failure in a crate you did not touch](./lessons/shared-checkout.md#a-failure-in-a-crate-you-did-not-touch-is-probably-not-yours) | The tree changes while you verify it. **Every reading on a shared checkout is a sample, not a fact** — `ps`, `git status`, a file read ten minutes ago, a peer's report of their own state. A reading true when taken and false when used is indistinguishable at the point of use from one that was never true. |


## Verification — what a green does not mean

Selectors with blind spots, and checks that answered a question nobody asked.
→ [`lessons/verification-and-false-greens.md`](./lessons/verification-and-false-greens.md)

| Lesson | What it covers |
| --- | --- |
| [`--workspace` does not mean everything](./lessons/verification-and-false-greens.md#--workspace-does-not-mean-everything--2026-08-23) | `Cargo.toml:35` excludes **two** crates, so no workspace command sees `pos-updater` or `pos-contract`. `pos-contract` was red from `040d0c1` through **five consecutive task verifications**, every one green. Both exclusions are correct and measured (native-tls; 80 extra crates and `onig_sys` from C source) — believing `--workspace` covers them is what costs. |
| [`-p <crate>` skips the root package's tests](./lessons/verification-and-false-greens.md#-p-crate-does-not-build-the-root-packages-tests--fired-twice) | `tests/guards.rs` is a root-package test that **scans `crates/`**, so a per-crate green can bless a change the guards refuse. Fired twice. The trap is that the correct shared-checkout advice — *verify what you touched* — points straight at the selector that cannot see the tree-wide guards. |
| [Never conclude from an exit code or an absence](./lessons/verification-and-false-greens.md#do-not-conclude-from-an-exit-code-and-never-from-an-absence--2026-08-23) | Nothing-printed + non-zero is consistent with OOM, a bad path, and a real failure. The only reliable signal is a **plausible error *total***. Three wrong diagnoses in one day, including `pgrep -af typecheck` **matching its own command line** — fired while relaying a warning about that exact defect — and three consecutive readings that blamed the subject when the instrument was the new part. |
| [A check whose expected value came from the thing under test](./lessons/verification-and-false-greens.md#a-check-whose-expected-value-came-from-the-thing-under-test--measured-in-the-platform-2026-08-23) | The sharpest instance found: `tsc` prints only the change set when the buildinfo is valid and **silently replays the rest**, while the script's own header (line 50) asserts *"a warm run and a cold run report identical errors"*. Six measurements across four sessions, because most candidate discriminators **cannot discriminate** — design what each hypothesis predicts *before* running. The script was sound and its documented premise false: **"unguarded" and "broken" are different verdicts.** Plus `ci.sh:583` — a timeout promotes an advisory lane to a **strict red**. |
| [A guard keyed on the vocabulary it surveyed](./lessons/verification-and-false-greens.md#a-guard-keyed-on-the-vocabulary-it-surveyed--2026-08-24) | The same blind-selector shape one layer in: not a build command that cannot see part of the tree, but a **scan predicate that cannot see part of its own target** — worse, because a named guard retires the question it fails to ask. A prose-branching guard keyed on `contains("4xx")`/`("5xx")` was blind to **seven of the nine** matchers in `SyncFailureType::classify`, including `"invalid"` — the exact one whose damage the rule exists for. Re-keyed on **branch shape**: 11 matches, 0 false positives, and it drops the discount-percent `contains("500")` the vocabulary version turned red on. **The vocabulary is the survey; the shape is the rule.** Includes why a mutation test must aim at the suspected blind spot, not the handled case. |


## Building — the two modes

The offline vendored tree, the config file cargo always reads, and the excluded crates.
→ [`lessons/build-and-vendor.md`](./lessons/build-and-vendor.md)

| Lesson | What it covers |
| --- | --- |
| [`.cargo/config.toml` is read on every invocation](./lessons/build-and-vendor.md#cargoconfigtoml-is-read-on-every-invocation-vendortoml-only-when-asked) | `vendor/` is **1.1 GB, gitignored, carried by no ref — a clone does not have it**, so a `[source.*]` there breaks every fresh clone *at dependency resolution, before compiling a line*. It happened. Pinned by `tests/guards.rs:982`, not by a comment, because the broken state is **invisible to everyone who already has `vendor/`** — the only reader always in the clone-shaped position is a guard. |
| [Two modes, different resolutions](./lessons/build-and-vendor.md#the-two-modes-resolve-to-different-versions-and-switching-rebuilds-everything) | The vendored tree is an older snapshot, `Cargo.lock` is gitignored, so **neither mode is pinned** and switching rebuilds the lot — in a `target/` shared with every other session. A "works on my machine" report must say which mode. Adding a dependency needs `cargo vendor` first. `git clean -fd` forbidden. |
| [The two excluded crates](./lessons/build-and-vendor.md#the-two-excluded-crates-and-why-each-is-excluded) | Why each exclusion is right (`pos-updater`: native-tls / system OpenSSL; `pos-contract`: 80 unvendored crates, **a 32% increase on a 253-crate tree**, and `onig_sys` compiling Oniguruma from C). And why `pos-contract` commits its `Cargo.lock` against the repo-wide rule — without it, a non-empty pact diff stops meaning "an expectation changed". |


## The contract against the platform

What the pact proves, what it structurally cannot, and the copy nothing performs for you.
→ [`lessons/the-platform-contract.md`](./lessons/the-platform-contract.md)

| Lesson | What it covers |
| --- | --- |
| [What a pact detects, and the two things it does not](./lessons/the-platform-contract.md#what-a-pact-detects-and-the-two-things-it-does-not) | It detects a field **moving**, never one **appearing** — so it cannot police data exposure, and buying that costs removal detection (an `eachKey` matcher disables missing-key detection at its own node). Coverage is **seven of 36 `/api/pos/*` paths, small on purpose**: a surface where both sides already disagree cannot be pinned without failing the platform's suite for a change it made correctly. |
| [Never declare an empty JSON request body](./lessons/the-platform-contract.md#never-declare-an-empty-json-request-body) | `json_body(json_pattern!({}))` **hangs verification for 30 s** reporting `error sending request`, measured twice against two databases while the same route answered in milliseconds. Declare no body at all. Worth the paragraph because the symptom reads as an unreachable provider, not as anything about the contract. |
| [Regeneration MERGES; it does not replace](./lessons/the-platform-contract.md#regeneration-merges-into-the-artifact-it-does-not-replace-it) | Byte-stability holds only while nothing changes. Editing an interaction's `description` or `given` **adds** the new form and leaves the old — seven interactions became **nine**, both stale copies looking exactly like real coverage. Delete the artifact before regenerating an edit. Guarded by a declared-count check at `tests/contract.rs:699`. |
| [Deserialise with the till's real types](./lessons/the-platform-contract.md#deserialise-with-the-tills-real-types) | Never a restatement of them: a contract test that restates the consumer's DTO records what the author believed and **tests itself**. It passes while the till fails. Plus the literal-vs-`like!` rule — branch on it, pin it; carry it, match it loosely. |
| [The manual copy, and the document of record](./lessons/the-platform-contract.md#the-copy-to-the-platform-is-manual-and-nothing-does-it-for-you) | Until the artifact is copied to the platform, **the platform verifies the till's previous expectations** — with both suites green and no signal anywhere. And: the interface is `e2manage/doc/pos-till-server-contract`, a document. **Neither side reads the other's issue board to learn a contract fact.** |


## How a wrong claim gets made, and what catches it

The agent-level failure modes — the ones that cost the most last night, none of which were code.
→ [`lessons/agentic-method.md`](./lessons/agentic-method.md)

| Lesson | What it covers |
| --- | --- |
| [The reading must be able to come out differently](./lessons/agentic-method.md#the-reading-must-be-able-to-come-out-differently--2026-08-23) | The recurring defect, in its final form: **what value would have appeared if the claim were false?** Three shapes — an ambiguous answer, a no-answer wearing an answer's clothes, and a check whose expected value came from the thing under test. Stating the rule does **not** make it fire (it didn't, while being written); what fires is a **positive control** — run the check against a case you know is positive, before spending its green. |
| [Inherited claims get less scrutiny](./lessons/agentic-method.md#inherited-claims-get-less-scrutiny-than-claims-you-make--2026-08-24) | Anything arriving ready-to-use skips the checking that producing it would have required. **Self-blame is the most trusted form of relay and therefore the least checked** — a confession was believed and acted on, and it was wrong while the conclusion it retracted was right. And **the right file is not enough when it holds several rules**: a hole was reported open an hour after its fix, by a reading that opened the correct file and the wrong region. |
| [Manufacture the checking traffic](./lessons/agentic-method.md#manufacture-the-checking-traffic--2026-08-24) | Measured across five sessions: **every wrong claim that got caught was caught by a peer's unrelated question, and none fired unprompted.** A re-read reproduces the reasoning that produced the claim, skipped step included — so the check must come from outside that head. A **fresh** subagent (a fork inherits the blind spot) pointed at **one named claim** (an open "review this" returns the agreeable summary you would have written). |
| [Retract with the evidence bar you assert with](./lessons/agentic-method.md#retract-with-the-same-evidence-bar-you-assert-with) | A real constraint was deleted from a design doc and relayed to three lanes on an argument nobody measured; a lane then hit `error[E0004]` at `refusal_details.rs:325`. The discriminator was one command (**0** catch-all arms there vs **3** in `failure.rs`), and the wrong file was measured because that is where the objector pointed. Then the correction was over-applied into a statement false the other way. **A retraction is an assertion.** |
| [Four smaller findings, each earned](./lessons/agentic-method.md#four-smaller-findings-each-earned) | **A guard written from a survey certifies the survey** — build the expected set by derivation and require set correspondence both ways, not a count. **A red lane hides a new red** as effectively as a vacuous assertion hides a defect. **Report "could not answer"** rather than the most plausible value — reached independently three times in one night, in three different currencies. **Finding what your entry contradicts** is half the obligation of adding one. |
