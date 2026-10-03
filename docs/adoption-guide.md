Title: Adoption guide
Type: Operator Guide / First System Setup
Context: Practical path for turning an existing messy workspace into an mdr-kernel memory without importing chaos.

# Adoption guide

> **The work is the data. The data is the work.**

Use this after the quick start works. The goal is not to pour every old file into the repo. The
goal is to create one trusted coordination layer that can point at the rest of your work.

Uncoordinated work is the default. This system makes it difficult to stay uncoordinated. That is
the point.

## Rule zero — your memory is private

Everything this system grows — `STATE.md`, handoffs, decisions, and the generated node maps —
will contain your file names, local paths, and work history. Before the first setup:

- On GitHub, use **Use this template**, choose **Private**, then clone the new repository you own.
- Never adopt inside a **public fork** — GitHub forks of public repos are always public.
- Run `bin/setup-private.ps1` inside that clone. It verifies and pins the private destination;
  bootstrap, doctor, closeout, and pre-push all enforce the same result.
- Local-only mode is explicit: `bin/setup-private.ps1 -Model '<id>' -LocalOnly` removes `origin`
  before creating memory.

## Minimum setup

You need:

- git
- PowerShell 7+ (`pwsh`)
- one AI coding surface that can read/write the clone and run shell commands

Everything else is optional. Do not install every tool this repository mentions. Add tools only
when your own work needs them, and record the setup in a file.

## First hour

1. Create your own **private** repository from the template and clone that repository.
2. Run private setup and canonical start:

```powershell
pwsh -NoProfile -File bin/setup-private.ps1 -Model '<your model id>'
pwsh -NoProfile -File bin/start-session.ps1 -Model '<your model id>'
```

3. Give the first full participant a narrow adoption objective. Do not ask it to absorb your whole
   computer:

```text
Turn this scaffold into my first real mdr-kernel state.
Inventory: <short list of active projects, notes folders, tools, and blockers>.
Do not move files yet. Classify what belongs in STATE, reference, specs, handoffs, inbox, or skills.
```

4. The participant sets `BRAIN_MODEL`, acquires `STATE.lock` and carries the printed token as
   `BRAIN_LOCK_TOKEN`, replaces the scaffolded `STATE.md` with a short real one, creates a handoff
   from `_templates/handoff-template.md`, updates `handoffs/INDEX.md`, regenerates the node map,
   and runs closeout.

Every commit in this clone needs an explicit identity, including your own hand commits: set
`$env:BRAIN_MODEL = '<name>'` or end the message with `Brain-Model: <name>`. A commit with
neither, or with `unknown`, is refused. `git config brain.model` is display-only, and setup does
not write it.

The first real `STATE.md` should read like this:

```text
Right now: what is true today.
Active work: one to three live items.
Blockers: only things that actually stop work.
Next actions: the next concrete command, file, or decision.
```

That proves the loop before you add complexity.

## Bring existing work in slowly

Do not move a giant desktop, notes vault, source folder, cloud drive, or media archive into
`mdr-kernel`. Make an inventory note first:

```text
inbox/YYYY-MM-DD-existing-work-inventory.md
```

Closeout never adopts `inbox/` folder-wide. When the session that wrote the inventory note wants it
committed, it names the note explicitly: `-Also inbox/YYYY-MM-DD-existing-work-inventory.md`.

For each existing item, choose one bucket:

| Bucket | Put it here | Example |
|---|---|---|
| Live status | `STATE.md` | "Website launch blocked on DNS." |
| Durable fact or setup note | `reference/<slug>.md` | "How network storage is mounted." |
| Planned or required behavior | `specs/<topic>.md` | "What the import tool must do." |
| Repeatable agent workflow | `skills/<name>/SKILL.md` | "When I ask for release prep, do these checks." |
| Session detail | `handoffs/YYYY-MM-DD-topic.md` | "What changed during this session." |
| Not ready yet | `inbox/<date>-<topic>.md` | "Messy notes to fold later." |

If the material is large, private, binary, or changes constantly, keep it outside the repo and
write a reference note that points to it. The repo holds authority and pointers, not every asset.
A good reference note tells another agent where the material lives, what it is allowed to do
there, and what not to touch.

**New reference notes and new skills are gated.** The pre-commit hook refuses a commit that adds
a new `reference/*.md` or skill entrypoint unless a matching approve entry is staged with it.
The flow is two commands, not a direct file write:

```powershell
pwsh -NoProfile -File bin/write-approval.ps1 -Propose -Target reference/<slug>.md -Source <draft> -Reason '<why>'
pwsh -NoProfile -File bin/write-approval.ps1 -Approve -ProposalId <id-printed-by-propose>
```

Approve writes the file and appends the audit record (`inbox/write-approval-audit.jsonl`), which
close-session carries automatically. In a one-human kernel you approve your own proposals — the
audit line is the record, not a second person. **What approve does not do:** it never deletes the
proposal from `.write-proposals/` — that directory is gitignored local staging, and approved
proposals accumulate there indefinitely (each one a full copy of its content). `-List` shows what
is pending; the audit log is the durable record of what was approved. It is safe to delete
`.write-proposals/` entirely at any time — nothing reads it after `-Approve` writes the real file.
Add the `reference/INDEX.md` row in the same session so the index and the file land in one
commit; when closing, name the new file with `-Also reference/<slug>.md` (multiple paths:
`-Also 'a.md,b.md'`).

## Existing decision logs

If you bring an older `DECISIONS.md` along, do not paste it in and commit: the pre-commit history
check expects every entry to carry an authority class. Follow the explicit migration in
`docs/decision-authority.md` (`bin/initialize-decision-authority.ps1 -PreserveLegacy`, run under
your own lock after the old log is committed unchanged). It preserves old entries without
ratifying them; new entries go through `bin/add-decision.ps1`.

## External folders

`maps/node-roots.json` can index external Markdown folders, but start with the repo itself. When
you add another root, make it `index-only` first:

```json
{
  "name": "project-notes",
  "path": "C:/path/to/notes",
  "writePolicy": "index-only",
  "trackGit": false,
  "includes": ["**/*.md"],
  "excludes": ["**/.git/**", "**/node_modules/**"]
}
```

Run:

```powershell
pwsh -NoProfile -File bin/update-node-map.ps1
pwsh -NoProfile -File bin/update-node-map.ps1 -Check
```

`-Check` is not just an issue diff. It also fails when the generated map artifacts are stale and no
longer match the current roots.

Only switch a root to `repair-allowed` after you trust the map repair behavior on that folder.

## Skills

Make a skill when the operator has a repeatable request that should trigger a procedure. Keep
ordinary facts in `reference/` instead.

Use:

```text
_templates/skill-template.md
skills/<skill-name>/SKILL.md
skills/<skill-name>/evals/trigger-queries.json
```

A useful skill has:

- a precise "when to use" trigger
- a "do not use when" boundary
- required inputs
- a step-by-step procedure
- verification
- a concrete output shape

Do not create a skill for a vague preference, a one-time task, or a tool you have not used yet.
Write a reference note or inbox note first.

## If you are not used to the software

Keep the first system boring:

- one terminal
- one AI coding agent
- one clone
- one `STATE.md`
- one handoff per session

Add specialized software as references when it becomes real. A video editor, design app, local
model server, network storage, build system, or deployment target does not need a skill on day one. It needs
a small setup note, the exact command or UI path that works, and a known failure mode if you have
one.

## Safety rules

- Never commit secrets. Store them outside the repo and reference them by filename.
- Do not publish a clone that contains personal state, client names, private paths, screenshots,
  or operational hostnames.
- Do not let two sessions edit coordination files without `STATE.lock`.
- Prefer `inbox/` for messy material until a full participant folds it into `STATE.md`,
  `reference/`, `specs/`, or `skills/`.
- Keep `STATE.md` short. If it needs a narrative, write a handoff.

## Good first objectives

- "Turn my current project status into a real `STATE.md`."
- "Inventory my existing notes and propose what becomes reference, spec, skill, or inbox."
- "Create one skill for my release checklist."
- "Add one external notes folder as an index-only node-map root."
- "Run closeout and prove a second agent can resume cold."

---

## SYSTEM NODE LINKS

Directory Path: docs/
Next Sequential File: ../skills/mdr-kernel/SKILL.md
Related Workflows: ../protocol/RUNBOOK.md, ../_templates/skill-template.md, ../skills/mdr-kernel/SKILL.md, ../bin/update-node-map.ps1
AI Execution Prompts: ../protocol/CONTRACT.md
