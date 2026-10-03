Title: Node map usage guide
Type: Operator Guide / Link-Health Workflow
Context: Shows how the generated node map checks link health and where reading selection actually comes from.

# Node map usage guide

The node map is generated link-health evidence. It lists what each Markdown file's envelope says
it is for, what it links to next, and which links or envelopes are broken. It is **not** the
reading router: `STATE.md` plus `reference/INDEX.md` choose what an agent reads first.

## Agent workflow

At startup, the model reads the `STATE.md` that `bin/start-session.ps1` prints. When assigned
work, it uses `STATE.md` plus `reference/INDEX.md` to pick the smallest relevant file set, states
that selection, and expands only when the first slice proves insufficient.

A compliant response looks like this:

```text
Selected files:
- STATE.md — live truth.
- reference/INDEX.md — points this objective at one reference note.
- specs/node-map.md — objective touches map behavior.

Not reading yet:
- handoffs/ — STATE did not point to a session-specific handoff.
- maps/NODE-MAP.md — link-health evidence; consulted only when checking links.
```

Consult `maps/NODE-MAP.md` or `maps/node-map.json` when the task is about link integrity, broken
envelopes, or a missing node — not to decide what to read.

## Human workflow

After changing Markdown structure, regenerate the map:

```powershell
pwsh -NoProfile -File bin/update-node-map.ps1
pwsh -NoProfile -File bin/update-node-map.ps1 -Check
```

`brain-doctor` runs the same parity check: it warns when tracked Markdown, JSON, the readable map,
or the Obsidian bridge disagree. The generated map files are coordination files, so regenerate and
commit them under your own `STATE.lock`.

## Why envelopes still save tokens

Every tracked Markdown file carries a `Title:`/`Type:`/`Context:` envelope and a terminal
`SYSTEM NODE LINKS` block. That lets an index row or a map row say what a file is for without
opening it:

- titles and types identify authority level
- contexts summarize why the file exists
- terminal node links expose the next file and execution prompts
- issue sections surface broken or off-shape nodes before they cause retrieval drift

In the private system that produced this template, an earlier map-routed workflow reduced one
review bundle from 16,387 prompt tokens to 1,735 while keeping the relevant authority files in
view. Treat that as evidence that compact envelope-based selection beats broad reading, not as a
guaranteed ratio, and not as a reason to route through the map: selection now starts from
`STATE.md` and `reference/INDEX.md`.

## Maintenance rules

- Do not hand-edit generated map files (`NODE-MAP.md`, `node-map.json`, `OBSIDIAN-NODE-LINKS.md`).
- Keep `maps/node-roots.json` small and explicit.
- Prefer relative root paths when the map describes the repo itself.
- Mark external roots `index-only` unless you want repair mode to write there.
- Keep every tracked Markdown file on the envelope standard so the map remains useful.
- Express links relative to the source file or the repo root; generated bridges use the resolved
  root coordinate rather than interpreting the raw link again from `maps/`.
- Put intentional template placeholders in the root's exact `waivedIssues` registry. Do not use a
  broad directory exemption that could hide a real broken template link.

---

## SYSTEM NODE LINKS

Directory Path: docs/
Next Sequential File: ../specs/node-map.md
Related Workflows: ../maps/node-roots.json, ../bin/update-node-map.ps1, ../reference/INDEX.md
AI Execution Prompts: ../protocol/CONTRACT.md
