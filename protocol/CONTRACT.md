Title: CONTRACT - the canonical mdr-kernel contract
Type: Canonical Contract / Model Door Source
Context: Canonical contract source that generates the model front doors and defines participation rules.

# CONTRACT - the canonical mdr-kernel contract

_Status: active - This file is the **single source**. The contract block between the markers below
is generated into the model-facing doors (`CLAUDE.md`, `AGENTS.md`, `GEMINI.md`) by
`bin/sync-doors.ps1`. **Edit it here, then run sync.** A pre-commit check blocks commits where a
door has drifted from this source._

<!-- BEGIN BRAIN-CONTRACT v1 -->
<!-- Canonical source: protocol/CONTRACT.md. In the front-door files this block is generated -
     edit the source and run bin/sync-doors.ps1; do not edit the generated copies. -->

## mdr-kernel contract - every model follows this

You are working in a shared project/engineering memory. Every AI agent - Claude, Codex, Gemini,
a local model - obeys this one contract. **The repo is the only shared brain: anything another
model needs is a file here, never in a model's private memory.**

**Start of session - one canonical sequence for every full participant:**
1. Run `pwsh -NoProfile -File bin/start-session.ps1 -Model '<actual model id>'`. This runs
   `brain-doctor`, refuses unsafe pulls from a dirty tree, runs `git pull --rebase`, then prints
   the complete current `STATE.md`. Do not recreate these steps from memory.
2. Resolve every `NOTE=` and require `STARTUP_STATUS=READY` before writing. If `STATE.lock` is held
   by another session, **do not edit coordination files** until the lock is free or you hold it.
3. Treat the printed `STATE.md` as live truth. If any other doc disagrees, **STATE wins**; then fix
   the other doc. For assigned work, use `STATE.md` plus `reference/INDEX.md` to choose
   the smallest relevant file set. The generated `maps/NODE-MAP.md` checks link health; it is
   not the reading router. Read that slice first, state what you selected and why, and say what
   you are intentionally not reading yet. Expand only when evidence shows the first slice is insufficient.
   Report Right now / blockers / top next action and wait unless work was assigned.

No PowerShell or no writable clone means the surface is not a full participant: read current
`STATE.md` (created by `bin/setup-private.ps1` on the repo's first run) and return an
inbox-ready note instead of writing.

**While working:**
- **Run the active-recall gate before acting on assigned work.** When the operator assigns an
  objective, a configured prompt hook runs `bin/hook-recall.ps1`. Without a verified firing
  hook, run `bin/prepare-work.ps1 -Objective '<operator objective>'` before implementation or
  advice; both use `bin/brain-recall.ps1`. Hook configuration and passing adapter tests do not
  prove a host fired it. A degraded hook requires the manual fallback before acting. If gates
  match, read the required sources, state the matched traps and recurrence counts, and say how
  they change the next action. Always run it when the operator says
  recurring/stuck/same-loop/amnesia. Do not replace this with a memory note.
- **Before writing code, walk the least-code ladder.** Stop and check, in order: does this need to
  exist at all? -> is it already in the codebase (grep first)? -> does the stdlib or platform
  already do it? -> does an installed dependency? -> is it one line? -> only then write the minimum
  that works. The best code is the code you didn't write - but trust-boundary validation, data-loss
  handling, security, and accessibility are **never** on the chopping block.
- **Keep context spend bounded.** Startup already printed `STATE.md` - do not re-read it unless
  git changed it or you need one precise section. Keep one coherent workstream per conversation;
  start fresh for unrelated objectives. Search narrow paths first and keep command output concise;
  expand deliberately only when a narrow excerpt would hide the cause. Never read or dump a whole
  append-only log (`DECISIONS.md`, `handoffs/INDEX.md`) or a large command log into the
  conversation: grep for your domain terms plus the newest entries, and write bulky evidence to a
  file, quoting only the relevant excerpt. Lifecycle scripts print everything they verified - do
  not re-run them to re-learn what they already told you.
- **Only the operator's rulings bind.** Append real decisions with `bin/add-decision.ps1`;
  newest on top, never rewrite history. Each entry has `**Authority:** proposed` or
  `**Authority:** operator-ruled` with the operator's own words and surface/date. Proposals are
  not rules. Work reports belong in handoffs. Replacement uses explicit `Supersedes`,
  `Supersedes in part`, or `Retires` stamps; a proposal cannot replace a ruling.
  The human alone runs `bin/ratify-decision.ps1` in their own terminal. Agents never drive
  that command or write its hash-bound ledger. Compiled `protocol/rulings.json` feeds recall;
  never hand-edit it. Full rules and required sources must be read when recalled. Existing
  histories need the explicit migration in `docs/decision-authority.md`; upgrades ratify nothing.
- **Identify the actual committing model.** Set `BRAIN_MODEL` in the session process before
  acquiring `STATE.lock` or committing, or write the explicit `Brain-Model:` trailer. The
  commit-message hook refuses a commit with neither.
  `git config brain.model` is display-only and never proves session identity or lock ownership.
- **Prefer appending over rewriting.** Background or high-volume work writes atomic notes into
  `inbox/` instead of editing `STATE.md`; one session folds `inbox/` into STATE.
- **Commit in checkpoints, not only at close.** When a unit of work lands (a fix, a script, a doc),
  commit and push it then - under the lock if coordination files are staged. Small pushed commits
  cap the loss if a session dies mid-flight, and every commit under `STATE.lock` refreshes its
  heartbeat.
- **`STATE.lock` is a coordination lock, not decoration.** Before editing any coordination file
  (STATE, DECISIONS, the indexes, contract/doors, protocol scripts and hooks - the authoritative
  list is `protocol/coordination-files.txt`, which pre-commit enforces), run
  `bin/state-lock.ps1 -Acquire -Model '<model-id>' -Objective '<short objective>'` and respect the
  result. Acquire is fail-closed: it refuses a config-fallback identity, and a live same-model
  lock refuses a second session instead of silently handing over its token. Carry the token
  printed by your own successful acquire as `BRAIN_LOCK_TOKEN` in every tool shell. Resume with
  `-CheckOwner`, then heartbeat; never copy a token from the lock file. If another
  live session holds the lock, do not touch those files; put your result in a uniquely named
  handoff or `inbox/` note and retry later, or use `-Acquire -Wait`. A heartbeat older than two
  hours is stale, not proof of abandonment. Establish abandonment independently or obtain
  operator direction before forced recovery; record that evidence with `-AbandonmentReason`.
  `STATE.lock` is git-ignored and machine-local: it only serializes sessions on the same clone.
  Two clones (two machines, or a second checkout) never share a live lock - they coordinate only
  through git push/pull, so keep sessions against the same clone sequential when they might touch
  the same coordination files.
- The integrating session takes the lock first, performs the STATE/index/decision integration,
  commits/pushes, then releases it. The pre-commit hook blocks staged coordination-file commits
  unless `STATE.lock` is held by the committing model.
- Shared facts, procedures, and evidence live in this repo (`STATE.md`, `reference/`, `specs/`,
  `DECISIONS.md`, scripts, handoffs). Never depend on a model's private memory or prior chat for
  anything another model needs.
- **Durable artifacts need durable tools.** A document's regeneration command must point to a
  tracked script or a durable project tool, never a disposable scratch directory.
- **Markdown creation standard.** Tracked `.md` files use a retrieval envelope: `Title:`, `Type:`,
  and `Context:` near the top, plus a terminal `SYSTEM NODE LINKS` block (`Directory Path`,
  `Next Sequential File`, `Related Workflows`, `AI Execution Prompts`). Start new docs from
  `_templates/markdown-file-template.md`. After changing Markdown structure, run
  `bin/update-node-map.ps1` so `maps/NODE-MAP.md` and `maps/node-map.json` stay current.
- **Never put a secret value in a tracked file.** Keep secrets outside the repo and reference them
  by name. A pre-commit scan enforces this.

**End of session:**
1. Acquire `STATE.lock` before changing `STATE.md`, the indexes, `DECISIONS.md`, or contract/door
   files. If another session holds it, write a standalone handoff or `inbox/` note instead.
2. Update `STATE.md` to match reality - keep it scannable (Right now / Active work / Blockers /
   Next actions) and within the doctor's size budget. Check size with **one** run of
   `bin/state-budget.ps1`: byte-exact headroom, plus the per-section / fattest-line breakdown
   when over - one targeted trim, not edit/measure/re-read cycles.
3. Write a handoff `handoffs/YYYY-MM-DD-<topic>.md` and add a row to `handoffs/INDEX.md`.
4. If Markdown files changed, regenerate the node map with `bin/update-node-map.ps1`.
5. Commit `STATE.md` + `DECISIONS.md` + the handoff together, with a trailer naming the model you
   actually are (e.g. `Brain-Model: your-model-id`). Push if this memory has a private remote.
   Release `STATE.lock`. Leave the working tree clean.

`pwsh -NoProfile -File bin/close-session.ps1 -Topic <topic> -Model '<model-id>'` automates the
mechanical tail: it verifies the handoff + INDEX row + STATE freshness, checks/acquires the lock,
and commits the close set with your trailer: coordination files, your current handoff,
generated maps, and explicit `-Also` paths. Inbox notes ride only when this session owns them
and names them with `-Also`; there is no folder-wide adoption. Foreign staged paths cause
refusal without unstaging them. Not every close pushes: a private-remote memory pushes and releases
`STATE.lock` only after the push succeeds (a failed push keeps the lock so nothing else lands on
top of it); a local-only memory commits and releases the lock without attempting a push, by
design; `-OfflineCheckpoint` commits, records a loud pending-sync warning, and releases the lock
without pushing, as a deliberate offline escape hatch for a private-remote memory (recover with
`bin/sync-private.ps1` once connectivity returns). The committing identity must be explicit
(`-Model` or `BRAIN_MODEL`); the clone's `git config brain.model` is a display fallback only.
Prefer it over a hand-rolled close.

How to start/end a session: `protocol/RUNBOOK.md`. Full layout & rules: `README.md`.
<!-- END BRAIN-CONTRACT -->

---

## Advisory variant (chat apps that can't read the repo)

The block below is for surfaces that cannot read the repo, run scripts, or touch git - a web/app
chat assistant. `bin/sync-doors.ps1` generates it into `protocol/advisory.md`; paste that into a
custom-instruction field. Edit it here, then sync.

<!-- BEGIN BRAIN-ADVISORY v1 -->
<!-- Canonical source: protocol/CONTRACT.md. Generated into protocol/advisory.md by
     bin/sync-doors.ps1; paste into a custom-instruction field. Edit the source, not copies. -->

You are an **advisory participant** in a shared project/engineering memory (also driven by CLI
coding agents). You are running in a chat app, so you have **no access** to the repo, git, or the
local machine. Act as a planning and reasoning advisor only.

- The source of truth is **STATE.md**, which the user pastes when grounding is needed. If you do
  not have current STATE, **ask for it before any claim about project status - never invent it.**
- Reason from the smallest provided file set instead of requesting a broad dump.
- Don't assume you can read files, run commands, or commit. Produce text the user can act on.
- When you produce something worth keeping (a decision, finding, or draft), format it as a short
  self-contained Markdown note the user can drop into the repo's `inbox/`. Use the envelope: first
  lines `Title:`, `Type:`, `Context:`, then the body, then `SYSTEM NODE LINKS`.
- Shared facts live in the repo, not in your memory or past chats. Never output a secret value.
<!-- END BRAIN-ADVISORY -->

---

## SYSTEM NODE LINKS

Directory Path: protocol/
Next Sequential File: RUNBOOK.md
Related Workflows: bin/sync-doors.ps1, protocol/RUNBOOK.md
AI Execution Prompts: protocol/CONTRACT.md
