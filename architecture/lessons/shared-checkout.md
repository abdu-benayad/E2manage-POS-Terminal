# Several sessions, one checkout

Every hazard here has the same shape: **a command that is correct in a single-session
checkout, and silently destructive when another session is working in the same tree.** None
of them fail loudly. Most of them surface as a symptom in *someone else's* session, in a file
you never touched, which is why they get diagnosed wrong before they get diagnosed at all.

**CLAUDE.md is the rule of record; this file is the argument.** If the two disagree, CLAUDE.md
wins and this file needs updating. What lives here is the measurement behind each rule — the
date, what actually happened, and the reasoning that says why the obvious weaker rule does not
cover it.


## The index is shared state, and `git add <explicit paths>` does not protect you — 2026-08-23

The rule everyone learns first is *never `git add -A`*. That rule is real and **strictly weaker
than the hazard.** A bare `git commit` commits **the whole index**, not the paths you added. So
anything another session has staged rides along under your message, and you will not notice,
because the command you ran was the careful one.

Measured 2026-08-23 in the sibling platform repo: a session staged three explicit paths, ran
`git commit`, and committed **three file deletions belonging to another lane.** It had followed
the `git add` rule exactly.

The fix is a pathspec on the commit itself, and `--only` is the load-bearing token — it leaves
everything else in the index staged and untouched, so the other session's work survives:

```bash
git commit --only -F msg -- crates/pos-api/src/client.rs crates/pos-api/src/failure.rs
```

**Read the resulting stat before moving on.** That is the only step that can tell you the
pathspec did what you meant.

### The recovery window is short, and closing it changes the right answer

While the bad commit is still `HEAD`, this restores it without disturbing anyone's staging:

```bash
git log -1 --format=%B > msg     # keep the message
git reset --soft HEAD~1          # HEAD back, index untouched
git commit --only -F msg -- <your paths>
```

**Once anyone has committed on top of it, this stops applying** — minutes, on a shared trunk
with concurrent writers. `reset --soft HEAD~N` then rewrites history under everyone still
working in the checkout, to fix *an attribution error in a commit message.* Do not.

At that point the swept work is **committed, not lost**; only the authorship and the message
are wrong. Say so, name the commit so its owner can find their work, and leave it. Measured
2026-08-23: one such commit sits four back with three sessions' work on top of it.

**This is timing, not discipline.** The same commands are clean whenever nothing else happens
to be staged — which is exactly why the habit survives review and bites later.

### An untracked file has no pathspec to commit against

`git commit --only -- .lane-lock` on a new file fails with *"pathspec did not match any
file(s) known to git"*, which reads like a typo and is not. A new file needs `git add <path>`
first — an explicit path, never `-A`, never `.` — and then the `--only` commit. Hit
2026-08-24 adding `.lane-lock` to both repos.


## `git checkout -- <path>` is not "undo my change" — 2026-08-23

It is **"undo everyone's uncommitted work under this path."** That makes it worse than the
staging hazard, and worse in a specific way: **how it surfaces.**

It prints nothing. It reappears as a **runtime error in a different session's process, in a
file the reverting session never touched.**

Measured 2026-08-23 in the sibling platform repo: a session ran a tree-wide probe, reverted it
with `git checkout -- src`, and took another session's unstaged edit with it. There, a constant
vanished mid-edit and the app began answering `500 Cannot read properties of undefined` on a
route that had been fine. The session hit by it diagnosed *a peer deliberately backing out a
change*, and wrote that wrong conclusion into a message to the group.

**To undo your own work on a shared checkout: commit first, then revert to your own SHA.** That
scopes the undo to what you actually changed, and stamps ownership on it. Copy-then-restore
also works, but it fails silently into a *stale* version — the one failure mode nothing catches.

`git stash` is forbidden here for the same reason (it takes the whole tree, not your part of
it), and so is `git clean -fd` — see [`build-and-vendor.md`](./build-and-vendor.md), where
`vendor/` is 1.1 GB, gitignored, and carried by no ref.


## Contention: a name is the entire protection — 2026-08-24

The protocol before this was social: announce in the channel, run, send an all-clear. It failed
twice in one night in the same way — **an announce sent point-to-point instead of broadcast,
and an all-clear that never came, leaving three lanes blocked on a lock nobody held.**

The replacement is `lane-lock` (`~/.claude/skills/lanes/lane-lock`, on `PATH`), and the reason
it is not just a tidier convention is one property: **it holds `flock` on a file descriptor, so
the kernel releases it when the process dies** — SIGKILL included. There is no all-clear to
send, and therefore no all-clear to forget. The child process inherits the fd, so the lock is
held for the duration of the *work*, not of the wrapper.

```bash
lane-lock cargo -- cargo test -p pos-services
lane-lock --status
```

This repo declares its contended resources in `.lane-lock` at the root (`cargo`, `db`).

### Three findings from building it, each of which cost a defect

**The resource name is the whole protection, so it is validated rather than trusted.** Two
lanes that both "use lane-lock" under different names for one 8 GB resource serialise
**nothing** — and `--status` shows both as `HELD`, which is visually identical to real
protection. A lane typing `typecheck` instead of `tsc` gets a private lock, a clean exit, and
an OOM. The check **refuses** rather than warns: a warning in a lane's tool output is a thing
the next caller skips, which is the same reasoning that produced a tool instead of a convention.

**A guard whose bypass is ergonomic is not a guard, it is a speed bump with a posted detour.**
The first fix added `--new-resource` as the escape, which recreates the defect exactly: habituate
to typing it, and `lane-lock tcs --new-resource -- …` runs concurrently with a real `tsc` holder.
So the flag does **not** license a near miss — a genuinely new resource is never one keystroke
from an existing one, which makes the extra strictness free. Distance is Damerau, because
`tcs`/`tsc` is a transposition and transpositions are what typos are.

**A refusal with a genuine false positive must name its escape or it reads as a dead end.** A
second instance of a real resource is *exactly* one keystroke from it — `db2` — and that is the
normal way people name one. `db2` cannot be told from a typo; `db-pact` can. The message says
so. Deliberately **not** a `--force` flag: that is the ergonomic bypass again, one layer up.

### The lock is machine-wide; only the names are per-repo

8 GB of compiler is 8 GB whichever repo asked for it, so two repos both naming a resource
`build` **should** contend. That makes a name a claim about a machine-scarce resource, not a
repo-local label. Scope a name to a repo only when the resource genuinely is (`db-till`), never
to skip a queue.

The set of legal names comes from `.lane-lock` at the git root, or `$LANE_LOCK_RESOURCES`.
A repo with neither falls back to a built-in list — that is a fallback, not a recommendation,
because a repo whose real resources are all unlisted is precisely where the typo guard becomes
theatre.


## A failure in a crate you did not touch is probably not yours

The tree changes while you verify it. Before investigating, re-run, `stat -c %y` the file, and
check `git status` for another lane's in-flight work.

The general form: **every reading taken on a shared checkout is a sample, not a fact.** `ps`
output, `git status`, a file you read ten minutes ago, and a peer's report of their own state
all go stale between the moment they are taken and the moment they are used. A reading that was
true when taken and false when used is indistinguishable, at the point of use, from a reading
that was never true. Timestamp what you rely on, or re-take it.

See [`agentic-method.md`](./agentic-method.md) for the wider class this belongs to.
