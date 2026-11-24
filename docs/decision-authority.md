Title: Decision authority and preserved history
Type: Operator Guide / Decision Integrity
Context: Exactly what the decision tools do: proposals, quoted rulings, human ratification, compiled recall, the hashing rule and explicit migration of an existing log.

# Decision authority

A work report belongs in a handoff. A proposed rule belongs in the decision log with
`**Authority:** proposed`. A ruling records something the operator actually said, with
`**Authority:** operator-ruled` and an `**Operator said (<surface>, YYYY-MM-DD):** "<quote>"`
line. The surface is one of `chat`, `terminal`, `operator terminal`, `in person`, `email` or
`phone`; the quote is at least 12 characters, in straight or curly double quotes. The agent must
not invent that quotation. These fields make authority reviewable; they do not cryptographically
prove who spoke on a shared account.

## What the checker enforces

`mdr-kernel check-decision-history` prints `DECISION_HISTORY_STATUS=OK ...` and exits 0, or prints
`DECISION_HISTORY_STATUS=FAIL` plus one `ERROR=` line per problem and exits 1. It checks:

- **Structure.** Headings `## D<n> - YYYY-MM-DD - Title` (a hyphen or an em dash), unique ids,
  strictly descending order, no gaps, ending at D1, exactly one `**Decision:**` body per entry.
  An empty log must say `_No decisions yet._` and contain no decision body.
- **Authority.** Every entry from the authority boundary on has exactly one `**Authority:**` line
  with `operator-ruled` (plus the quote line) or `proposed`. `operator-ratified` is never typed
  ("never typed"); `record` is refused ("a work report belongs in a handoff").
- **Supersession as data.** `**Supersedes:**`, `**Supersedes in part:**` and `**Retires:**` list
  older ids. Each target carries a matching `_Status: <kind> by D<n> (YYYY-MM-DD)_` stamp directly
  under its heading, dated with the superseding entry, and every stamp is backed by such a field.
  A proposal may supersede only an unratified proposal and may retire nothing ("is proposed and
  cannot supersede" / "cannot retire"): a ruling yields only to a ruling.
- **Append-only history.** With `-FromIndex` (pre-commit) the staged log is compared with HEAD;
  with `-BaselinePath` it is compared with that file. Each existing entry must be byte-identical
  except for added stamp lines; an edit or deletion fails ("differs from HEAD", "was removed").
  Record a correction as a new entry.
- **Ratifications.** Each line of `protocol/decision-ratifications.jsonl` is
  `{id, entry_sha256, ratified_at, quote, surface}` and must match the current hash of its entry
  ("no longer matches the hash it was ratified at"). The ledger is append-only against HEAD (or
  `-RatificationsBaselinePath`). The live ledger binds only the canonical `DECISIONS.md`; a `-Path`
  fixture is checked against `-RatificationsPath` or no ledger.

**The hashing rule** (one rule, used by the checker, `mdr-kernel ratify-decision` and `-HashOf`): take
the entry from its heading to the next heading, or for the oldest entry to the
`<!-- Template for a new entry` comment or the `---` line before `## SYSTEM NODE LINKS`; remove
stamp lines; fold CRLF to LF; drop trailing whitespace; SHA-256 the UTF-8 bytes. A BOM, line-ending
style, a later supersession stamp or a footer edit never changes it. Print it with
`mdr-kernel check-decision-history -HashOf D1`.

## Adding an entry

Hold your own sealed `STATE.lock` (`-Model`/`-LockToken` or `BRAIN_MODEL`/`BRAIN_LOCK_TOKEN`)
before changing the canonical log. Write the body to a file and run:

```powershell
mdr-kernel add-decision -Title 'A concrete decision' -BodyPath proposal.md
```

A proposal body can be as small as:

```markdown
**Authority:** proposed

**Decision:** Preserve the source receipt with each imported record.
```

`mdr-kernel add-decision` validates the existing log, prepends the next id (replacing
`_No decisions yet._` on an empty log), stamps every target named in a supersession field, keeps
the file's line endings and BOM, validates the result and restores the original bytes on failure
("rolled back"). It prints `DECISION_APPEND_STATUS=OK id=D<n>`; any refusal exits 1. Only an append
to the canonical `DECISIONS.md` rebuilds `protocol/rulings.json`; a `-Path` fixture needs no lock
and leaves rulings alone. IDs stay consecutive and newest first. Do not use the log for builds,
batches, approvals of individual artifacts, or ordinary session choices.

## Ratification is a human action

An operator can ratify an entry after reviewing its exact contents. In their own terminal,
with their own identity and acquired lock token, the human runs:

```powershell
mdr-kernel ratify-decision -Id D1 -Model '<operator identity>'
```

The tool refuses before reading anything, prints `RATIFICATION_STATUS=REFUSED` with the reason
and exits 3 unless standard input and output are attached to an interactive terminal. Otherwise it shows the heading and
hash, asks for optional words and the exact phrase `RATIFY D1` (a mismatch exits 1 and writes
nothing), verifies the lock, re-hashes the entry, runs the checker, appends one record, and
rebuilds rulings. An entry already ratified at the same hash prints `RATIFICATION_STATUS=ALREADY`.
Agents never run this command through a terminal they control and never write the ledger. This is
a cooperative boundary; a process with the same account's filesystem access can defeat it
deliberately. It is not a security sandbox or an identity-signing service.

## Rules reach the next task

`mdr-kernel build-rulings` runs the checker, then compiles `protocol/rulings.json`. A live ruling is
not superseded or retired and is either hash-ratified (`operator-ratified`) or carries
`**Authority:** operator-ruled` with an `**Operator said (...)**` line (`operator-ruled`). The full
Decision body is kept; a superseded-in-part ruling stays with a note. Proposals, unclassified
legacy entries, and retired or fully superseded entries are excluded. A recall gate may cite only
live ids in its `rulings` array; any other citation fails with "not live rulings". `-Check` exits
1 with `RULINGS_STATUS=STALE` when the file no longer matches its sources.

After changing gates or decision sources, compile and stage the generated output together with
its sources. Pre-commit runs the checker and `mdr-kernel build-rulings -Check` against the index. Recall
checks source hashes before presenting rulings. A compact NEEDS_READ output is a reading
instruction, never a claim that the sources have been read.

```powershell
mdr-kernel build-rulings
mdr-kernel brain-recall -Ruling D1
```

## Existing logs require an explicit boundary

`protocol/decision-authority.json` (`{"schema_version":1,"authority_from":<n>}`) says where
authority enforcement starts. It applies to the canonical log and to pre-commit, never to a
`-Path` fixture. Setup writes `authority_from` 1 for a new empty log. An existing log is not
rewritten or ratified automatically; without a boundary its entries fail the authority check
until you migrate. Commit the history unchanged, review it, and with your own lock run:

```powershell
mdr-kernel initialize-decision-authority -PreserveLegacy
```

It refuses without your lock, refuses an existing boundary ("immutable"), refuses an existing
history without `-PreserveLegacy`, refuses a `DECISIONS.md` that is not committed or differs from
HEAD ("Commit existing history unchanged"), and refuses a structurally invalid history. On success
it writes `authority_from` = newest id + 1 (1 for an empty log), rebuilds rulings and prints
`AUTHORITY_INITIALIZED from=D<n> legacy=unclassified`. Pre-commit then refuses a boundary that
exempts entries not already in HEAD, and refuses any later change or deletion of the committed
boundary. Existing entries still need unique, consecutive ids in descending order and valid
Decision bodies; malformed histories need deliberate repair outside this path.

Unclassified legacy entries remain historical evidence, not compiled rules. An old entry that
already carries `**Authority:** operator-ruled` and an `**Operator said (...)**` line is compiled as a
ruling, but the checker does not validate entries below the boundary, so review that
classification before migration. To adopt an old idea as a current rule, have the operator rule
on it explicitly in a new entry. See [upgrading.md](upgrading.md) for safe integration.

---

## SYSTEM NODE LINKS

Directory Path: docs/
Next Sequential File: upgrading.md
Related Workflows: protocol/CONTRACT.md, native/src/decisions.rs
AI Execution Prompts: protocol/CONTRACT.md
