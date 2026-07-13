Title: Action recall gates
Type: Reference / Active-Recall Protocol
Context: How to author and maintain objective-time recall gates so an agent surfaces recurrence-counted prior traps before acting.

# Action recall gates

The action recall gate closes the storage-vs-use gap: a repo can enforce that knowledge is stored
correctly and still have no mechanical step that forces an agent to *use* the relevant knowledge
before acting. The gate is small and portable — every full participant runs one command against one
JSON file.

Run it after startup when an operator assigns work:

```powershell
pwsh -NoProfile -File bin/brain-recall.ps1 -Objective '<operator objective>'
```

If the result is `MATCHED`, read the listed sources before advising, editing, or running recovery
steps. Then say which gates matched, which traps recurred, and how that changes the next action. Do
not replace this with a private memory note or another prose reminder — discretionary reading is the
failure mode this gate exists to fix.

## Gate data shape

The live gates live in `protocol/recall-gates.json` (bootstrap copies it from
`protocol/recall-gates.example.json`). Each gate has:

- `triggers`: objective words that require active recall.
- `aliases`: extra phrasings that should also match.
- `required_reads`: files to consult before acting.
- `times_recurred`: how many times the pattern has repeated.
- `traps`: recurrence-counted failures, each with the wrong move and the right move.
- `stop_signs`: behaviors that mean the agent is about to repeat the loop.

## Recording a recurrence

When a trap repeats, increment the count instead of writing another "fixed" note, then record the
evidence in a decision or handoff:

```powershell
pwsh -NoProfile -File bin/brain-recall.ps1 -Increment <trapId>
```

It bumps the trap and its parent gate, writes the JSON, and reminds you to commit under `STATE.lock`
and record evidence. A count without evidence is just another note. New recurring domains should get
a new gate rather than a larger startup dump. The design goal is the right knowledge at action time,
not more context at boot.

## Two seams

1. **Objective-time.** `bin/brain-recall.ps1 -Objective '…'` is required before assigned work and
   always when the operator says recurring, stuck, same loop, or amnesia. Matching is tolerant:
   single-token triggers match on a word boundary, multi-word triggers as phrases, plus the per-gate
   `aliases` list. A `CLEAR` result is an active prompt to reconsider your objective wording, not a
   green light.
2. **Tool-time.** A fragile domain's own scripts can print the relevant gate's compact trap banner
   at invocation via `bin/brain-recall.ps1 -GateId <id> -Header`, so the warning appears in output
   the agent caused itself. The call is advisory and never fatal. Add the same one line to new
   fragile-domain scripts as their first real traps surface.

Residual risk: no script can force an agent to reason well. The gate makes skipping prior knowledge
visible and checkable across every model surface. When a trap still repeats, increment the count and
add a tighter surfacing point inside the domain tool — do not add another reminder paragraph.

---

## SYSTEM NODE LINKS

Directory Path: reference/
Next Sequential File: reference/INDEX.md
Related Workflows: protocol/recall-gates.example.json, bin/brain-recall.ps1, specs/brain-active-recall.md
AI Execution Prompts: protocol/CONTRACT.md
