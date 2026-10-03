Title: RUNBOOK - how to start and end an mdr-kernel session
Type: Operator Runbook / Session Workflow
Context: Human-facing session startup, closeout, lock, and participant workflow guide.

# RUNBOOK - how to start and end an mdr-kernel session

> **The work is the data. The data is the work.**

Use this when you are the human operator. The loop is simple:

1. Point the model at this repo.
2. Let it run startup.
3. Let it work.
4. Have it close out before you stop.
5. Confirm `brain-doctor` is green and the repo is clean.

The repo is the shared brain. `STATE.md` is the live status. `protocol/CONTRACT.md` is the rules.

## First time only: create and verify the private memory repo

On GitHub, click **Use this template**, create a **Private** repository, then clone the new
repository you own. Do not fork the public template and do not grow memory in a direct clone of it.

The path is: **template -> private clone -> open agent -> setup -> one startup.** The easiest way
to run it is to open your AI tool in the fresh clone and hand it the first prompt from the README;
the shipped phase-zero front doors (`CLAUDE.md` / `AGENTS.md` / `GEMINI.md`) route the agent
through the same two commands below, and setup regenerates those doors into full contract doors.

`bin/setup-private.ps1` is the **only** adopter entry point (`bin/bootstrap-clone.ps1` is its
internal primitive, used directly only to repair an already-set-up clone). Setup runs once per
clone: it verifies and pins the destination, arms hooks, scaffolds the memory, checkpoints it,
regenerates doors and the node map, and runs the doctor. Then startup runs **once**:

```powershell
pwsh -NoProfile -File bin/setup-private.ps1 -Model '<your model id>'
pwsh -NoProfile -File bin/start-session.ps1  -Model '<your model id>'
```

(`-Model` is the attribution identity: an agent passes its real model id, e.g.
`-Model 'claude-opus-5'`; a human driving by hand picks any short name, e.g. `-Model 'alex'`.)

After setup, `brain-doctor` should report the verified private destination and `main`. The public
template, a public repository, a fork, a changed remote, or unverifiable GitHub visibility blocks
setup and push. Use `-LocalOnly` if no remote backup is intended.

The approval pin and hooks are deliberately clone-local. Run setup again after a re-clone or on a
second computer; it is idempotent. If doctor reports only missing scaffold files, bootstrap can
repair them without overwriting existing files.

If this clone is becoming the coordination layer for existing messy work, read
`docs/adoption-guide.md` before moving files around. Start with an inventory note in `inbox/`,
then decide what becomes `STATE.md`, `reference/`, `specs/`, `handoffs/`, or `skills/`. The
starter skill at `skills/mdr-kernel/SKILL.md` is the model-facing procedure for that adoption
work.

## The three surfaces

| Surface | What it can do | Kickoff |
|---|---|---|
| **Full participant** (any CLI coding agent: Claude Code, Codex, Gemini CLI, ...) | Read/write the repo, run scripts, commit/push | `bin/start-session.ps1` (below) |
| **Read-only local model** | Read allow-listed files, produce inbox-ready notes; never writes | Point it at `STATE.md`; it returns notes you fold in |
| **Advisory chat** (a web/app assistant, no repo access) | Reason and plan only; you paste `STATE.md` to ground it | Paste `protocol/advisory.md` as its custom instruction, then paste STATE |

## Full-participant kickoff

Run this at the start of a session:

```powershell
pwsh -NoProfile -File bin/start-session.ps1 -Model '<your model id>'
```

Expected result:

```text
STARTUP_STATUS=READY
```

The script runs the complete sequence: doctor, safe pull/rebase, then full STATE readback. The
model then reports Right now, blockers, and the top next action and waits unless you already
assigned work.

For assigned work, the model uses `STATE.md` plus `reference/INDEX.md` to pick the smallest
relevant reading slice. The generated node map checks links. If it is missing or stale,
regenerate it under your coordination lock:

```powershell
pwsh -NoProfile -File bin/update-node-map.ps1
```

When you assign work, a verified prompt hook runs recall automatically. Until the host firing
is verified, or if it reports DEGRADED, run the portable fallback before acting
(see `docs/recall-hooks.md`):

```powershell
pwsh -NoProfile -File bin/prepare-work.ps1 -Objective '<operator objective>'
```

If a gate matches, it reads the listed sources and states how the surfaced traps change the next
move. This is mandatory for recurring, stuck, same-loop, or amnesia work.

Before a full participant acquires the lock or commits from a shared clone, set the actual model
id in the session process:

```powershell
$env:BRAIN_MODEL = '<your model id>'
```

Committing by hand: set `$env:BRAIN_MODEL = '<name>'` or end the message with
`Brain-Model: <name>`. A commit with neither, or with `unknown`, is refused. `git config
brain.model` is display-only (setup does not write it); it may describe a different session.

## Close prompt

At the end of a session, have the model update `STATE.md`, append any new decisions to
`DECISIONS.md`, write a handoff in `handoffs/` plus its INDEX row, regenerate the node map if
Markdown changed, then run:

```powershell
pwsh -NoProfile -File bin/close-session.ps1 -Topic '<short-topic>' -Model '<your model id>'
```

The committing identity must be explicit — `-Model` or `$env:BRAIN_MODEL`; the clone's
`git config brain.model` is a display fallback only and is refused for lock/close claims.
That script verifies the closeout and commits with the `Brain-Model` trailer. What happens next
depends on the memory's mode: a private-remote memory re-verifies and pushes the approved
`origin/main`, releasing `STATE.lock` only after the push succeeds (a failed push keeps the lock);
a local-only memory commits and releases the lock without attempting a push, by design. See below
for the offline exception. It stages only coordination files, the current handoff, generated
maps, and paths named with `-Also` (multiple paths: `-Also 'a.md,b.md'`); unrelated files are
listed and left untouched. Inbox notes are never adopted folder-wide: name the notes this session
owns with `-Also`. Unrelated staged work blocks the close and stays staged. New `reference/*.md`
files and skills go through `bin/write-approval.ps1` first (see
`docs/adoption-guide.md`); their audit entry rides the close automatically, and the close refuses
to commit an INDEX row whose target would be left behind.

If connectivity is unavailable, and only after normal private-remote setup, make the exception
explicit:

```powershell
pwsh -NoProfile -File bin/close-session.ps1 -Topic '<short-topic>' -Model '<your model id>' -OfflineCheckpoint
```

This commits without pushing, releases the lock, and leaves a persistent warning. Startup names
the same recovery command every time until it succeeds:

```powershell
pwsh -NoProfile -File bin/sync-private.ps1
```

`sync-private` is the shared push implementation used by setup, normal close, and recovery. It
re-verifies live privacy, fetches, rebases, pushes explicit `origin/main`, and allows one bounded
retry if the remote moves during the attempt. Do not substitute a manual `git push`; the cached
destination pin is identity memory, not push authorization. A local-only memory is already offline
by design and closes normally without either command.

A clean close ends with the one-line Waffle Party authorization; offline checkpoints skip the
ceremony while a sync is pending. Diagnostics stay concise (`doctor: ok.`) so automation and
humans get the same signal.

## Lock rules

`STATE.lock` protects the shared coordination layer: `STATE.md`, `DECISIONS.md`, the indexes,
contract/front-door files, protocol scripts, and hooks. The exact enforced list is
`protocol/coordination-files.txt`.

If a model is going to edit those files, it must hold the lock:

```powershell
pwsh -NoProfile -File bin/state-lock.ps1 -Acquire -Model '<your model id>' -Objective '<short objective>'
$env:BRAIN_MODEL = '<your model id>'
$env:BRAIN_LOCK_TOKEN = '<token printed by your own acquire>'
pwsh -NoProfile -File bin/state-lock.ps1 -CheckOwner
pwsh -NoProfile -File bin/state-lock.ps1 -Release
```

Acquire is fail-closed: it refuses a config-fallback identity, and a live same-model lock refuses
a second session (token mismatch) instead of silently handing over.

If `brain-doctor` says another live session holds the lock, do not ask a second model to edit
coordination files. It can still do research or write a standalone `inbox/` note; one session
folds notes into `STATE.md` later.

A heartbeat older than two hours is a reason to investigate. It does not prove the owner died.
On your own resume, carry your original token, run `-CheckOwner`, and then `-Heartbeat`.
For another holder, establish independent abandonment evidence or obtain operator direction.
Only then recover with explicit identity and `-Release -Force -AbandonmentReason '<evidence>'`.
The tool requires a further `MDR_FORCE_LIVE_LOCK=1` override for a fresh heartbeat; that override
is never evidence by itself. Never retrieve someone else's token from the lock file.

## Decisions and upgrades

Use `bin/add-decision.ps1` for new proposals and quoted operator rulings. Work reports belong in
handoffs. The human alone performs hash-bound ratification in their own terminal. Read
`docs/decision-authority.md` before migrating an existing log: no historical entry is ratified
just because the software was upgraded, and an old entry that already quotes an operator ruling
compiles as one, so review it first. Follow `docs/upgrading.md` to preserve private memory, local
hooks and history while merging distribution changes.

## Mental model

Open the repo. Run startup. Work. Close out. Keep `STATE.md` short and current. Put session
detail in `handoffs/`, durable choices in `DECISIONS.md`, and background or advisory output in
`inbox/` until a full participant integrates it.

The worker exits. The record stays.

---

## SYSTEM NODE LINKS

Directory Path: protocol/
Next Sequential File: ../docs/adoption-guide.md
Related Workflows: CONTRACT.md, ../docs/adoption-guide.md, ../skills/mdr-kernel/SKILL.md, ../docs/brand-manual.md, bin/start-session.ps1, bin/close-session.ps1
AI Execution Prompts: CONTRACT.md
