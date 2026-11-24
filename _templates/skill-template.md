---
Title: <Skill Title>
Type: Skill Definition / Workflow Instructions
Context: <One sentence: what repeatable workflow this skill covers and when it fires.>
name: <skill-name>
description: >
  Use when the operator asks to <specific repeatable workflow or exact trigger phrase>.
  Do not use for <near miss that would be mistaken for this skill>.
compatibility: Requires repo access, Git, and the installed mdr-kernel binary. <Add any additional requirements.>
metadata:
  mdr_kernel:
    status: draft
    posture: read-first
    network: default-deny
    writes: proposal-only
---

# <Skill Title>

<!--
  SKILL RULES — read before filling in.

  A skill is a repeatable procedure the operator invokes by name. It requires a precise trigger,
  a clear boundary, and steps specific enough that an agent executes them cold without asking
  for clarification.

  A skill is NOT:
  - a vague preference or style note
  - a one-time or ad-hoc task
  - a tool or system you have not used yet (write a reference note in reference/ first)
  - a general explanation of a concept or technology

  Size budget: 40–80 lines.
  - Under 20 lines: incomplete. A skill without Gotchas and Verification is not ready to ship.
  - Over 100 lines: too broad. Split into two skills or move bulk detail to reference/.

  Quality bar:
  - When To Use must be specific enough to invoke without ambiguity.
  - Do Not Use When must name the near miss that wastes a session.
  - Every Procedure step must be a concrete command, file operation, or decision — not a summary.
  - Verification must name the exact check that confirms success. "Looks right" is not verification.
  - Gotchas must name at least one known failure mode. If none exist yet, the skill is not proven.

  YAML FRONTMATTER fields, filled in above:
  name:          Lowercase words separated by hyphens (e.g. release-checklist, asset-intake).
                 Must match the directory name under skills/.
  description:   Block scalar. Line 1: the specific trigger phrase or condition.
                 Line 2: the near miss to exclude.
  compatibility: Any requirements beyond repo access, Git, and the installed mdr-kernel binary.
  status:        draft | productive | retired
  posture:       read-first (default) | write-heavy
  network:       default-deny (default) | external-allowed
  writes:        proposal-only | lock-required-for-coordination | append-only
-->

## When To Use

- <Exact condition or operator phrase that triggers this skill.>
- <Add additional triggers if needed. Each must be unambiguous.>

## Do Not Use When

- <The near miss — a situation that looks like this skill but requires a different procedure.>
- <Add additional exclusions if needed.>

## Inputs Needed

- <File or state the model must read before starting.>
- <Operator-provided context required.>
- <Any external system state that must be verified before proceeding.>

## Procedure

1. Run canonical startup: `mdr-kernel start-session -Model '<actual model id>'`.
   Require `STARTUP_STATUS=READY` before proceeding.
2. Run active recall. A verified prompt hook runs `mdr-kernel hook-recall` automatically; otherwise,
   or when it reports DEGRADED, run
   `mdr-kernel prepare-work -Objective '<operator objective>'`.
   If a gate matches, read the listed sources before acting.
3. Read the smallest relevant file set chosen from `STATE.md` plus `reference/INDEX.md`. State what
   you selected and what you are intentionally not reading yet.
4. <First specific step of the actual workflow. Name the exact command or file operation.>
5. <Continue with concrete executable steps. Each step is a command, a file operation, or a
   decision with a named outcome.>
6. Verify the output against the Verification section below.
7. Write durable results back into the repo. Set `BRAIN_MODEL`, acquire `STATE.lock` and carry its
   token before editing coordination files. Run `mdr-kernel update-node-map` if Markdown structure
   changed.

## Gotchas

- <Known failure mode or edge case. Name the symptom and the correct response.>
- <If a recall gate covers this skill's domain, name it here.>

## Verification

- <Exact command, file state, or output that confirms the skill completed correctly.>
- <At least one check must be runnable, not subjective.>

## Outputs

- <File created or edited, with path.>
- <Command run and its expected result.>
- <Decision recorded and where (DECISIONS.md, handoff, inbox note).>

---

## SYSTEM NODE LINKS

Directory Path: skills/<skill-name>/
Next Sequential File: ../../docs/adoption-guide.md
Related Workflows: ../../docs/adoption-guide.md, ../../protocol/CONTRACT.md
AI Execution Prompts: ../../protocol/CONTRACT.md
