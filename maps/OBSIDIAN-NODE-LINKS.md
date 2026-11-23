Title: Obsidian Node Links
Type: Generated Obsidian Graph Bridge / Do Not Hand-Edit
Context: Generated Markdown-link bridge that lets Markdown tools see mdr-kernel node-map relationships.

# Obsidian Node Links

Generated from `maps/node-map.json` by `bin/update-node-map.ps1`. Do not hand-edit.

This file gives Markdown graph tools real links for the node map. The source files remain canonical; this generated bridge exists so graph views, backlinks, and outgoing links can see the retrieval structure.

generatedUtc: 2026-11-23T05:03:37.0641730Z

## [_templates/handoff-example.md](../_templates/handoff-example.md)

- Title: Handoff — 2026-01-15 — first recall gate for the deploy loop
- Type: Handoff / Session Closeout (EXAMPLE)
- Context: Use when writing a handoff and you want a worked example of the expected shape, altitude and completeness; it is not a live handoff, so never place it in handoffs/.
- Next Sequential File: [_templates/handoff-template.md](../_templates/handoff-template.md)
- Related Workflows: [STATE.md](../STATE.md), [DECISIONS.md](../DECISIONS.md), [protocol/RUNBOOK.md](../protocol/RUNBOOK.md)
- AI Execution Prompts: [protocol/CONTRACT.md](../protocol/CONTRACT.md)

## [_templates/handoff-template.md](../_templates/handoff-template.md)

- Title: Handoff - YYYY-MM-DD - <short topic>
- Type: Handoff / Session Closeout
- Context: End-of-session transfer note that lets another full participant resume the work cold.
- Next Sequential File: [handoffs/INDEX.md](../handoffs/INDEX.md)
- Related Workflows: [STATE.md](../STATE.md), [DECISIONS.md](../DECISIONS.md), [protocol/RUNBOOK.md](../protocol/RUNBOOK.md)
- AI Execution Prompts: [protocol/CONTRACT.md](../protocol/CONTRACT.md)

## [_templates/markdown-file-template.md](../_templates/markdown-file-template.md)

- Title: <Document Title>
- Type: <See type vocabulary below>
- Context: <Use when ... - one sentence, at most 300 characters, for a model that has never seen this file.>
- Next Sequential File: none recorded
- Related Workflows: none recorded
- AI Execution Prompts: none recorded
- Waived Node Map Issues: note:Directory Path:Relative path of the directory containing this file (e.g. docs/).; note:Next Sequential File:Relative path to the next file a reader should open after this one.; note:Related Workflows:Comma-separated relative paths to files that execute or depend on this one.; note:AI Execution Prompts:Relative path to the contract or skill that governs agent behavior here.

## [_templates/skill-template.md](../_templates/skill-template.md)

- Title: <Skill Title>
- Type: Skill Definition / Workflow Instructions
- Context: <One sentence: what repeatable workflow this skill covers and when it fires.>
- Next Sequential File: [docs/adoption-guide.md](../docs/adoption-guide.md)
- Related Workflows: [docs/adoption-guide.md](../docs/adoption-guide.md), [protocol/CONTRACT.md](../protocol/CONTRACT.md)
- AI Execution Prompts: [protocol/CONTRACT.md](../protocol/CONTRACT.md)
- Waived Node Map Issues: broken-link:skills/<skill-name>/

## [AGENTS.md](../AGENTS.md)

- Title: AGENTS.md - mdr-kernel phase-zero door
- Type: Phase-Zero Agent Front Door
- Context: Pre-setup instructions for an agent opened in a fresh clone; setup regenerates this file into the full contract door.
- Next Sequential File: [protocol/CONTRACT.md](../protocol/CONTRACT.md)
- Related Workflows: [README.md](../README.md), [bin/setup-private.ps1](../bin/setup-private.ps1), [protocol/CONTRACT.md](../protocol/CONTRACT.md)
- AI Execution Prompts: [protocol/CONTRACT.md](../protocol/CONTRACT.md)

## [CHANGELOG.md](../CHANGELOG.md)

- Title: mdr-kernel changes
- Type: Release Notes / Changelog
- Context: Describes portable changes in the distribution without treating a source version as proof of publication or host runtime verification.
- Next Sequential File: [docs/upgrading.md](../docs/upgrading.md)
- Related Workflows: [README.md](../README.md), [docs/upgrading.md](../docs/upgrading.md), [.github/workflows/doctor.yml](../.github/workflows/doctor.yml)
- AI Execution Prompts: [protocol/CONTRACT.md](../protocol/CONTRACT.md)

## [CLAUDE.md](../CLAUDE.md)

- Title: CLAUDE.md - mdr-kernel phase-zero door
- Type: Phase-Zero Agent Front Door
- Context: Pre-setup instructions for an agent opened in a fresh clone; setup regenerates this file into the full contract door.
- Next Sequential File: [protocol/CONTRACT.md](../protocol/CONTRACT.md)
- Related Workflows: [README.md](../README.md), [bin/setup-private.ps1](../bin/setup-private.ps1), [protocol/CONTRACT.md](../protocol/CONTRACT.md)
- AI Execution Prompts: [protocol/CONTRACT.md](../protocol/CONTRACT.md)

## [CONTRIBUTING.md](../CONTRIBUTING.md)

- Title: Contributing to mdr-kernel
- Type: Public Contribution Guide
- Context: Explains how to report cold-clone issues, portability gaps, and useful adaptations.
- Next Sequential File: [README.md](../README.md)
- Related Workflows: [README.md](../README.md), [docs/brand-manual.md](../docs/brand-manual.md), [protocol/RUNBOOK.md](../protocol/RUNBOOK.md), [bin/brain-doctor.ps1](../bin/brain-doctor.ps1)
- AI Execution Prompts: [protocol/CONTRACT.md](../protocol/CONTRACT.md)

## [docs/adoption-guide.md](../docs/adoption-guide.md)

- Title: Adoption guide
- Type: Operator Guide / First System Setup
- Context: Practical path for turning an existing messy workspace into an mdr-kernel memory without importing chaos.
- Next Sequential File: [skills/mdr-kernel/SKILL.md](../skills/mdr-kernel/SKILL.md)
- Related Workflows: [protocol/RUNBOOK.md](../protocol/RUNBOOK.md), [_templates/skill-template.md](../_templates/skill-template.md), [skills/mdr-kernel/SKILL.md](../skills/mdr-kernel/SKILL.md), [bin/update-node-map.ps1](../bin/update-node-map.ps1)
- AI Execution Prompts: [protocol/CONTRACT.md](../protocol/CONTRACT.md)

## [docs/brand-manual.md](../docs/brand-manual.md)

- Title: mdr-kernel brand manual
- Type: Public Style Guide / Brand Manual
- Context: Use when writing public-facing docs, UX copy, terminal output or visual assets for mdr-kernel; it is the authoritative style.
- Next Sequential File: [docs/node-map-usage-guide.md](../docs/node-map-usage-guide.md)
- Related Workflows: [README.md](../README.md), [FLOOR-GUIDE.md](../FLOOR-GUIDE.md), [bin/brain-doctor.ps1](../bin/brain-doctor.ps1), [bin/waffle-party.ps1](../bin/waffle-party.ps1)
- AI Execution Prompts: [protocol/CONTRACT.md](../protocol/CONTRACT.md)

## [docs/decision-authority.md](../docs/decision-authority.md)

- Title: Decision authority and preserved history
- Type: Operator Guide / Decision Integrity
- Context: Exactly what the decision tools do: proposals, quoted rulings, human ratification, compiled recall, the hashing rule and explicit migration of an existing log.
- Next Sequential File: [docs/upgrading.md](../docs/upgrading.md)
- Related Workflows: [protocol/CONTRACT.md](../protocol/CONTRACT.md), [bin/add-decision.ps1](../bin/add-decision.ps1), [bin/check-decision-history.ps1](../bin/check-decision-history.ps1), [bin/build-rulings.ps1](../bin/build-rulings.ps1), [bin/ratify-decision.ps1](../bin/ratify-decision.ps1), [bin/initialize-decision-authority.ps1](../bin/initialize-decision-authority.ps1)
- AI Execution Prompts: [protocol/CONTRACT.md](../protocol/CONTRACT.md)

## [docs/node-map-usage-guide.md](../docs/node-map-usage-guide.md)

- Title: Node map usage guide
- Type: Operator Guide / Link-Health Workflow
- Context: Shows how the generated node map checks link health and where reading selection actually comes from.
- Next Sequential File: [specs/node-map.md](../specs/node-map.md)
- Related Workflows: [maps/node-roots.json](../maps/node-roots.json), [bin/update-node-map.ps1](../bin/update-node-map.ps1), [reference/INDEX.md](../reference/INDEX.md)
- AI Execution Prompts: [protocol/CONTRACT.md](../protocol/CONTRACT.md)

## [docs/recall-hooks.md](../docs/recall-hooks.md)

- Title: Prompt recall adapters and their verification limits
- Type: Guide / Hook Integration
- Context: Use when connecting a host's prompt event to the portable recall adapter, verifying its behavior, or falling back to explicit recall before assigned work.
- Next Sequential File: [reference/action-recall-gates.md](../reference/action-recall-gates.md)
- Related Workflows: [bin/brain-recall.ps1](../bin/brain-recall.ps1), [bin/hook-recall.ps1](../bin/hook-recall.ps1), [bin/prepare-work.ps1](../bin/prepare-work.ps1), [tests/recall.ps1](../tests/recall.ps1)
- AI Execution Prompts: [protocol/CONTRACT.md](../protocol/CONTRACT.md)

## [docs/upgrading.md](../docs/upgrading.md)

- Title: Upgrade the kernel while preserving your memory
- Type: Operator Guide / Upgrade
- Context: Exact, rehearsed commands for moving an existing private 0.1 memory to 0.2 without merging histories or touching your STATE, decisions, handoffs, gates or remote.
- Next Sequential File: [docs/decision-authority.md](../docs/decision-authority.md)
- Related Workflows: [CHANGELOG.md](../CHANGELOG.md), [docs/decision-authority.md](../docs/decision-authority.md), [docs/recall-hooks.md](../docs/recall-hooks.md), [protocol/RUNBOOK.md](../protocol/RUNBOOK.md)
- AI Execution Prompts: [protocol/CONTRACT.md](../protocol/CONTRACT.md)

## [employee-handbook.md](../employee-handbook.md)

- Title: Employee handbook
- Type: Public Themed Layer / Onboarding Texture
- Context: Use when you need the lore, taste and humor rules or metaphor-to-mechanism intent of the mdr-kernel themed layer; it is not operational authority, and STATE.md, the contract, the runbook and the scripts win.
- Next Sequential File: [FLOOR-GUIDE.md](../FLOOR-GUIDE.md)
- Related Workflows: [FLOOR-GUIDE.md](../FLOOR-GUIDE.md), [docs/brand-manual.md](../docs/brand-manual.md), [README.md](../README.md), [protocol/CONTRACT.md](../protocol/CONTRACT.md)
- AI Execution Prompts: [protocol/CONTRACT.md](../protocol/CONTRACT.md)

## [FLOOR-GUIDE.md](../FLOOR-GUIDE.md)

- Title: Floor Guide
- Type: Public Reference / Themed Glossary
- Context: Defines the contained workplace metaphor used by mdr-kernel and maps each themed alias to its real mechanism.
- Next Sequential File: [protocol/CONTRACT.md](../protocol/CONTRACT.md)
- Related Workflows: [README.md](../README.md), [docs/brand-manual.md](../docs/brand-manual.md), [protocol/CONTRACT.md](../protocol/CONTRACT.md), [bin/brain-recall.ps1](../bin/brain-recall.ps1)
- AI Execution Prompts: [protocol/CONTRACT.md](../protocol/CONTRACT.md)

## [GEMINI.md](../GEMINI.md)

- Title: GEMINI.md - mdr-kernel phase-zero door
- Type: Phase-Zero Agent Front Door
- Context: Pre-setup instructions for an agent opened in a fresh clone; setup regenerates this file into the full contract door.
- Next Sequential File: [protocol/CONTRACT.md](../protocol/CONTRACT.md)
- Related Workflows: [README.md](../README.md), [bin/setup-private.ps1](../bin/setup-private.ps1), [protocol/CONTRACT.md](../protocol/CONTRACT.md)
- AI Execution Prompts: [protocol/CONTRACT.md](../protocol/CONTRACT.md)

## [protocol/CONTRACT.md](../protocol/CONTRACT.md)

- Title: CONTRACT - the canonical mdr-kernel contract
- Type: Canonical Contract / Model Door Source
- Context: Canonical contract source that generates the model front doors and defines participation rules.
- Next Sequential File: [protocol/RUNBOOK.md](../protocol/RUNBOOK.md)
- Related Workflows: [bin/sync-doors.ps1](../bin/sync-doors.ps1), [protocol/RUNBOOK.md](../protocol/RUNBOOK.md)
- AI Execution Prompts: [protocol/CONTRACT.md](../protocol/CONTRACT.md)

## [protocol/RUNBOOK.md](../protocol/RUNBOOK.md)

- Title: RUNBOOK - how to start and end an mdr-kernel session
- Type: Operator Runbook / Session Workflow
- Context: Human-facing session startup, closeout, lock, and participant workflow guide.
- Next Sequential File: [docs/adoption-guide.md](../docs/adoption-guide.md)
- Related Workflows: [protocol/CONTRACT.md](../protocol/CONTRACT.md), [docs/adoption-guide.md](../docs/adoption-guide.md), [skills/mdr-kernel/SKILL.md](../skills/mdr-kernel/SKILL.md), [docs/brand-manual.md](../docs/brand-manual.md), [bin/start-session.ps1](../bin/start-session.ps1), [bin/close-session.ps1](../bin/close-session.ps1)
- AI Execution Prompts: [protocol/CONTRACT.md](../protocol/CONTRACT.md)

## [PUBLIC-EXPORT-AUDIT.md](../PUBLIC-EXPORT-AUDIT.md)

- Title: Public export audit
- Type: Generated Audit / Public Release Hygiene
- Context: Documents how the public mdr-kernel export was sanitized and verified.
- Next Sequential File: [README.md](../README.md)
- Related Workflows: [README.md](../README.md), [PUBLIC-EXPORT-AUDIT.md](../PUBLIC-EXPORT-AUDIT.md), [maps/NODE-MAP.md](../maps/NODE-MAP.md)
- AI Execution Prompts: [protocol/CONTRACT.md](../protocol/CONTRACT.md)

## [README.md](../README.md)

- Title: mdr-kernel
- Type: Public README / Coordination Kernel
- Context: Public orientation for a git-backed shared-memory kernel for AI coding agents.
- Next Sequential File: [FLOOR-GUIDE.md](../FLOOR-GUIDE.md)
- Related Workflows: [protocol/CONTRACT.md](../protocol/CONTRACT.md), [protocol/RUNBOOK.md](../protocol/RUNBOOK.md), [docs/adoption-guide.md](../docs/adoption-guide.md), [skills/mdr-kernel/SKILL.md](../skills/mdr-kernel/SKILL.md), [docs/brand-manual.md](../docs/brand-manual.md)
- AI Execution Prompts: [protocol/CONTRACT.md](../protocol/CONTRACT.md)

## [reference/action-recall-gates.md](../reference/action-recall-gates.md)

- Title: Action recall gates
- Type: Reference / Active-Recall Protocol
- Context: How to author and maintain objective-time recall gates so an agent surfaces recurrence-counted prior traps before acting.
- Next Sequential File: [reference/INDEX.md](../reference/INDEX.md)
- Related Workflows: [protocol/recall-gates.example.json](../protocol/recall-gates.example.json), [bin/brain-recall.ps1](../bin/brain-recall.ps1), [docs/recall-hooks.md](../docs/recall-hooks.md), [specs/brain-active-recall.md](../specs/brain-active-recall.md)
- AI Execution Prompts: [protocol/CONTRACT.md](../protocol/CONTRACT.md)

## [reference/INDEX.md](../reference/INDEX.md)

- Title: Reference index
- Type: Reference Index / Knowledge Registry
- Context: Registry of durable reference notes; the shared facts other sessions rely on.
- Next Sequential File: [STATE.md](../STATE.md)
- Related Workflows: [STATE.md](../STATE.md)
- AI Execution Prompts: [protocol/CONTRACT.md](../protocol/CONTRACT.md)

## [reference/markdown-system-node-standard.md](../reference/markdown-system-node-standard.md)

- Title: Markdown system node standard
- Type: Documentation Standard / Retrieval Envelope
- Context: Defines the required AI-crawlable header and terminal node block for tracked Markdown in an mdr-kernel repo.
- Next Sequential File: [reference/INDEX.md](../reference/INDEX.md)
- Related Workflows: [_templates/markdown-file-template.md](../_templates/markdown-file-template.md), [_templates/handoff-template.md](../_templates/handoff-template.md), [bin/update-node-map.ps1](../bin/update-node-map.ps1)
- AI Execution Prompts: [protocol/CONTRACT.md](../protocol/CONTRACT.md)

## [SECURITY.md](../SECURITY.md)

- Title: mdr-kernel security and privacy-bug reporting
- Type: Security Policy
- Context: How to report a vulnerability or a bug that could leak private memory content, privately and before public disclosure.
- Next Sequential File: [CONTRIBUTING.md](../CONTRIBUTING.md)
- Related Workflows: [CONTRIBUTING.md](../CONTRIBUTING.md), [README.md](../README.md), [bin/remote-privacy.ps1](../bin/remote-privacy.ps1), [tests/remote-privacy-unit.ps1](../tests/remote-privacy-unit.ps1)
- AI Execution Prompts: [protocol/CONTRACT.md](../protocol/CONTRACT.md)

## [skills/mdr-kernel/SKILL.md](../skills/mdr-kernel/SKILL.md)

- Title: mdr-kernel adoption and operations
- Type: Skill Definition / Workflow Instructions
- Context: Repeatable procedure for setting up, operating, and extending an mdr-kernel repo from an existing messy workspace.
- Next Sequential File: [_templates/skill-template.md](../_templates/skill-template.md)
- Related Workflows: [protocol/CONTRACT.md](../protocol/CONTRACT.md), [docs/adoption-guide.md](../docs/adoption-guide.md), [_templates/skill-template.md](../_templates/skill-template.md)
- AI Execution Prompts: [protocol/CONTRACT.md](../protocol/CONTRACT.md)

## [specs/brain-active-recall.md](../specs/brain-active-recall.md)

- Title: Spec — mdr-kernel active recall: closing the storage-vs-use gap
- Type: Spec / Active Architecture
- Context: Use when changing or explaining the objective-time recall gate: why storing knowledge is not enough, what a valid fix must satisfy and how the gate works (STATE.md and DECISIONS.md win for live status).
- Next Sequential File: [specs/INDEX.md](../specs/INDEX.md)
- Related Workflows: [specs/brain-architecture.md](../specs/brain-architecture.md), [STATE.md](../STATE.md), [DECISIONS.md](../DECISIONS.md), [reference/action-recall-gates.md](../reference/action-recall-gates.md)
- AI Execution Prompts: [protocol/CONTRACT.md](../protocol/CONTRACT.md), [bin/update-node-map.ps1](../bin/update-node-map.ps1)

## [specs/brain-architecture.md](../specs/brain-architecture.md)

- Title: Spec — mdr-kernel architecture: shared memory across model surfaces
- Type: Spec / Design Authority
- Context: Use when you need the design rationale of the shared-memory kernel (invariant, surfaces, guardrails, residual failure modes); verify live status against STATE.md and DECISIONS.md before acting.
- Next Sequential File: [specs/INDEX.md](../specs/INDEX.md)
- Related Workflows: [specs/brain-active-recall.md](../specs/brain-active-recall.md), [STATE.md](../STATE.md), [DECISIONS.md](../DECISIONS.md)
- AI Execution Prompts: [protocol/CONTRACT.md](../protocol/CONTRACT.md), [bin/update-node-map.ps1](../bin/update-node-map.ps1)

## [specs/INDEX.md](../specs/INDEX.md)

- Title: Specs index
- Type: Specs Index / Authority Registry
- Context: Registry of specs with current status and update dates.
- Next Sequential File: [STATE.md](../STATE.md)
- Related Workflows: [STATE.md](../STATE.md)
- AI Execution Prompts: [protocol/CONTRACT.md](../protocol/CONTRACT.md)

## [specs/node-map.md](../specs/node-map.md)

- Title: Node map
- Type: Spec / Link-Health Contract
- Context: Defines the generated node-map subsystem that keeps mdr-kernel link health small, inspectable, and accurate.
- Next Sequential File: [CONTRIBUTING.md](../CONTRIBUTING.md)
- Related Workflows: [maps/node-roots.json](../maps/node-roots.json), [maps/NODE-MAP.md](../maps/NODE-MAP.md), [maps/node-map.json](../maps/node-map.json), [bin/update-node-map.ps1](../bin/update-node-map.ps1)
- AI Execution Prompts: [protocol/CONTRACT.md](../protocol/CONTRACT.md)

---

## SYSTEM NODE LINKS

Directory Path: maps/
Next Sequential File: maps/NODE-MAP.md
Related Workflows: maps/node-map.json, maps/node-roots.json, specs/node-map.md, docs/node-map-usage-guide.md
AI Execution Prompts: bin/update-node-map.ps1
