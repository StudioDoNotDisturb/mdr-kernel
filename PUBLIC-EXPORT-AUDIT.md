Title: Public export audit
Type: Generated Audit / Public Release Hygiene
Context: Documents how the public mdr-kernel export was sanitized and verified.

# Public export audit - mdr-kernel

This repository is a **sanitized, fresh-history** export of the coordination pattern from a private
studio system. It was produced mechanically by an exporter, not hand-copied.

## How it was produced (2026-10-03)

- **Two content sources:** purpose-built public files (a cold-clone-passing doctor, a
  de-studio-ified contract/runbook, a bootstrap that scaffolds your own memory, the README +
  Floor Guide, publish hygiene, and the public enforcement scripts) copied verbatim; plus any
  still-generic private files that have not yet been templated, copied through a sanitize map.
- **Sanitize map** genericizes machine, user, brand, network, owner, email, and host strings to
  placeholders like `C:\Users\<you>` and `<your-github>`.
- **Hard post-scan:** the initial commit is refused if any forbidden personal/operational string
  survives. Failed candidates remain local for inspection and must not be published. Only a
  successful exporter exit marks this candidate as ready for release review.
- **Fresh history:** the private commit log is discarded. This repo starts at one commit.
- **Secret checks:** scanner coverage and a standalone secret scan must pass before the initial
  commit is created.

## What is NOT included

The live system's memory - its STATE, decisions, handoffs, reference notes, inbox, private node
maps, and studio-specific lifecycle scripts - is not exported. This export generates its own clean
node map from the public template registry. This is the coordination machinery you grow your own
memory inside, not the memory itself.

---

## SYSTEM NODE LINKS

Directory Path: ./
Next Sequential File: README.md
Related Workflows: README.md, PUBLIC-EXPORT-AUDIT.md, maps/NODE-MAP.md
AI Execution Prompts: protocol/CONTRACT.md