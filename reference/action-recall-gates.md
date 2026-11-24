Title: Action recall gates
Type: Reference / Active-Recall Protocol
Context: How to author and maintain objective-time recall gates so an agent surfaces recurrence-counted prior traps before acting.

# Action recall gates

The action recall gate closes the storage-vs-use gap: a repo can enforce that knowledge is stored
correctly and still have no mechanical step that forces an agent to *use* the relevant knowledge
before acting. The gate is small and portable — every full participant runs one command against one
JSON file.

Run it after startup when an operator assigns work. A verified prompt adapter can surface the same
gate automatically; an unavailable, unverified or degraded hook requires the portable fallback:

```powershell
mdr-kernel prepare-work -Objective '<operator objective>'
```

If the result is `MATCHED`, read the listed sources before advising, editing, or running recovery
steps. Then say which gates matched, which traps recurred, and how that changes the next action. Do
not replace this with a private memory note or another prose reminder — discretionary reading is the
failure mode this gate exists to fix. A compact hook reminder marked `NEEDS_READ` preserves the gate
IDs and the expansion command; it does not mean the agent has already read their traps or sources.
See [Prompt recall adapters](../docs/recall-hooks.md) for configuration and verification limits.

## Gate data shape

The live gates live in `protocol/recall-gates.json` (bootstrap copies it from
`protocol/recall-gates.example.json`). The document has integer `version: 1` and a `gates` array
(which may intentionally be empty). Each gate has:

- `id`: unique within the registry; letters, digits, underscore or hyphen, at most 80 characters,
  beginning with a letter or digit.
- `label`: a nonempty human-readable name.
- `triggers`: objective words that require active recall.
- `aliases`: optional extra phrasings that should also match.
- `required_reads`: files to consult before acting.
- `times_recurred`: how many times the pattern has repeated.
- `traps`: recurrence-counted failures, each with the wrong move and the right move.
- `stop_signs`: behaviors that mean the agent is about to repeat the loop.

`triggers`, `required_reads`, `stop_signs` and optional `aliases` are arrays of nonempty strings;
empty arrays are allowed. `traps` is an array. Each trap has a globally unique safe `id`, an integer
`times_recurred`, and nonempty `symptom`, `wrong_move` and `right_move` strings. Optional `evidence_refs`
is an array of nonempty strings. Recurrence counts are nonnegative Int32 integers; an increment
that would overflow fails without writing. The file is limited to 2 MiB and 128 gates.

Optional `context_rules` supports a narrow paired match when either word alone is too broad:

```json
"context_rules": [
  { "any": ["remember", "recall"], "with": ["workflow", "handoff"] }
]
```

Each side must contain at least one nonempty string. A pair matches when at least one term from
each side matches; direct triggers and aliases continue to work independently. Tokens and phrases
use literal matching with boundaries that exclude word characters and hyphens on either side.
For example, `deploy` matches `(deploy)` but not `pre-deploy`. Add explicit aliases when a compound
is a useful signal rather than loosening every gate's boundary. Matching is case-insensitive and
Unicode-aware: letters in any script count as word characters, so a trigger inside unspaced text
(for example `部署` inside `请部署`) does not match; add the longer phrase as an alias when needed.

Optional `rulings` is an array of live decision IDs such as `["D7"]`. Only operator-ruled or
operator-ratified compiled entries can be shown. Build `protocol/rulings.json` with
`mdr-kernel build-rulings`; never hand-edit it. Recall verifies its source hashes and fails visibly for
a stale index or missing/unknown citation. Normal, JSON and `-Header` output include the complete
compiled rule; `-Brief` retains its ID and requires expansion. `-Ruling D7` or `-Ruling 'rule words'`
looks up a live rule directly. Generic gates with no citations do not require a ruling index, and
the shipped example gates cite none.

### Adding rulings to a gate

Once the decision log holds an operator-ruled or operator-ratified entry that governs a gate:

1. Add `"rulings": ["D<n>"]` to that gate (decision IDs only; proposals never qualify).
2. Run `mdr-kernel build-rulings` to compile `protocol/rulings.json`. The gate
   file is one of its hashed sources, so rebuild after every later gate edit, including `-Increment`.
3. Run `mdr-kernel brain-recall -CheckGates`, then an objective that should match;
   the output must show `Ruling in force: D<n> ...`. A `RECALL_STATUS=BLOCKED reason=invalid-rulings`
   result means the index is missing, stale or names an unknown ID.

Validate before using changed data:

```powershell
mdr-kernel brain-recall -CheckGates
mdr-kernel brain-recall -CheckGates -GatesPath protocol/recall-gates.example.json
```

`-GatesPath` is allowed only with `-CheckGates` and no other action. It cannot redirect matching,
rule lookup or recurrence writes. Schema validation is read-only and does not prove that a host
fires a prompt hook, that a trigger captures all relevant language, or that cited source content is
correct. Invalid JSON, wrong field types and duplicate IDs block evaluation; they never become a
healthy `CLEAR`.

## Recording a recurrence

When a trap repeats, acquire `STATE.lock` with your explicit model identity and retain the session
token before changing the gate file. Increment the count, then record the observed evidence in a
handoff; a work report does not become an operator ruling:

```powershell
mdr-kernel brain-recall -Increment <trapId>
```

It bumps the trap and its parent gate, sets `updated`, rewrites the JSON as UTF-8 without BOM with
LF line endings (other fields, including adopter extensions and key order, are kept), and reminds
you to commit under `STATE.lock` and record evidence. An unknown trap ID or a count at the Int32
maximum blocks with exit 2 and writes nothing. The command does not acquire the lock for you. If gates cite rulings, rebuild
the generated index after the gate data changes. A count without evidence is just another note. New recurring domains should get
a new gate rather than a larger startup dump. The design goal is the right knowledge at action time,
not more context at boot.

## Two seams

1. **Objective-time.** A verified prompt hook or `mdr-kernel prepare-work -Objective '…'` is required
   before assigned work and always when the operator says recurring, stuck, same loop, or amnesia.
   `mdr-kernel brain-recall` is the matching authority used by both adapters. Without verified hook firing,
   use the portable wrapper; for large objectives, pipe the complete text to its `-FromStdin`.
   The 131,072-character limit rejects overflow rather than matching only a prefix.
   The matcher's `-Brief` emits nothing on healthy `CLEAR` and compact mandatory gate information on
   a match. A `CLEAR` result is a prompt to reconsider objective wording, not a green light.
2. **Tool-time.** A fragile domain's own scripts can print the relevant gate's compact trap banner
   at invocation via `mdr-kernel brain-recall -GateId <id> -Header`, so the warning appears in output
   the agent caused itself. Invalid data or an unknown ID returns a nonzero status; the caller must
   surface that failure and require fallback before acting. Add a surfacing point to domain scripts
   when their first real recurring traps appear.

Residual risk: no script can force an agent to reason well. The gate makes skipping prior knowledge
visible and checkable across every model surface. When a trap still repeats, increment the count and
add a tighter surfacing point inside the domain tool — do not add another reminder paragraph.

---

## SYSTEM NODE LINKS

Directory Path: reference/
Next Sequential File: reference/INDEX.md
Related Workflows: protocol/recall-gates.example.json, native/src/recall.rs, docs/recall-hooks.md, specs/brain-active-recall.md
AI Execution Prompts: protocol/CONTRACT.md
