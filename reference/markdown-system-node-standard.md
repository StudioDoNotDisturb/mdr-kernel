Title: Markdown system node standard
Type: Documentation Standard / Retrieval Envelope
Context: Defines the required AI-crawlable header and terminal node block for tracked Markdown in an mdr-kernel repo.

# Markdown system node standard

Tracked Markdown files should be easy for an AI agent to classify before reading the whole file,
and easy to route from after reading it. mdr-kernel treats this retrieval envelope as the default
for every authored Markdown document.

## Required top envelope

Normal authored Markdown uses these as the first three non-empty lines:

```text
Title: <human-readable document title>
Type: <document class / operational role>
Context: <one-sentence retrieval context explaining when an agent should use this file>
```

Do not hide this information in prose. The envelope is a label, not a summary:

- **Title** is a name of at most **100 characters**.
- **Context** is **one sentence** of at most **300 characters** saying when an agent should use
  the file - "Use when ..." is the reliable shape.
- Provenance, bylines, approval quotes, evidence lists, dates and caveats go in the body, e.g. an
  `> **About this file:** ...` note under the H1.

`.githooks/pre-commit` runs `bin/envelope-check.ps1 -Staged` and blocks any staged `.md` over these
limits (a ratchet: older files are fixed when next touched, and the doctor reports the remaining
count). Generators and templates that write an envelope must produce compliant values.

Markdown files that already require YAML front matter, such as skill files, use the front-matter
variant instead:

```yaml
---
Title: <human-readable document title>
Type: <document class / operational role>
Context: <one-sentence retrieval context explaining when an agent should use this file>
name: <existing skill or tool metadata>
...
---
```

Keep existing executable/config metadata intact. The envelope should improve retrieval without
breaking skill discovery, generated-door sync, or other tooling.

## Required bottom node

End the file with a system node block:

```text
---

## SYSTEM NODE LINKS

Directory Path: <repo-relative directory path>
Next Sequential File: <next file in the intended read order, or [N/A only when there is no useful next file]>
Related Workflows: <comma-separated files or [N/A only when no related workflow exists]>
AI Execution Prompts: <prompt/script files or [N/A only when there is no useful execution prompt]>
```

Use paths that another agent can resolve from the repo root. Prefer explicit filenames over prose
references.

Plain path values are valid and easiest for local and frontier models to parse. If a node field
uses Markdown links (`[STATE](STATE.md)`) or wikilinks (`[[STATE]]`), `bin/update-node-map.ps1`
normalizes them back to the same path values for node-map checks. After node edits, regenerate the
map so the link-health evidence and any graph view stay accurate:

```powershell
pwsh -NoProfile -File bin/update-node-map.ps1
```

## Scope

This applies to all tracked Markdown in the repo, and to new Markdown going forward:

- `reference/`
- `specs/`
- `handoffs/`
- `inbox/` notes
- `docs/`

Adding an envelope does not make old status text current. `STATE.md` and newer `DECISIONS.md` still
win when facts conflict.

Generated front doors (`AGENTS.md`, `CLAUDE.md`, `GEMINI.md`, generated skills) keep the envelope
outside generated contract blocks where practical, while the generated block itself continues to
come from `protocol/CONTRACT.md`.

## Creation workflow

1. Start from `_templates/markdown-file-template.md` unless a more specific template exists.
2. Fill `Title`, `Type`, and `Context` before drafting the body.
3. Add or update `SYSTEM NODE LINKS` before committing.
4. If the file belongs in `reference/`, `specs/`, or `handoffs/`, update the matching index.

---

## SYSTEM NODE LINKS

Directory Path: reference/
Next Sequential File: reference/INDEX.md
Related Workflows: _templates/markdown-file-template.md, _templates/handoff-template.md, bin/update-node-map.ps1
AI Execution Prompts: protocol/CONTRACT.md
