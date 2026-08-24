# The contract against the platform — what a pact proves, and what it cannot

`crates/pos-contract` publishes a **pact**: a machine-readable record of what this till reads
from the E2Manage API. The platform replays it against its real app and a real database, so a
change there that moves a shape this till depends on fails **that** repository's suite, in the
pull request making the change.

That is the whole value, and it is worth being precise about, because the failure mode of a
contract test is that it **looks like coverage it does not have.**


## What a pact detects, and the two things it does not

**A pact detects a field *moving*. It never detects one *appearing*.** It cannot police data
exposure — a platform response that starts including a customer's phone number passes every
interaction here, because nothing asserts absence.

Expressing absence needs a V4 `eachKey` whitelist, and **defining an each-key matcher at a node
disables missing-key detection at that same node.** So buying exposure detection costs removal
detection, which is the pact's primary job here. The trade is not worth it, and the reason it is
not is the kind of thing that has to be written down or it gets re-litigated as an oversight.

**Coverage is small on purpose.** Seven interactions against a surface of **41 distinct route
templates** — **36 under `/api/pos/`** (35 in `crates/pos-api/`, plus `/api/pos/version/check` at
`crates/pos-updater/src/version.rs:55`), **4 under `/api/carts/`**, and `/api/health`
(`client.rs:592`).

That decomposition is written down because the number is a trap. A fresh enumeration of
`pos-api` returns **35**, and the obvious next move is to "correct" the 36 — measured
2026-08-24, by an agent dispatched to verify the figure, which reported 35 with a careful
account of what its method would miss and did not name the crate that was missing.
`pos-updater` is excluded from the workspace (`Cargo.toml:35`), so it is outside every sweep
scoped to what `--workspace` covers, and its one route is a live call: `check()` builds the URL
by `format!` from `base_url` and issues it through its own `reqwest` client. The guard
`only_the_transport_crates_name_a_route` (`tests/guards.rs`) is what bounds the search — it
allows route literals in exactly `pos-api`, `pos-contract`, `pos-updater`, and matches both
`"/api/` and `"{}/api/`, so a green guard proves no fourth crate holds one.

Two independent enumerations of `pos-api` — one reading call sites, one normalising literals
and stripping query strings — returned the **same 35 paths**. The convergence is what makes the
count trustworthy; the missing crate is what makes it 36.

**And 36 was still the wrong headline, for a third reason nobody had named: it is the count under
one mount, not the till's surface.** Re-measured 2026-08-24 over **what `ApiClient` requests**, plus
`pos-updater`: **41 templates — 36 `/api/pos/`, 4 `/api/carts/`, 1 `/api/health`.** Every
enumeration above had been scoped to `/api/pos/` by the search string, so each one confirmed the
others while all of them answered a narrower question than the sentence they were quoted in.
`project/till`'s brief independently reached 41 and split it 37/4, folding `/api/health` into the
`/api/pos` bucket — two measurements agreeing on a total and disagreeing on its parts, which is the
tell that both were derived and neither was transcribed.

**The boundary has to say *calls*, not *names* — and this file got that wrong once.** "Over all
three crates the guard allows" is a different predicate with a different answer: route literals
across `pos-api`, `pos-contract` and `pos-updater`, comments stripped, come to **44**, because
`pos-contract` declares 6 templates it never requests. The number 41 was right and the predicate
printed beside it was not, which is this lesson happening to the sentence that states this lesson.

**What the shortfall is *not* attributable to.** A first correction (`af71f25`) blamed three causes.
Re-measuring 2026-08-24 leaves one:

- *A doc-comment placeholder.* `/api/pos/some-endpoint` is real — `pos-api/src/lib.rs`, the crate
  doc example — but comment-only, so it inflates a naive search and never a comment-stripped one.
- *Paths built base-URL-first.* There are exactly **two**: `format!("{}/api/pos/sync/status")`
  (`client.rs:576`) and `format!("{}/api/health")` (`:607`). A leading-quote literal search misses
  **both**, not one; only `/api/health` is *also* outside the `/api/pos` prefix. A peer's
  independent derivation enumerated `ApiClient` *methods* rather than literals and so placed
  `sync/status` inside the 35 — the same path, two predicates, two answers, and neither is wrong.
- *The prefix boundary.* This is the whole delta: widening from `/api/pos` to what `ApiClient`
  requests adds exactly 5 — the four `/api/carts/…` plus `/api/health`.

**The predicate that produced the committed 36 was never written down, so which of these actually
bit is not recoverable.** That is the argument for the rule, arriving as a demonstration of it.

The rule this leaves: **a count is only as scoped as its search string, and the search string is
usually invisible in the sentence that quotes the count.** State the predicate with the number. A surface where the two sides already disagree **cannot** be pinned without failing
the platform's suite for a change it made correctly — the pact would encode the till's bug as
the platform's obligation. Coverage grows one interaction per repaired surface, never ahead of
one.

Read `till/doc/till-consumer-surface-audit` (taskum) before assuming an endpoint works. It
carries per-endpoint verdicts — `accurate` / `drifted` / `no route` / `unverified`, measured
2026-08-23 — and several endpoints do not work.


## Four rules that are not obvious

The first three are in CLAUDE.md; all four are stated at the top of `tests/contract.rs`, which
is the file of record.

### Never declare an empty JSON request body

`json_body(json_pattern!({}))` records `"body": {}` plus a content-type header, and provider
verification then **hangs for 30 seconds and reports `error sending request`** — measured twice,
against two different databases, while the same route answered `supertest` in milliseconds.

**A route that ignores its request body must declare no body at all.** The paragraph is worth
its length because the failure gives no hint of its cause: it reads as the provider being
unreachable, not as anything about the contract.

### Regeneration MERGES into the artifact; it does not replace it

"Byte-stable regeneration" holds **only while nothing changes.** When an interaction's
`description` or its `given` changes, the writer **adds** the new form and leaves the old one
behind. Editing two interactions took the artifact from seven to **nine** — and both stale
copies looked exactly like real coverage, so the platform would have gone on verifying
expectations the till no longer has.

**Delete `pacts/e2manage-pos-terminal-wadi-dms-api.json` and re-run whenever you edit an
existing interaction.** Adding one is safe; changing one is not.

This is the sharpest instance in the repo of a growing artifact reading as growing coverage.
The guard is `the_artifact_pins_exactly_the_interactions_this_crate_declares`
(`tests/contract.rs:699`) — a count the crate declares, checked against the file, so a stale
copy reds instead of accumulating.

### Deserialise with the till's real types

`pos_api::ApiErrorResponse` and friends — **never a restatement of them.** A contract test that
restates the consumer's types records what the test author believed and tests itself. It will
pass while the till fails.

### A value the till branches on gets a literal; a value it carries gets `like!`

`error.code` deserialises into `ServerErrorCode`, so its spelling is load-bearing and pinned
exactly. Pinning a `message` would pin prose and turn every copy edit into a failed build.


## The copy to the platform is manual and nothing does it for you

After changing what the till expects:

```bash
cd crates/pos-contract && cargo test    # regenerate (delete the artifact first if you edited one)
# then copy to wadi-dms-api/src/modules/pos/__tests__/contracts/pacts/
# and let the platform's `npm run test:contracts:till` confirm it still holds
```

**Until that copy happens, the platform is verifying the till's *previous* expectations** — and
both suites are green while doing it. There is no signal anywhere that says the copy is stale.

`pos-contract` is not a workspace member, so `cargo test --workspace` never runs it. It was red
for five consecutive task verifications this way — see
[`verification-and-false-greens.md`](./verification-and-false-greens.md).


## The interface with the platform is a document, not an issue board

`e2manage/doc/pos-till-server-contract` (taskum) is the **contract of record**: what is pinned,
what is excluded and why, and every till-facing surface.

**Neither side reads the other's issue board to learn a contract fact.** An issue that changes a
till-facing surface updates that document *in the same issue*, and amends the pact interaction
if the surface is pinned. An issue is a record of work; a document is a record of truth, and a
consumer reading the wrong one is reading a snapshot of somebody's intent.
