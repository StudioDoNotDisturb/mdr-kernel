---
Title: mdr-kernel adoption and operations
Type: Skill Definition / Workflow Instructions
Context: Repeatable procedure for setting up, operating, and extending an mdr-kernel repo from an existing messy workspace.
name: mdr-kernel
description: >
  Use when the operator asks to set up, bootstrap, or operate mdr-kernel — including organizing
  an existing messy workspace, creating a first STATE/handoff/recall gate, or deciding where a
  workflow belongs. Do not use for ordinary coding once the repo has already routed the task to
  a project-specific file set.
compatibility: Requires repo access, git, PowerShell 7+, and write permission on the clone.
metadata:
  mdr_kernel:
    status: productive
    posture: read-first
    network: default-deny
    writes: lock-required-for-coordination
---

# mdr-kernel adoption and operations

## When To Use

- The operator asks how to install, bootstrap, or start a session.
- The operator has lots of existing files, notes, tools, or projects and wants them organized.
- The operator asks where a fact, runbook, spec, handoff, inbox note, or skill should live.
- The operator asks to create or review a repeatable `skills/<name>/SKILL.md` workflow.
- The operator says agents are repeating a mistake, losing context, or colliding in files.

## Do Not Use When

- The work is ordinary coding inside an already-routed project file set.
- The operator only wants a general explanation of git, PowerShell, or AI agents.
- The task requires private project facts that are not in the repo or provided by the operator.
- The surface cannot read the repo or run commands; in that case, produce an inbox-ready advisory
  note instead.

## Inputs Needed

- Current `STATE.md`, unless the clone has not been set up yet.
- `reference/INDEX.md`, which with `STATE.md` selects the reading slice.
- `maps/NODE-MAP.md` only when checking link health; it is generated evidence, not a router.
- The operator's objective.
- If organizing existing work: a short inventory of folders, repos, notes, tools, and pain points.

## Procedure

1. If the clone has never been set up (no `STATE.md`; phase-zero front doors), run the one
   adopter entry point — never raw bootstrap:

```powershell
pwsh -NoProfile -File bin/setup-private.ps1 -Model '<your model id>'
```

   Pass your actual model id (`-LocalOnly` for memory with no remote backup). If the clone was
   set up before and the doctor reports unarmed hooks or missing scaffold files, repair with the
   internal primitive instead. It refuses a nonempty index and acquires `STATE.lock` (with your
   explicit identity) before it writes:

```powershell
pwsh -NoProfile -File bin/bootstrap-clone.ps1 -Model '<your model id>' -CommitScaffold
```

2. Run the canonical startup once, if the surface can execute commands:

```powershell
pwsh -NoProfile -File bin/start-session.ps1 -Model '<your model id>'
```

Require `STARTUP_STATUS=READY` before proceeding.

3. Before acting on assigned work, run active recall. A configured prompt hook runs
   `bin/hook-recall.ps1` automatically, but configuration is not proof that the host fired it.
   Without a verified firing receipt, or when the hook reports DEGRADED, run the portable
   fallback (see `docs/recall-hooks.md`):

```powershell
pwsh -NoProfile -File bin/prepare-work.ps1 -Objective '<operator objective>'
```

If a gate matches, read the listed sources before giving implementation advice.

4. Use `STATE.md` plus `reference/INDEX.md` to select the smallest relevant reading slice. State
   what you selected and what you are intentionally not reading yet.

5. If the operator is adopting existing work, follow `docs/adoption-guide.md`: inventory first,
   then bucket material into `STATE.md`, `reference/`, `specs/`, `skills/`, `handoffs/`, or
   `inbox/`. New `reference/*.md` files and new skills are gated: create them through
   `bin/write-approval.ps1` (`-Propose` then `-Approve`) — never a direct write — and name them
   with `-Also` at close so the file, its INDEX row, and the audit entry land in one commit.

6. If a repeatable procedure is needed, start from `_templates/skill-template.md` and create:

```text
skills/<skill-name>/SKILL.md
skills/<skill-name>/evals/trigger-queries.json
```

7. Keep tools outside the kernel until they are real. Record a setup note in `reference/` before
   making a skill.

8. For coordination-file edits, set `BRAIN_MODEL`, acquire `STATE.lock`, and carry the printed
   token as `BRAIN_LOCK_TOKEN`; if another session holds the lock, write a standalone handoff or
   inbox note instead. A stale heartbeat is not proof that the holder is gone. New decisions go
   through `bin/add-decision.ps1` as `proposed` unless you can quote the operator's ruling.

9. Regenerate the node map after Markdown structure changes:

```powershell
pwsh -NoProfile -File bin/update-node-map.ps1
pwsh -NoProfile -File bin/update-node-map.ps1 -Check
```

## Gotchas

- Do not import a whole messy workspace. Point to large or private material from reference notes.
- Do not turn every preference into a skill. Skills are for repeatable procedures with triggers.
- Do not ask a new user to install every tool in someone else's setup. Git and PowerShell are the
  load-bearing requirements; other tools are added when the user's work proves they are needed.
- A clean doctor is necessary but not sufficient. The user still needs a real `STATE.md`.
- If recall matches a gate, read the listed sources before giving implementation advice.
- Every commit needs `BRAIN_MODEL` or an explicit `Brain-Model:` trailer; `git config
  brain.model` is display-only and the commit-message hook refuses a commit without either.
- Never run `bin/ratify-decision.ps1`; ratification is the human operator's act.

## Verification

- `bin/setup-private.ps1 -Model '<actual model id>'` works in a fresh private clone (and
  `bin/bootstrap-clone.ps1 ... -CommitScaffold` repairs an already-set-up one).
- `bin/start-session.ps1 -Model '<actual model id>'` reaches `STARTUP_STATUS=READY`.
- `bin/prepare-work.ps1 -Objective '<objective>'` prints useful traps for the example or newly
  created gate, and `bin/brain-recall.ps1 -CheckGates` accepts the gate file.
- The node map check passes.
- The repo has no secret-shaped strings and no private operational paths.

## Outputs

- A short diagnosis of the current repo/memory state.
- A minimal file set to read next.
- Concrete edits or a proposed inbox note, depending on write permissions and lock state.
- If a skill is created: `SKILL.md`, `evals/trigger-queries.json`, and a node-map refresh.

---

## SYSTEM NODE LINKS

Directory Path: skills/mdr-kernel/
Next Sequential File: ../../_templates/skill-template.md
Related Workflows: ../../protocol/CONTRACT.md, ../../docs/adoption-guide.md, ../../_templates/skill-template.md
AI Execution Prompts: ../../protocol/CONTRACT.md
