Title: Spec — mdr-kernel architecture: shared memory across model surfaces
Type: Spec / Design Authority
Context: Design rationale for the shared-memory coordination kernel — the invariant, the surfaces, the guardrails, and the known residual failure modes. Verify live status against STATE.md and DECISIONS.md before acting.

# Spec — mdr-kernel architecture

## 1. Purpose

`mdr-kernel` is a shared-memory operating environment for AI coding agents. Its job is to let
multiple model surfaces — and the human operator — resume the same work without relying on any
model's private memory or on a chat transcript that does not survive the session.

The core invariant:

> If another model needs a fact, decision, current state, or handoff, it must live in this repo.

The repository is the memory. The chat transcript is not. Agents start each shift without trusted
private continuity, route through a small retrieval slice instead of reading broadly, confront known
traps before acting, and leave accountable handoff for the next shift.

## 2. Architecture

### 2.1 Source-of-truth surfaces

| Surface | Role | Write discipline |
|---|---|---|
| `STATE.md` | Live state of play; wins over other docs when they disagree | Single-writer under `STATE.lock` |
| `DECISIONS.md` | Append-only decision log | Newest on top; never rewrite history |
| `specs/` | Topic specs and architecture rationale | One topic per file; update `specs/INDEX.md` |
| `reference/` | Shared facts addressable by `[[wiki-links]]` | One fact per file; update `reference/INDEX.md` |
| `handoffs/` | Session narratives and evidence | One handoff per session; update `handoffs/INDEX.md` |
| `inbox/` | Atomic intake from background or advisory work | Append-only until one full session folds it into state |

Git is the transport and audit log. The expected lifecycle is:

1. Run `bin/start-session.ps1 -Model <actual-id>`, which performs the health check, a safe
   pull/rebase, and a complete `STATE.md` readback.
2. Run the active-recall gate when the operator assigns work.
3. Work.
4. Acquire `STATE.lock` before coordination edits.
5. Update state / index / handoff / decisions as needed, committing in checkpoints.
6. Commit with a `Brain-Model:` trailer, push, then release the lock.

### 2.2 Participant tiers

| Tier | Capabilities | Contract |
|---|---|---|
| Full participants | Read/write repo, run scripts, commit/push | Universal contract block generated from `protocol/CONTRACT.md` |
| Read-only local participant | Read allowlisted files; output inbox-ready notes; no write/lock/git route | Same contract, routing subset |
| Advisory participants | No file/git access; reason from pasted STATE | Advisory contract block |

The split is intentional. A surface that cannot run the scripts or hold the lock is not a full
participant; it reads current state and returns an inbox-ready note instead of editing coordination
files.

### 2.3 Contract generation

`protocol/CONTRACT.md` is the canonical source for the model-facing front doors:

- the full-participant contract → `CLAUDE.md`, `AGENTS.md`, `GEMINI.md`
- the advisory block → `protocol/advisory.md`

`bin/sync-doors.ps1` regenerates the targets. `bin/sync-doors.ps1 -Check` is used by `brain-doctor`
and pre-commit to make drift loud. Edit the contract, never the generated doors.

### 2.4 Guardrails

| Guardrail | Behavior | Residual risk |
|---|---|---|
| `bin/brain-doctor.ps1` | Checks root, cleanliness, hooks, state freshness, door drift, indexes, secret-shaped tracked content, wiki-links, inbox, and lock state | Advisory except committed-secret hazards |
| `.githooks/pre-commit` | Blocks added secret-shaped strings, front-door drift, and protected coordination commits without a matching `STATE.lock` — including deletions, renames-away, and commits from a non-canonical root | Only runs when `core.hooksPath=.githooks`; can be bypassed with `--no-verify` |
| `.githooks/prepare-commit-msg` | Adds a `Brain-Model:` trailer when missing | Falls back to `unknown` unless env/config is set |
| `STATE.lock` | Machine-local cooperative lock for coordination files; JSON body, atomic acquire, holder-verified release, opt-in token ownership, commit-refreshed heartbeat | Not a cross-machine lock; same-model same-host sessions share identity unless the acquiring session carries a per-lock token |
| `bin/start-session.ps1` | Canonical startup: doctor → clean-tree gate → pull/rebase → STATE readback | Requires PowerShell and a writable canonical clone |
| `bin/brain-recall.ps1` + `protocol/recall-gates.json` | Active recall gate: matches assigned objectives to recurrence-counted traps and required reads before an agent acts | Surfaces knowledge but cannot force good reasoning |
| `bin/bootstrap-clone.ps1` | Sets hooks, checks the canonical root, and runs the doctor for a fresh clone | Must be run once per clone |

## 3. Residual failure modes

These are the known ways a shared-memory kernel can malfunction. They are numbered R1–R13 as a
stable taxonomy; adopters inherit the same failure surface.

### R1 — Hook arming is clone-local

Git does not version-control `core.hooksPath`. A fresh clone can have the hook files present but
inactive, bypassing secret scanning, door-drift checks, lock enforcement, and commit attribution.
Mitigation: the doctor warns when `core.hooksPath` is not `.githooks`, and `bin/bootstrap-clone.ps1`
arms hooks and runs the doctor for a fresh clone.

### R2 — `STATE.lock` is cooperative, local, and easy to release incorrectly

The lock is a local JSON file (model, host, pid, timestamps, base commit, objective, token). It is
not a cross-machine lock, and token ownership only seals sessions that carry `$env:BRAIN_LOCK_TOKEN`
or pass `-Token`. Acquisition is atomic (no test-then-write race), re-acquiring your own lock
refreshes the heartbeat, and every commit the holder makes refreshes it too — so staleness is judged
by heartbeat age, not acquire age. The doctor flags a lock whose heartbeat is older than two hours as
a dead session and prints the recovery command. Mitigation: doctor reporting plus pre-commit checking
staged coordination edits against the holder.

### R3 — Canonical-root protection is asymmetric by design

Ordinary commits from a non-canonical root warn only, preserving flexibility. Staging a coordination
file when the working tree's top level differs from the declared canonical root hard-blocks. The lock
gate matches old and new paths so staged deletions and renames-away of coordination files cannot slip
past with no lock held.

### R4 — Index checks catch missing rows, not stale truth

The doctor checks that every Markdown file appears in its index and that every index row resolves to a
real file, catching renames and deletes in both directions. It does not verify that status/date cells
are current — that stays procedural: the closing session updates the row when it updates the file.

### R5 — Advisory surfaces can reason from stale pasted state

An advisory surface cannot read the repo. If the operator pastes stale STATE, the output can be
coherent but wrong. Mitigation: export state with a freshness header (date, branch, commit, dirty
flag) so pasted state is self-describing; ask for a fresh export when the header is old.

### R6 — Secret scanning is pattern-based

The pre-commit scan catches common credential shapes in tracked text. It will not catch every
provider, binary payload, private URL, or human-pasted sensitive note. Keep secret *values* out of
tracked files entirely; reference secrets by filename from an untracked location.

### R7 — Model-ID labels drift

Concrete model ids age quickly. Full participants set a process-scoped `BRAIN_MODEL` or write an
explicit `Brain-Model:` trailer; a shared `git config brain.model` is only a warned fallback because a
shared clone pinned to one model mislabels every other model's commits. The tier-detection regex is
version-generous so a newer point release still classifies correctly.

### R8 — Close-out is protocol-driven, not fully automated

The close flow asks an agent to update STATE, write a handoff, update indexes, commit, push, and
release the lock. `bin/close-session.ps1` automates the mechanical tail: it verifies the lock holder
(auto-acquiring when free), today's handoff and INDEX row, STATE freshness, and doctor hard failures;
then stages, commits with the trailer, pushes, and releases the lock only after the push succeeds. The
contract also mandates checkpoint commits mid-session, because concentrating all durable writes at
session end is exactly when token exhaustion strikes — checkpoint commits cap a dead session's loss
and double as lock heartbeats.

### R9 — STATE.md outgrows its own scannability contract

STATE mandates a ~30-second read but tends to accrete completed-item narrative that belongs in
handoffs. The doctor warns when STATE exceeds its size budget; the end-of-session step mandates
dropping completed items once a handoff or decision records them.

### R10 — Lock-scope docs and enforcement can drift apart

If the hook protects a superset of what the contract lists, an agent following the written contract
can be blocked on files it was never told are protected. `protocol/coordination-files.txt` is the
single authoritative list (self-listing); pre-commit reads it and the doctor verifies every listed
path exists, so a rename cannot silently unprotect a file.

### R11 — Same-model concurrent sessions share one lock identity

Baseline lock identity is model + host. Token sealing is opt-in: a sealed session (carrying
`$env:BRAIN_LOCK_TOKEN` or passing `-Token`) prevents another same-model same-host session from
silently re-acquiring, heartbeating, or releasing its lock, and pre-commit refuses coordination
commits when both the lock and the session expose tokens that differ. Without a token, it falls back
to model+host ownership for legacy/no-lockout behavior.

### R12 — The kernel enforces knowledge STORAGE, not knowledge USE

R1–R11 all protect that facts are correctly *stored* — locked, attributed, indexed, validated,
size-bounded. **None ensures an agent CONSULTS the relevant prior knowledge before ACTING on a
recurring problem.** Reading is discretionary, every session, fresh — so a documented trap does not
prevent re-walking into it, across sessions and across models. This is the root cause of cross-model
"amnesia."

Why the obvious fix fails: "add a rule that says read X first" is what a contract already does
(`should read` / `prefer` / `before editing`) and what already fails — a cold session treats injected
docs as background and skims them. More "remember" prose just adds another discretionary rule to the
pile.

Mitigation: the active recall gate (`bin/brain-recall.ps1` + `protocol/recall-gates.json`) matches
assigned work against recurrence-counted traps and required reads before an agent advises, edits, or
guides recovery. The full mechanism is specified in `specs/brain-active-recall.md` and maintained
through `reference/action-recall-gates.md`. The gate forces surfacing, not obedience — see R13.

### R13 — Surfacing is not obedience

The recall gate makes the missed-knowledge step visible, checkable, and cross-model, but an agent can
still read the output and reason badly. When the same trap repeats, the fix is to increment the
recurrence count with evidence and add a tighter surfacing point inside the domain tool or workflow —
not to add another private-memory note or reminder paragraph. Full closure would require a hook that
blocks the *action*, which most actions here (reasoning, guiding) are not gated by. "Impossible to
not-see, recurrence-counted, at action time" is the realistic ceiling.

## 4. Adopting this architecture

Do not fork a private workspace wholesale. `bin/bootstrap-clone.ps1` scaffolds the adopter-owned live
files (`STATE.md`, `DECISIONS.md`, the indexes, `inbox/`, and `protocol/recall-gates.json`) empty,
arms the hooks, and runs the doctor. Replace the example recall gate with your own recurring traps,
keep the recurring-work gate, and let the repo — not the transcript — become the memory.

---

## SYSTEM NODE LINKS

Directory Path: specs/
Next Sequential File: specs/INDEX.md
Related Workflows: specs/brain-active-recall.md, STATE.md, DECISIONS.md
AI Execution Prompts: protocol/CONTRACT.md, bin/update-node-map.ps1
