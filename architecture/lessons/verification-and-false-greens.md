# Verification — what a green does not mean

Every lesson in this file is one shape: **a command that returned a plausible answer to a
question nobody had asked it.** Not a broken tool, not a flaky test. A check that ran, exited
zero, printed something reasonable, and could not have observed the thing being claimed.

The generalisation is in [`agentic-method.md`](./agentic-method.md#the-reading-must-be-able-to-come-out-differently--2026-08-23):
**before trusting a reading, ask what value would have appeared if the claim were false.** If
the answer is *the same one*, you have not measured anything. Everything below is that rule
failing in a specific place, at a specific cost.


## `--workspace` does not mean everything — 2026-08-23

`Cargo.toml:35` excludes **two** crates:

```toml
exclude = ["crates/pos-updater", "crates/pos-contract"]
```

so **no workspace command can see either of them.** Not `cargo test --workspace`, not
`cargo clippy --workspace`, not `cargo check --workspace`.

`pos-contract` was red from `040d0c1` (*"the 32 POS codes stop arriving as 'no information'"*)
and stayed invisible through **five consecutive task verifications.** Every one of those
verifications ran the command its author believed covered the tree, and every one was green.

The exclusions are deliberate and both are measured, not assumed — `pos-updater` pulls reqwest
0.11 with default features and links native-tls (system OpenSSL headers nothing else here
needs); `pact_consumer` resolves 80 crates the rest of the till does not vendor, **a 32%
increase on a 253-crate tree**, including `onig_sys`, which compiles Oniguruma from C source
(`Cargo.toml:25-35`). Keeping them out is right. Believing `--workspace` covers them is what
costs.

```bash
cd crates/pos-contract && cargo test     # after any change to ServerErrorCode,
cd crates/pos-updater  && cargo check    # RefusalDetails, or a shared DTO
```


## `-p <crate>` does not build the root package's `tests/` — fired twice

`tests/guards.rs` lives in the **root package**, and it **scans `crates/`**. So a per-crate run
can be green on a change the guards refuse — the guard is a root-package integration test, and
`-p pos-api` never compiles it.

That combination is the trap: the advice to narrow verification on a shared checkout
(*"verify what you touched"*, correct, and in CLAUDE.md for a good reason) points straight at
the one selector that cannot see the tree-wide guards.

```bash
cargo test -p e2manage-pos-terminal      # whenever you touch anything the guards inspect
```

`tests/guards.rs` is 54 KB and states its own contract at the top: it scans `crates/` and
`src/` — the shipped tree — and **there are no exemptions**, deliberately. One guard does not
scan Rust at all: `the_config_cargo_reads_by_default_needs_nothing_a_clone_lacks`
(`tests/guards.rs:982`) — see [`build-and-vendor.md`](./build-and-vendor.md).


## Do not conclude from an exit code, and never from an absence — 2026-08-23

**A run that prints nothing and exits non-zero is consistent with OOM, a bad path, and a real
failure.** Those three demand opposite responses and the exit code separates none of them.

The only reliable signal is a **plausible error *total***. A count you can sanity-check against
the size of the change is a reading that can come out differently; an exit code is not.

Measured 2026-08-23 across both repos: **three separate wrong diagnoses in one day**, each from
a check that returned a perfectly plausible answer for the wrong question. A sample of the
shapes, all real:

- `ls <script>` run from the wrong directory, with the `&&` swallowing the follow-up grep —
  one keystroke from reporting *"the script does not exist"* about a script that exists.
- `pgrep -af typecheck` **matching its own command line**, so the probe for "is anyone running
  a typecheck" answers *yes* whenever it runs. This one fired while relaying a warning about
  exactly this defect.
- Three consecutive wrong readings while testing `lane-lock`'s release-on-death: the first
  killed only the wrapper, not knowing the child inherits the fd; the second used `setsid`,
  which returned immediately, so the kill hit a dead process group. Both looked like a defect
  **in the thing under test**. Only the third, which found the real fd holders through `/proc`,
  measured anything.

That last pattern is worth naming on its own: **when a probe misfires, the most available
explanation is that the subject is broken.** Suspect the instrument first — it is the part you
just built.


## A check whose expected value came from the thing under test — measured in the platform, 2026-08-23

The sharpest instance found in either repo, recorded here because the *class* is not
TypeScript-specific.

The platform's typecheck gate parses `tsc` **stdout**. When the incremental buildinfo is valid,
`tsc --noEmit --project` prints only the files in the change set and **silently replays** the
rest — the errors are real, known, and unprinted. So the gate under-reports in exactly the
state it normally runs in.

The header of `scripts/typecheck-baseline.mjs` (line 50) asserts the opposite in as many words:

> *a warm run and a cold run report identical errors*

Nothing checks that sentence, and the script depends on it. It took **six measurements across
four sessions** to settle, because most of the obvious discriminators cannot discriminate — one
proposed check compared `BETTER` counts, and the script unions current and recorded keys before
reporting, so both hypotheses produce the same flood. What finally settled it was a set the
hypotheses disagree about: baseline files **unmodified and still on disk** (279 files, 481
errors).

Three carry-over rules:

- **Recording and checking must not be the same call.** If the value you compare against was
  produced by the run you are judging, the comparison is an identity.
- **Design the discriminator before running it**, and state what each hypothesis predicts. A
  check both hypotheses pass is a check you can skip.
- **The script was sound; its documented premise was false.** "Unguarded" and "broken" are
  different verdicts, and the strong one is easy to reach for. Three full readings by an
  independent lane (499/495/493) proved the gate itself fine.

The related trap on the same suite: `ci.sh:583` promotes a **timeout** (exit 124/137) to
`strict`, overriding a lane's advisory policy — *"a lane that never finished has no numbers to
offer"*. So an advisory lane's timeout is a **strict red**, and reading the policy table
without reading that line gives you the wrong colour for the whole run.


## The two rules a verification claim has to satisfy

1. **Name the selector's blind spot before quoting its result.** `--workspace` excludes two
   crates; `-p` excludes the root package's tests; stdout parsing excludes a silent replay.
2. **A green is a claim about what the command observed**, never about the tree. Say which one
   you mean.
