Title: CONTRACT - the canonical mdr-kernel contract
Type: Canonical Contract / Model Door Source
Context: Canonical contract source that generates the model front doors and defines participation rules.

# CONTRACT - the canonical mdr-kernel contract

_Status: active - This file is the **single source**. The contract block between the markers below
is generated into the model-facing doors (`CLAUDE.md`, `AGENTS.md`, `GEMINI.md`) by
`bin/sync-doors.ps1`. **Edit it here, then run sync.** A pre-commit check blocks commits where a
door has drifted from this source._

<!-- BEGIN BRAIN-CONTRACT v2 -->
<!-- Canonical source: protocol/CONTRACT.md. In the front-door files this block is generated -
     edit the source and run bin/sync-doors.ps1; do not edit the generated copies. -->

## mdr-kernel contract - every model follows this

This repo is a shared project memory for every AI agent that works in it: Claude, Codex, Gemini or
a local model. **The repo is the only shared brain: anything another model needs is a file here,
never a model's private memory.** Hooks and scripts enforce most rules: when one refuses, or a
recall banner appears, read it and do what it says. Rule specifics: `protocol/CONTRACT.md`
§ Rule detail. Procedures: `protocol/RUNBOOK.md`.

**Start:** run `pwsh -NoProfile -File bin/start-session.ps1 -Model '<actual model id>'`. It runs
the doctor, pulls, and prints `STATE.md`. Never rebuild those steps from memory. Resolve every
`NOTE=`, and write nothing until `STARTUP_STATUS=READY`. `STATE.md` is live truth: when another
doc disagrees, STATE wins, and you fix the other doc. With no PowerShell or no writable clone,
you are not a full participant: read `STATE.md` and return an inbox-ready note.

**Never:**
- Put a secret value in a tracked file. Keep secrets outside the repo and reference them by name.
- Treat your own choice as a ruling. **Only the operator's rulings bind.** Add decisions only
  with `bin/add-decision.ps1`. Only the human runs `bin/ratify-decision.ps1`, in their own
  terminal.
- Edit a file listed in `protocol/coordination-files.txt` without holding `STATE.lock` yourself
  (`bin/state-lock.ps1`; RUNBOOK § Lock rules), or force-recover another session's lock
  without independent evidence or the operator's direction. The lock is machine-local: two
  clones never share it, so keep sessions on different clones sequential.

**While working:**
- **Run recall before acting on assigned work.** A prompt hook runs `bin/hook-recall.ps1` only
  if you configured one (`docs/recall-hooks.md`). Without a hook you have seen fire, run
  `bin/prepare-work.ps1 -Objective '<operator objective>'`, which calls `bin/brain-recall.ps1`.
  When gates match, read their sources, state the traps and their counts, and say how they
  change your next step.
- **Walk the least-code ladder before writing any code.** Go in order and stop at the first rung
  that holds: is it needed at all? Is it already in the repo (grep)? Does the stdlib or the
  platform do it? Does an installed dependency? Is it one line? Only then write the minimum that
  works. Never cut trust-boundary validation, data-loss handling, security or accessibility. A
  commit that adds a new `bin/` file or over 60 code lines needs a `Least-code:` trailer.
- **Walk the least-file ladder before creating any file or folder.** Does anyone need it after
  this session? Can an existing file hold it? Is it bulk, generated or temporary (scratch, never
  the repo)? Only then create one file; a new folder needs three files landing now. A commit
  that adds files beyond one dated handoff and `inbox/` notes needs a `Least-file:` trailer.
- Commit and push each unit of work as it lands.
- Every tracked `.md` file starts with `Title:`, `Type:` and `Context:` lines and ends with a
  `SYSTEM NODE LINKS` block. Start new files from `_templates/markdown-file-template.md`.
- Don't re-read what startup printed. Read narrow slices. Never paste `DECISIONS.md`,
  `handoffs/INDEX.md` or large logs into the conversation.

**Close:** update `STATE.md` and write your handoff, then run
`pwsh -NoProfile -File bin/close-session.ps1 -Topic <topic> -Model '<model-id>'`. It checks both,
regenerates `maps/NODE-MAP.md`, commits, pushes when the memory has a private remote, and
releases the lock.
<!-- END BRAIN-CONTRACT -->

---

## Rule detail

This section is **not** generated into the doors. The summary above keeps each rule short; the
full explanation of each one lives in the file listed here. Read it when the rule applies.

| Rule | Full explanation |
|---|---|
| Startup, the reading slice, the report after startup | `protocol/RUNBOOK.md` § Full-participant kickoff; `bin/start-session.ps1` prints the reporting rule at the end of every startup |
| Recall gates, hooks and the manual fallback | `docs/recall-hooks.md`; gate format in `protocol/recall-gates.example.json` |
| Decisions: proposals, rulings, Supersedes/Retires stamps, ratification, migrating an older log | `docs/decision-authority.md` |
| `STATE.lock`: acquire, session token, resume, stale heartbeat, forced recovery | `protocol/RUNBOOK.md` § Lock rules |
| Model identity and the `Brain-Model:` trailer | `protocol/RUNBOOK.md` § Full-participant kickoff; enforced by `.githooks/prepare-commit-msg` |
| Close: private remote vs local-only, offline checkpoint, owned paths | `protocol/RUNBOOK.md` § Close prompt |
| Markdown envelope and the generated node map (`maps/NODE-MAP.md`) | `_templates/markdown-file-template.md`; `docs/node-map-usage-guide.md` |
| Upgrading from an earlier release | `docs/upgrading.md` |

Two rules have no other home:

- **Prefer appending over rewriting.** Background or high-volume work writes atomic notes into
  `inbox/` instead of editing `STATE.md`; one session at a time folds `inbox/` into STATE.
- **Durable artifacts need durable tools.** A document's regeneration command must point to a
  tracked script or a durable project tool, never a disposable scratch directory.

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
- Produce text the user can act on.
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
