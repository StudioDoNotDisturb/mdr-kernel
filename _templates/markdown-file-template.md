Title: <Document Title>
Type: <See type vocabulary below>
Context: <One sentence: why this file exists and what it is for. Written for a model that has never seen this file before.>

# <Document Title>

<!--
  RETRIEVAL ENVELOPE — fill in the three header fields above before writing anything else.
  They are parsed by bin/update-node-map.ps1 to build the routing index.

  Title:   The document's plain name. Matches the H1 heading.
  Type:    One of the standard type vocabulary entries (see below).
  Context: One sentence that answers "why does this file exist and what does it contain?"
           Write it for a model arriving cold with no prior context.

  TYPE VOCABULARY
  ---------------
  Live State / Coordination File      — STATE.md and its siblings
  Canonical Contract / Model Door     — CONTRACT.md, CLAUDE.md, AGENTS.md, GEMINI.md
  Operator Runbook / Session Workflow — human-facing startup and closeout guides
  Operator Guide / Retrieval Workflow — how-to docs for humans and agents
  Operator Guide / First System Setup — adoption and onboarding guides
  Public README / Coordination Kernel — the public-facing README
  Public Reference / Themed Glossary  — FLOOR-GUIDE.md and similar orientation docs
  Public Style Guide / Brand Manual   — visual, voice, and terminal-UX rules
  Public Contribution Guide           — CONTRIBUTING.md
  Spec / Retrieval Contract           — technical specs and system contracts
  Skill Definition / Workflow Instructions — skills/<name>/SKILL.md files
  Handoff / Session Closeout          — end-of-session transfer notes
  Reference / Operations              — operational reference notes
  Reference / Setup                   — environment and tooling setup notes
  Template / Retrieval Envelope Starter — files in _templates/
-->

---

## <Section>

(Body content goes here. Keep sections short and scannable. Use plain language.
The doctrine: clinical, procedural, information-first. No hype, no lore.)

---

## SYSTEM NODE LINKS

<!--
  SYSTEM NODE LINKS — required terminal block, parsed by the node-map generator.
  Do not rename or reorder the four fields. Do not add content after this block.

  Directory Path:      Relative path of the directory containing this file (e.g. docs/).
  Next Sequential File: Relative path to the next file a reader should open after this one.
                        Use the most logical continuation in the reading chain.
  Related Workflows:   Comma-separated relative paths to files that execute or depend on this one.
  AI Execution Prompts: Relative path to the contract or skill that governs agent behavior here.
                        Usually protocol/CONTRACT.md or the relevant SKILL.md.
-->

Directory Path: <relative directory path, e.g. docs/>
Next Sequential File: <relative path to next file in reading chain>
Related Workflows: <comma-separated relative paths>
AI Execution Prompts: <relative path to governing contract or skill>
