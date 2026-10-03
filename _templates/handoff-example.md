Title: Handoff — 2026-01-15 — first recall gate for the deploy loop
Type: Handoff / Session Closeout (EXAMPLE)
Context: Use when writing a handoff and you want a worked example of the expected shape, altitude and completeness; it is not a live handoff, so never place it in handoffs/.

# Handoff — 2026-01-15 — first recall gate for the deploy loop

<!--
  THIS IS AN EXAMPLE, not a live handoff. It ships beside the blank handoff-template.md so an
  adopter can see one high-quality filled note. Do not copy it into handoffs/; start a real handoff
  from _templates/handoff-template.md.

  Notice what it does: outcomes not activities, a Current state that would match STATE.md, a real
  decision that also lands in DECISIONS.md, and Next actions concrete enough to run cold.
-->

## What landed this session

- Added the first domain recall gate, `DEPLOY-CONTAINER`, to `protocol/recall-gates.json`: three
  triggers (`redeploy`, `container restart`, `stuck deploy`), one trap counting the "restart the
  service from the dashboard instead of rebuilding the image" mistake (`times_recurred: 2`).
- Wired the tool-time banner into `bin/deploy.sh` so it prints the gate header at invocation via
  `bin/brain-recall.ps1 -GateId DEPLOY-CONTAINER -Header`.
- Recorded the choice as a `DECISIONS.md` entry (newest on top).
- Verified the gate matches: `bin/prepare-work.ps1 -Objective 'the deploy is stuck again'` returned
  `RECALL_STATUS=MATCHED` and listed the trap and required reads.

## Current state

The recall gate now covers the deploy loop that had recurred twice. `brain-doctor` is green, hooks
are armed, and `STATE.lock` is released. `protocol/recall-gates.json` has two gates: the shipped
recurring-work gate and the new `DEPLOY-CONTAINER` gate. No live deploy was run this session — only
the gate wiring and its self-test. This matches the "Active work" row in STATE.md.

## Decisions made

- **Deploy recurrences increment the trap count, not spawn new "fixed" notes.** The correct redeploy
  path is rebuild-image, not dashboard-restart; that is now the gate's right-move. Logged in
  `DECISIONS.md`.

## Open questions / blockers

- None blocking. Watch item: if the deploy trap recurs a third time despite the banner, add a tighter
  surfacing point inside `bin/deploy.sh` rather than another reminder line.

## Operational continuity

- Last successful diagnostic command + timestamp: `bin/brain-doctor.ps1` → ok, 2026-01-15 16:40.
- Connection state (network / auth / service): n/a — no external service touched.
- Runtime/deploy method and current version: `bin/deploy.sh` (image rebuild path); not run this
  session.
- Live-verified vs. only staged/built: gate wiring committed and self-tested; deploy path not
  re-run.
- Exact resume command or UI path: `pwsh -NoProfile -File bin/start-session.ps1 -Model <id>`.
- Operator-only step still required: none.

## Next actions

- [ ] On the next real deploy, follow the gate's right-move (rebuild image) and confirm the banner
      printed at invocation.
- [ ] If the trap recurs, run `bin/brain-recall.ps1 -Increment <trapId>` and add a tighter surfacing
      point in `bin/deploy.sh`.
- [ ] Add a second domain gate only when a second domain has actually failed twice — do not
      pre-invent gates.

## Files edited

- `protocol/recall-gates.json` [lock]
- `bin/deploy.sh`
- `DECISIONS.md` [lock]
- `STATE.md` [lock]

## Files read but not edited

- `reference/action-recall-gates.md`
- `specs/brain-active-recall.md`

---

## SYSTEM NODE LINKS

Directory Path: _templates/
Next Sequential File: _templates/handoff-template.md
Related Workflows: STATE.md, DECISIONS.md, protocol/RUNBOOK.md
AI Execution Prompts: protocol/CONTRACT.md
