# How a wrong claim gets made here, and what actually catches it

**This file is about the agents working in this repo, not about the repo.** It exists because
the expensive failures across two repos and five concurrent sessions were not bad code. They
were **confident, specific, false statements about the tree** — each one produced by a check
that ran, and each one surviving because the next reader had no cheaper way to doubt it than
the author had to make it.

Recorded honestly, mistakes included, because the mistakes are the content.


## The reading must be able to come out differently — 2026-08-23

The single recurring defect. Before trusting a reading, ask: **what value would have appeared
if the claim were false?** If the answer is *the same one*, nothing was measured.

Three shapes, all measured:

**An ambiguous answer.** One value, two incompatible causes. A command that prints nothing and
exits non-zero is consistent with OOM, a bad path, and a real failure — and the exit code
separates none of them.

**No-answer wearing an answer's clothes.** The check ran, returned something plausible, and
could not have observed the subject. `pgrep -af typecheck` matching **its own command line**, so
"is anyone typechecking" answers *yes* whenever asked. `ls <script>` from the wrong directory
with `&&` swallowing the follow-up. A grep for a rule that reads the right *file* and the wrong
*region*.

**A check whose expected value came from the thing under test.** The platform's typecheck gate
records and checks in the same call, so the comparison is an identity. See
[`verification-and-false-greens.md`](./verification-and-false-greens.md).

### The operational half: a positive control

Stating the rule does not make it fire — it did not, repeatedly, while it was being written
down. What fires is a mechanical habit: **run the check against a case you know is positive.**

If the probe cannot detect a defect you deliberately introduced, the probe's green means
nothing, and you now know that *before* spending the green. This is the one step that converts
"I should be careful" into a thing that either passes or fails.

Corollary, from the `lane-lock` build: **when a probe misfires, the most available explanation
is that the subject is broken.** Three consecutive readings blamed the tool; all three were the
instrument. Suspect the part you just built.


## Inherited claims get less scrutiny than claims you make — 2026-08-24

**Anything arriving ready-to-use skips the checking that producing it would have required.** A
peer's finding, a summary, a design doc's premise, a prior session's measurement — all of it
enters as a fact rather than as a claim, because the work of forming it happened elsewhere.

Two amplifiers, both measured:

**Self-blame is the most trusted form of relay, and therefore the least checked.** A lane
reporting *"I got this wrong, the real mechanism is X"* is believed immediately, because
admitting error reads as evidence of care. It is not evidence about X. One such confession was
accepted and acted on before anyone measured X — and X was wrong, while the conclusion it was
retracting had been right.

**The right file is not enough when it holds several rules.** A guard hole was reported as still
open an hour after its fix had been pushed: the reading opened the correct file, read the rule
it had been pointed at (`:83`, name-based), and reported about the file. The fix was at
`:107-108` — a structural selector, in the same file, closing the same hole. *"I checked the
file"* is not *"I checked the claim."*


## Manufacture the checking traffic — 2026-08-24

Measured across a five-session run: **every wrong claim that got caught was caught because a
peer's unrelated question sent the author back to the tree. None fired unprompted.** Not one
author re-read their own claim and found it wrong.

That is a structural result, not a discipline failure — a re-read reproduces the reasoning that
produced the claim, including the step that was skipped. So the check has to come from outside
the head that made it, and if no peer happens to ask, it has to be manufactured.

**A fresh subagent, pointed at one named claim.** Both halves are load-bearing:

- **Fresh, not a fork.** A fork inherits the blind spot along with the context.
- **One named claim, not "review this."** An open review returns the agreeable summary you
  would have written yourself. *"Verify that `X` at `path:line` does `Y`"* returns a verdict.

This is why subagent dispatch is a standing authorisation for **every** lane, not only an
orchestrator — a lane that cannot dispatch has no way to manufacture the one check that has
been observed to work.


## Retract with the same evidence bar you assert with

The most expensive single error of the run. A design doc carried a constraint about
`RefusalDetails`; a lane argued it was wrong; the constraint was **removed from the doc and
relayed to three lanes** without being measured. A lane then mutated the enum and hit
`error[E0004]` at `crates/pos-api/src/refusal_details.rs:325`. The constraint was real.

The discriminator was one command, and it still is:

```
crates/pos-api/src/refusal_details.rs   0 catch-all `_ =>` arms  ← exhaustive; a variant breaks it
crates/pos-api/src/failure.rs           3 catch-all `_ =>` arms  (:352, :827, :938)
```

**Root cause: the wrong file was measured.** `failure.rs` was checked because that is where the
objecting lane had pointed; the exhaustive `match` lives in `refusal_details.rs`. The same
right-file-wrong-region defect as above, one level up.

**Then the correction was over-applied.** The instruction went out to restore the previous
wording, which was false in the *other* direction — caught by a lane, not by the author. A
retraction is an assertion. It needs its own measurement, and so does the retraction of the
retraction.


## Four smaller findings, each earned

**A guard written from a survey certifies the survey.** If the allowlist, the roster, or the
expected set was built by *reading* while the check is run by *executing*, the guard proves the
two agree about the cases the reader found. It says nothing about the ones they missed. Build
the expected set by derivation, and require a **set correspondence in both directions** — not a
count.

**A red lane hides a new red as effectively as a vacuous assertion hides a defect.** Once a lane
is known-red, its output stops being read. Anything that breaks underneath it is free. A red
lane needs an error *total* that is watched, or it needs fixing.

**Report "could not answer" instead of manufacturing an answer.** Reached independently three
times in one night, in three different currencies — a probe, a review, and a status report. A
run that could not observe the thing has produced a result, and *"I could not measure this"* is
that result. Substituting the most plausible value is the defect, and it is invisible
downstream because plausible values look like measurements.

**Adding your entry is not the whole obligation; finding what your entry contradicts is.** Three
separate stale instructions survived a change to the protocol they described — a skill still
telling lanes to announce and send an all-clear after announcements had been replaced, in one
case in a paragraph calling the old mechanism *"the orchestrator's whole reason to exist"*. Each
was found by a different reader, none by the author who had just written the replacement two
sections above.


## What this file is not

It is not a reason to add a verification step to every claim. Most claims here are cheap and
checkable, and the fix for a wrong one is to check it. The rule that survives all of the above
is narrow:

> **Before a claim about the tree leaves your session — into a doc, a commit message, a peer, or
> Abdu — name the command that could have contradicted it.** If there isn't one, say that
> instead.
