Title: Handoff - YYYY-MM-DD - <short topic>
Type: Handoff / Session Closeout
Context: End-of-session transfer note that lets another full participant resume the work cold.

# Handoff - YYYY-MM-DD - <short topic>

<!--
  HANDOFF RULES — read before filling in.

  Purpose: this file is the record that lets the next full participant resume without asking you
  anything. Write for a model that arrives cold, has no chat history, and must trust only the repo.

  Size budget: 40–70 lines, 2,000–5,000 characters.
  - Under 30 lines: probably missing a section. Check completeness.
  - Over 80 lines: probably a narrative dump. Move bulk evidence to reference/ or specs/ and
    summarize here. A handoff is a transfer note, not a research report.

  Quality bar:
  - Every section that applies must be filled in. Do not leave placeholder bullets.
  - "Current state" must match STATE.md. STATE wins live conflicts; correct stale handoff text.
    Integrate new verified evidence into STATE only while holding your own STATE.lock.
  - "Next actions" must be concrete enough to execute without clarification.
  - Do not write a session narrative. Write outcomes, decisions, and resume instructions.

  Filing:
  1. Save this file as `handoffs/YYYY-MM-DD-<short-topic>.md`.
  2. Add a one-line row to `handoffs/INDEX.md`.
  3. Update `STATE.md` to reflect current state and next actions.
  4. Run `bin/close-session.ps1 -Topic <short-topic> -Model <your-model-id>` to commit, push, and release STATE.lock.
-->

## What landed this session

(Bullet list of completed outcomes — not activities. "Rewrote X" not "worked on X".
Include the specific files, commands, or decisions that are now in the record.)

-

## Current state

(One cold-resumable paragraph. Answer: what is true right now, what is working, what is not.
This must match STATE.md. Correct stale handoff text when they disagree; update STATE only
to integrate new verified evidence under your own lock.)

## Decisions made

(Quoted operator rulings and durable proposals use bin/add-decision.ps1 with the appropriate
authority. Proposals are not rules. Work reports and session choices stay in this handoff;
label choices "Session choices (not rulings)". Never write the human ratification ledger.)

-

## Open questions / blockers

(Things that are unresolved and will affect the next session. If none, write "None.")

-

## Operational continuity

(Required if any service, deploy target, external system, or running process was touched.
Omit only for pure doc or code sessions with no live system involvement.)

- Last successful diagnostic command + timestamp:
- Connection state (network / auth / service):
- Runtime/deploy method and current version:
- Live-verified vs. only staged/built:
- Exact resume command or UI path:
- Operator-only step still required:

## Next actions

(In priority order. Each item must be concrete enough to execute without asking anyone.
Use `[ ]` checkboxes. The top item is what the next session starts with.)

- [ ]

## Files edited

(Coordination files edited under STATE.lock should be marked with `[lock]`.)

-

## Files read but not edited

(Keep this short — only files that are directly relevant to resuming the work.)

-

---

## SYSTEM NODE LINKS

Directory Path: handoffs/
Next Sequential File: handoffs/INDEX.md
Related Workflows: STATE.md, DECISIONS.md, protocol/RUNBOOK.md
AI Execution Prompts: protocol/CONTRACT.md
