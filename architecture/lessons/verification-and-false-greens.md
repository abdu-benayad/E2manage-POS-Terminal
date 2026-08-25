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


## A guard keyed on the vocabulary it surveyed — 2026-08-24

Same shape as the four above, one layer in: those are about a **build selector** that cannot see
part of the tree. This is about a **scan predicate** that cannot see part of its own target — and
it is worse, because a named guard retires the question it fails to ask.

Writing `the_till_never_reads_a_refusal_out_of_prose`, I surveyed the tree for the anti-pattern,
found `contains("409")` and `contains("400")` and `contains("404")`, and keyed the guard on
`contains("4xx")` / `contains("5xx")`. It passed. It would have passed forever.

The real population in `SyncFailureType::classify` (`offline_service.rs:80-114`) is **nine**
checks, and **seven are words**:

```
95:  msg.contains("conflict")        104: msg.contains("invalid")
96:  msg.contains("already exists")  105: msg.contains("not found")
97:  msg.contains("duplicate")       106: msg.contains("400")
98:  msg.contains("409")             107: msg.contains("validation failed")
                                     108: msg.contains("malformed")
```

The digit guard was blind to seven of the nine, **including `"invalid"`** — the exact matcher
whose damage is the reason the rule exists, which classified `POS_OPERATOR_SESSION_INVALID` as a
permanent data fault and abandoned a queued sale forever. A new file could have added
`msg.contains("invalid")` the next day and the guard would have stayed green while reporting, by
its name, that the till never reads a refusal out of prose.

**The vocabulary is the survey; the shape is the rule.**

That is the actionable form of *"a guard written from a survey certifies the survey"* — it says
what to key on instead. The fix keys on the **branch shape**: a comment-stripped line that begins
`if` / `} else if` / `||` / `&&` / `return` **and** tests an error-shaped value with `.contains("`,
saying nothing about which string. Measured over `crates/` + `src/`:

| predicate | matches | false positives |
| --- | --- | --- |
| digit vocabulary (`4xx`/`5xx`) | 5 | 1 — `operator.rs:753`, a **discount percent** of 500 |
| branch shape | 11 in 3 files | 0 |

The shape predicate also fixes the false positive for free, because it distinguishes **deciding**
from **describing**: every test assertion that checks a rendered message begins with the receiver
(`refused.to_string().contains(…)`) rather than with a branch keyword. The vocabulary predicate
could not tell an assertion about a discount ceiling from a branch on an HTTP status, because at
the level it was looking, they are the same string.

### The half that only a mutation test reaches

A guard that has never been seen to fail has not been tested — but *which* mutation you choose is
the whole question here. A digit-spelled probe would have gone red against **both** versions of
this guard and proved nothing about the difference. The probe that discriminates is the one aimed
at the form the original measurement could not see:

```
contains("418")   on an error binding  -> red under both predicates   (proves nothing)
contains("teapot") on an error binding -> red only under branch shape (the whole finding)
```

**Mutate toward the blind spot you suspect, not toward the case you already handle.** The
companion lesson on this — mutation-testing a guard against the form its own measurement was blind
to — is in `till/doc/guard-tests`; this entry is the same lesson from the population side, that
one from the predicate side.

### A probe that does not fire is not automatically a guard that failed

A sixth probe here — `hardware_id.contains("418")` — did not turn the guard red, and the guard was
right: `hardware_id` is not an error. The probe was wrong. Read a non-firing mutation as a claim
about the probe first and about the guard second, or you will weaken a correct predicate to make a
bad probe pass.

## A green suite over the wrong picture — 2026-08-25

`tests/sign_in_both_directions.rs` asserted, in both reading directions, every sentence the
sign-in screen shows, every operator name in the script being read, that a refused PIN and an
undecided one share no words, and that a verification in flight offers no way out. Eight tests,
all green, all correct.

The screen was rendering **light widgets on a black panel**, and the heading was invisible on two
of the five phases. The first reference image taken showed it immediately.

**Why every assertion missed it.** They read the AccessKit tree. A panel's fill colour, a text
colour, and the contrast between them reach **no accessibility node** — so there is no query that
could have failed. This is not a gap in the tests; it is the boundary of the instrument, and the
whole reason a second layer exists.

**The mechanism, which is a live trap for any consumer of `abdu-egui-ui`.** The library keeps a
deliberate quarantine: `Environment::install` writes its own tokens and never egui's `Visuals` or
theme preference, which are documented as the host's to own. Left at the default
`EguiCoherence::Manual`, egui's preference follows the operating system and **falls back to dark
when there is no signal** — which is what a till on an embedded box is, and what a headless test
harness is. The library's own widgets were correct throughout; egui's chrome behind them was not.
The fix is one builder call, `.egui_coherence(EguiCoherence::AlignSelector)`, and the defect is
invisible without a picture.

**The second half, and the more general one.** The snapshot would have been just as blind if it
had built its own `Environment::light()`. It would then have photographed a configuration
**the binary does not use**, gone green, and certified nothing — a fixture composed from what the
app *ought* to be set to rather than from what it *is*. So the chrome moved out of `src/main.rs`
into `screen::chrome()`, one function, called by the binary and by every test. A test that
restates its subject's configuration tests itself; this is that failure wearing pixels.

**And a reference image is not self-certifying either.** `UPDATE_SNAPSHOTS=1` prints `ok` over
whatever it rendered — it has no oracle, so a cropped capture, a legibility artifact and a correct
render are the same output. The six references here were opened and looked at, which is how the
black chrome was found, and how a *second* defect was found in the same glance: the operator card
was rendering `OperatorRole::to_string()`, which is `as_wire_str` — documented in `pos-models` as
"the spelling the server and the store both use". An Arabic till was showing `SUPERVISOR` in Latin
capitals under an Arabic name. Layer 1 could not see that one either, and for a sharper reason
than the chrome: the accessibility tree carried **the string the screen actually drew**, so an
assertion about it would have agreed with the defect.

**The threshold is a property of the pictures, not of the tool.** `kittest.toml` here was a
byte-identical copy of the sibling repo's, including its 50 — calibrated against ~300 text-heavy
references — and prose citing two source files that have never existed in this repository. Measured
fresh on this corpus: floor **0** (two renders byte-identical, and re-comparing at
`failed_pixel_count_threshold = 0` passes, so the zero is the comparator's rather than
`md5sum`'s); smallest real break **244** counted pixels, from forcing `Reading::is_rtl` to `false`
so the layout mirror stops while the text stays Arabic. Set to 20. The floor is **same-machine,
same-driver** and one machine cannot sample cross-Mesa drift, so the lower bracket is recorded in
the file as an assumption borrowed from the sibling's measurement rather than a reading taken here.

**And the lane that runs it asserts a non-zero test count.** A cargo test filter is a literal
substring with no alternation, and one that selects nothing **exits 0 and prints `ok`** — the same
shape as `--workspace` silently skipping an excluded crate, one layer down. Controlled all three
ways: the real filter reports 3, a filter matching nothing is caught and exits 1, and the real
filter does not trip the check.

## The two rules a verification claim has to satisfy

1. **Name the selector's blind spot before quoting its result.** `--workspace` excludes two
   crates; `-p` excludes the root package's tests; stdout parsing excludes a silent replay; **a
   scan keyed on the vocabulary you surveyed excludes every other spelling of its own target.**
2. **A green is a claim about what the command observed**, never about the tree. Say which one
   you mean.
