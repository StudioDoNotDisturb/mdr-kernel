Title: Spec — mdr-kernel active recall: closing the storage-vs-use gap
Type: Spec / Active Architecture
Context: Use when changing or explaining the objective-time recall gate: why storing knowledge is not enough, what a valid fix must satisfy and how the gate works (STATE.md and DECISIONS.md win for live status).

# Spec — mdr-kernel active recall

## 1. The problem, stated precisely

A shared-memory kernel can have a full stack of storage guardrails (R1–R11 in
`specs/brain-architecture.md`). **Every one of them protects knowledge STORAGE** — that facts are
correctly written, attributed, locked, indexed, wiki-validated, and size-bounded.

**None of them protects knowledge USE.** Whether an agent actually consults the relevant prior
knowledge *before acting on a recurring problem* is left to agent discretion, every session, fresh.

That is the bug. The repo becomes a near-perfect library with **no requirement that anyone read the
right book before acting** — so a documented trap does not stop the next session (same model or
another) from walking straight back into it. This is the cross-model / cross-session "amnesia." It is
logged as R12 in `specs/brain-architecture.md`.

## 2. Why the obvious fix is the failure mode

The instinct — "add a rule that says the agent must read X before this kind of work" — **is what
already exists and already fails.** A contract is full of "should read / prefer / before editing." A
cold session treats injected docs as *background*, not *gates*, and skims them. **Adding more prose
that says "remember" just adds another discretionary rule to the same pile.** Any proposal that
reduces to "a note telling the agent to remember" is disqualified by definition.

## 3. Constraints any solution must meet

A candidate fix is valid only if it satisfies all of these:

1. **Not "a note that says remember."** Discretionary reading is the disease, not the cure.
2. **Mechanical leverage or dead-simple convention.** It must live on a seam that is hard to skip
   (startup output, a git hook, the health check, or a tool's own runtime output) — or be a
   convention so small that every model reliably honors it. Hooks can block a *commit*; nothing can
   block an agent from *reasoning badly* — so the fix must attach to an action the harness can gate,
   or to the moment the action is attempted.
3. **Cross-model.** It cannot depend on one harness's private feature. The mechanism must be a
   repo-level artifact every surface can honor.
4. **Action-time, not session-start.** Knowledge must surface at the moment of the relevant action,
   not be dumped at boot. Session-start dumping is *why* current docs get skimmed.
5. **Recurrence must be visible.** The fix should make "this exact failure has recurred N times"
   loud and countable. The illusion of progress — a decisions log that reads as a string of victories
   — is part of the trap; surfacing the repeat count breaks it.
6. **No new context tax.** It must not bloat what every session reads at boot; that worsens the skim
   problem. Prefer narrow, triggered, on-demand surfacing.
7. **Self-applying.** The mechanism must protect its own domain too — it should be the kind of thing
   that would have caught this very amnesia problem.

## 4. Chosen mechanism — the active recall gate

The live mechanism has four parts:

1. **Objective-time recall gate.** `bin/brain-recall.ps1` is the single matching authority: it
   matches the assigned work against `protocol/recall-gates.json`. Two entry points reach it. A host
   with a verified `UserPromptSubmit` command hook runs `bin/hook-recall.ps1`, which passes the
   operator's own prompt to the matcher and injects a compact banner only on a match or a visible
   degradation. Every other surface, and any surface whose hook is unverified or degraded, runs the
   portable fallback `bin/prepare-work.ps1 -Objective '<operator objective>'` (or `-FromStdin`), which
   prints full recall and `PREPARE_WORK_STATUS=READY` only after a complete run. If gates match, the
   agent must read the listed sources before advising, editing, or guiding recovery, then state the
   matched traps and how they change the next action. Configuring a hook, passing the adapter tests
   and observing a live firing are separate evidence (see `docs/recall-hooks.md`).
2. **Recurrence-counted trap data.** Each gate and each trap carries `times_recurred`. Traps name the
   wrong move, the right move, and evidence references. A repeat increments the count
   (`bin/brain-recall.ps1 -Increment <trapId>`) instead of producing another "fixed" note — the count
   is the antidote to the illusion of progress. (Constraint 5.)
3. **Shared contract placement.** `bin/start-session.ps1` prints the recall contract line;
   `protocol/CONTRACT.md` requires the gate before assigned work; the generated full-participant
   front doors are synced from that source; `protocol/RUNBOOK.md` gives the operator-visible command.
   (Constraint 3: one repo-level artifact every surface honors.)
4. **Verification.** `bin/brain-doctor.ps1` fails when the matcher, adapter or fallback is missing
   or the gate schema is invalid (`-CheckGates`), and reports host hook configuration as configured
   with runtime firing unverified. Matching behavior itself — keyword, phrase, boundary, paired
   context, long-input, non-ASCII and unrelated-control cases that must stay `CLEAR` — is proven by
   `tests/recall.ps1`, which `bin/test-staged.ps1` runs when recall files are staged. This catches a
   regressed matcher that would silently return `CLEAR` for objectives it should match.

Matching is literal and deterministic (no inference service): single-token triggers match on a word
boundary that also excludes hyphens, multi-word triggers as phrases, plus a per-gate `aliases` list
and optional paired `context_rules`. Malformed gate data blocks evaluation instead of returning
`CLEAR`. A `CLEAR` result is an active prompt to reconsider the objective
wording, not a green light. (Constraints 2, 4, 6: no boot tax; triggered on demand; verified by the
health check.)

A second, optional seam is tool-time surfacing: a fragile domain's own script prints the relevant
gate's compact trap banner at invocation (`bin/brain-recall.ps1 -GateId <id> -Header`), so the
warning appears in output the agent caused itself. Invalid data or an unknown gate ID returns exit 2;
the calling script must surface that failure rather than hide it. This is the
strongest form of the pattern — the only thing an agent literally cannot skip is output it causes by
running the tool it needs. (Constraint 2.)

**Self-applying (Constraint 7):** the recurring-work gate matches objectives about repetition,
stuckness, and amnesia itself, so the recall system surfaces on the recall-failure domain too.

## 5. Residual

The gate can force surfacing, not obedience. It makes the missed-knowledge step visible,
health-checked, cross-model, and — with the tool-time seam — self-caused at action time. But an agent
can still read the output and reason badly. When that happens, the fix is to increment the relevant
recurrence count with evidence and add a tighter surfacing point, not to add another private-memory
note or reminder paragraph. This is R13 in `specs/brain-architecture.md`.

## 6. Adopting the gate

Bootstrap copies `protocol/recall-gates.example.json` to `protocol/recall-gates.json`. Keep the
recurring-work gate, replace the example domain gate with your own recurring traps as they surface,
and do not pre-invent gates for domains that have not failed yet. New recurring domains get a new
gate rather than a larger startup dump — the goal is the right knowledge at action time, not more
context at boot. Authoring and maintenance rules are in `reference/action-recall-gates.md`.

---

## SYSTEM NODE LINKS

Directory Path: specs/
Next Sequential File: specs/INDEX.md
Related Workflows: specs/brain-architecture.md, STATE.md, DECISIONS.md, reference/action-recall-gates.md
AI Execution Prompts: protocol/CONTRACT.md, bin/update-node-map.ps1
