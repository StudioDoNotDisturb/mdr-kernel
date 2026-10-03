Title: Floor Guide
Type: Public Reference / Themed Glossary
Context: Defines the contained workplace metaphor used by mdr-kernel and maps each themed alias to its real mechanism.

# Floor Guide

> **The work is the data. The data is the work.**

`mdr-kernel` uses one controlled metaphor: a worker arrives without trustworthy private memory and
must trust the record over the chat window. That framing is useful because it makes context
isolation concrete and behavioral. The metaphor annotates the machine. It does not rename it.

The product name is `mdr-kernel`. The machinery keeps plain names. If a themed phrase does not
make a real mechanism easier to understand, it does not belong here.

## [ NAMESAKE ] Where The Name Comes From

`mdr` is **Macrodata Refinement** — in the source metaphor, the department whose workers refine data
they are severed from, without the full context of what it means. `kernel` is the small,
load-bearing core the whole system runs on.

That is the pun and the thesis in one: an agent refines the work in front of it, severed from prior
context, trusting the record over its own memory. The refinement is real; the severance is the
discipline that makes it trustworthy. The show supplied the metaphor's *name* only — the refinement
here is engineering, not homage.

## [ ORIENTATION ] What The Metaphor Maps

The severed-floor image explains five real behaviors:

- **Strict context boundary.** An agent cannot rely on private chat memory. The repository is the
  memory.
- **Single source of live truth.** `STATE.md` wins. If anything disagrees with it, `STATE.md` is
  correct and the stale source gets fixed.
- **Small retrieval slice.** `STATE.md` and `reference/INDEX.md` tell the agent what to read first.
  Erratic repo crawls are a compliance failure, not a search strategy.
- **Known-trap recall.** Recurring failures are brought back into view before action. The system
  makes the agent face prior evidence. It does not ask nicely.
- **Clean exit.** The handoff lets the next session start without inheriting the previous one's
  context window. The worker leaves. The record stays.

This is the useful part of the metaphor: not amnesia for its own sake, but disciplined continuity
between workers who cannot trust each other's memory.

## [ BOUNDARIES ] Where It Stops

The implementation is not a costume.

- No protected logos, art, marks, package names, badges, or product naming derived from the
  inspiration.
- No show-specific proper nouns as filenames, headings, branches, scripts, or CLI output.
- No themed language in parser-facing status lines, exit codes, or machine-readable JSON.
- No lore where a new adopter needs a plain instruction.
- No metaphor when the command name is already clear.

Public docs may acknowledge the inspiration in prose. Operational docs teach the real workflow:
startup, doctor, recall, lock, handoff, close.

## [ ALIASES ] Approved Terms

These aliases are allowed because each one maps cleanly to a working mechanism. They are secondary
labels, not replacements.

| Themed alias | What it describes | Real mechanism |
|---|---|---|
| **The severed floor** | The working copy where an agent starts fresh and must trust the repo over private memory. | Repository root |
| **The Floor** | Where executable refinement work happens. | `bin/` |
| **The Protocol** | The canonical rules every full participant receives. | `protocol/` |
| **The Intake** | Unrefined notes waiting to be classified and folded into state, reference, specs, or handoffs. | `inbox/` |
| **The macrodata table** | The generated link-health view of every Markdown node (not the reading router). | `maps/NODE-MAP.md`, `maps/node-map.json` |
| **The Break Room** | The active-recall gate that forces known mistakes back into view before action. | `bin/brain-recall.ps1` (via `bin/hook-recall.ps1` or `bin/prepare-work.ps1`) |
| **The Perpetuity Wing** | The durable record of decisions and indexed handoffs. | `DECISIONS.md`, `handoffs/` |
| **The elevator** | Session closeout: the worker exits, the record remains. | `handoffs/YYYY-MM-DD-topic.md`, `bin/close-session.ps1` |
| **The waffle party** | Optional success flavor around a clean diagnostic. Narrow use only — never the formal workflow name. | `bin/waffle-party.ps1` |

## [ LAW ] STATE Wins

> **STATE wins.**

If a comment, handoff, README, issue, or model memory disagrees with `STATE.md`, `STATE.md` is the
live truth. Fix the stale source.

Everything else protects that law. Locks prevent two sessions from editing the same coordination
layer. The doctor catches drift. Active recall prevents known loops. Handoffs make the next worker
less dependent on the previous worker's context window.

---

## SYSTEM NODE LINKS

Directory Path: ./
Next Sequential File: protocol/CONTRACT.md
Related Workflows: README.md, docs/brand-manual.md, protocol/CONTRACT.md, bin/brain-recall.ps1
AI Execution Prompts: protocol/CONTRACT.md
