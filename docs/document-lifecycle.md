Title: [Living] Document lifecycle
Type: Workflow Guide
Context: Use when promoting working notes, retiring records, or moving an eligible document into the archive.

# Document lifecycle

The lifecycle state goes in the Title field, for example `Title: [Open] Migration plan`.
The engine reads the first six lines. It preserves the document apart from the
state in that field when it archives the file.

| State | When to use it |
|---|---|
| [Draft] | The document is still being written. Give it an exit destination before circulating it for use. |
| [Open] | The document is ready to drive work. Keep the exit destination current. |
| [Living] | The document is a maintained source of current guidance. |
| [Adopted?] | The content has landed in its exit destination. The archive decision is pending. |
| [Ingested] | The relevant content has been folded into its durable destination. |
| [Superseded] | A newer document has replaced this one. |
| [Rejected] | The proposal was declined. Keep the disposition with the document. |
| [Record] | The document records a past session or event. It can expire after 30 days without changes. |
| [Archived] | The document has moved under `archive/` with its repository-relative path preserved. |

## Find and archive documents

```powershell
mdr-kernel find-stale list
mdr-kernel archive-adopted -Adopt 'reference/old note.md'
mdr-kernel archive-adopted -Expired 'handoffs/2026-01-01-example.md'
```

`find-stale list` displays `[Ingested]`, `[Superseded]`, and `[Rejected]`
documents, plus old `[Record]` files with no main citations. Record age comes from the last Git
commit that touched the path. The working copy and Git index must agree with
HEAD, and strictly more than 30 days must have elapsed.

After the operator approves the exact path, hold your own STATE.lock with
your model and token to run `archive-adopted`. Use `-Adopt` for an adopted
`[Open]` or `[Adopted?]` document or for one of the three dispositions above.
Use `-Expired` for an old `[Record]`.

`archive-adopted` refuses citations to the named document in maintained
Markdown and protocol text files.
Markdown files with no recognized lifecycle state count as maintained. The
scan includes tracked files and unignored new files. A document's own
row in its directory's `INDEX.md` is a registry entry. The command redirects
that row to the archived path and updates its Status cell when present.
Handoffs directly under `handoffs/` need that row. Other documents can be
unindexed. Archive one file per call. Review the resulting Git changes for the
move and any updated registry row.

`bin/find-stale.ps1` and `bin/archive-adopted.ps1` use the installed native
engine. The same model and token environment variables used by
`mdr-kernel state-lock` apply to `archive-adopted`.

## SYSTEM NODE LINKS

Directory Path: docs/
Next Sequential File: ../protocol/RUNBOOK.md
Related Workflows: ../bin/find-stale.ps1, ../bin/archive-adopted.ps1
AI Execution Prompts: ../protocol/CONTRACT.md
