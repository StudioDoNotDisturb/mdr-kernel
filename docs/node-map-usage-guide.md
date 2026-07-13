Title: Node map usage guide
Type: Operator Guide / Retrieval Workflow
Context: Shows how humans and agents use the node map to route work without wasting context.

# Node map usage guide

The node map is the retrieval layer. Use it before reading broadly. It is the compact directory
of what the repo says each Markdown file is for, what it links to next, and which workflows should
execute it.

Agents that skip the node map and crawl the repo broadly are non-compliant. That is not a style
preference. It is the mechanism that makes context budgets work.

## Agent workflow

At startup, the model reads `STATE.md`. When assigned work, it consults `maps/NODE-MAP.md` or
`maps/node-map.json` and picks the smallest relevant file set.

A compliant response looks like this:

```text
Selected files:
- STATE.md — live truth.
- maps/NODE-MAP.md — routing layer.
- specs/node-map.md — objective touches map behavior.

Not reading yet:
- handoffs/ — STATE did not point to a session-specific handoff.
- reference/ — map did not route this objective there.
```

The model expands only when evidence shows the first slice is insufficient. Expanding without
evidence is a non-compliance event.

## Human workflow

After changing Markdown structure, regenerate the map:

```powershell
pwsh -NoProfile -File bin/update-node-map.ps1
pwsh -NoProfile -File bin/update-node-map.ps1 -Check
```

`brain-doctor` runs the same exact parity check: it warns when tracked Markdown, JSON, the readable
map, or the Obsidian bridge disagree. Regenerate before asking an agent to do wide analysis.

## Why this saves tokens

Without a node map, agents open large files and indexes just to learn where to look. With the
map, they route from a compact generated index:

- titles and types identify authority level
- contexts summarize why the file exists
- terminal node links expose the next file and execution prompts
- issue sections surface broken or off-shape nodes before they cause retrieval drift

In the private system that produced this public template, compact map routing reduced one review
bundle from 16,387 prompt tokens to 1,735 prompt tokens while keeping the relevant authority files
in view. Treat that as evidence for the pattern, not a guaranteed ratio for every repo.

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
Related Workflows: ../maps/NODE-MAP.md, ../maps/node-map.json, ../bin/update-node-map.ps1
AI Execution Prompts: ../protocol/CONTRACT.md
