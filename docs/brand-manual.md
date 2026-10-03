Title: mdr-kernel brand manual
Type: Public Style Guide / Brand Manual
Context: Use when writing public-facing docs, UX copy, terminal output or visual assets for mdr-kernel; it is the authoritative style.

# mdr-kernel brand manual

> **The work is the data. The data is the work.**

`mdr-kernel` is a coordination pattern, not a mascot brand, SaaS shell, or fictional-company
replica. It should feel like a clinical environment built for compliance and continuity — a system
where agents arrive without trusted private context, read the record, perform the work, write the
handoff, and leave the next session a cleaner record.

The public style is precise, over-lit, procedural, and slightly uncomfortable. The machinery
remains plain PowerShell and git.

## Voice

The tone is bureaucratic, polite, precise, and emotionally detached.

Target qualities:

- Clinical
- Ceremonial
- Bureaucratically threatening
- Dryly funny
- Emotionally neutral
- Operationally legible

Prefer:

- "non-compliance event" over "bug" when the system blocks work
- "recorded", "authorized", "pending correction", "scaffolded" over hype language
- Short operational labels over lore-heavy explanations

Avoid:

- Hype
- SaaS cheerfulness
- Constant winking
- Camp
- Fan-fiction language
- Casual banter

The humor comes from overcontrolled institutional tone, not from punchlines. The system is
funniest when it sounds completely serious.

## Terminal UX

- **Success is silent.** Diagnostics end with `doctor: ok.` and stop.
- **Failure is bureaucratic.** Hard blocks are non-compliance events with the correction named
  plainly. Flavor is allowed around the edges; it must never hide the actual problem.
- **Rewards are corporate.** The Waffle Party protocol belongs to clean session exit or the
  explicit `bin/waffle-party.ps1` wrapper — not every successful diagnostic.
- **Parser-facing lines stay stable.** Do not theme or rename `STARTUP_STATUS=READY`,
  `RECALL_STATUS=MATCHED`, exit codes, script filenames, or protected file names.

Preferred behavior by surface:

- Startup: restrained induction (`Session Induction Initiated` is in-bounds).
- Diagnostic success: plain and concise.
- Failure: compliance language with a clear correction.
- Closeout: strongest ritual flavor (`Initiating elevator transition` is in-bounds).
- Optional wrappers: the most playful ceremonial language lives here (`Waffle Party`).

## Visual language

The visual umbrella is **clinical brutalism**.

The system should look efficient rather than decorated. Beauty comes from hierarchy, discipline,
and exposed structure.

Visual stance:

- Institutional
- Over-lit
- Flat
- Gridded
- Sparse
- Slightly hostile
- Information-first

Shorthand:

- Pretty-ugly, not messy
- Pixel-honest, not retro-cute
- Institutional, not cyberpunk
- Structured, not polished

### Palette

Use a 60 / 30 / 10 split:

- 60% dominant neutral: white or surgical off-white
- 30% structural slate / steel / charcoal
- 10% accent

Suggested tokens:

| Role | Token | Value |
|---|---|---|
| Background | `paper` | `#f7f7f2` |
| Primary text | `text` | `#111827` |
| Structural | `slate` | `#5b6670` |
| Rule / grid line | `line` | `#c7cdd3` |
| Warning / non-compliance | `warning` | `#b42318` |
| Actionable / navigation | `action` | `#1d4ed8` |

Warning red is the primary disruptive accent for alerts and non-compliance. Blue is for actionable
navigation: links, node references, and next-step indicators.

### Typography

Monospace-first.

- Preferred: IBM Plex Mono
- Acceptable fallback: Courier New, Courier, `monospace`

Typography should feel operational, not editorial.

### Graphic language

Allowed:

- Visible thin-line grids
- Schematic / spreadsheet / technical-diagram structure
- 1-bit or low-color bitmap honesty
- Blocky status marks, warning triangles, terminal-like icons
- Text-based progress bars or integrity bars when they aid comprehension

Avoid:

- Gradients
- Rounded SaaS cards
- Neon glow
- Friendly mascot design
- Smoothed-over retro assets
- Glitch effects as a default style

Graphics must serve orientation, state, or structure. If a visual exists only for mood, cut it.

## Term boundaries

`FLOOR-GUIDE.md` contains the metaphor map. The rest of the repo stays function-first. Themed
aliases are secondary labels for real mechanisms — they do not replace plain names. Do not use
protected show-specific proper nouns as filenames, package names, badges, or product labels.

The product name is `mdr-kernel`. The show-inspired language is an homage in tone only.

---

## SYSTEM NODE LINKS

Directory Path: docs/
Next Sequential File: node-map-usage-guide.md
Related Workflows: ../README.md, ../FLOOR-GUIDE.md, ../bin/brain-doctor.ps1, ../bin/waffle-party.ps1
AI Execution Prompts: ../protocol/CONTRACT.md
