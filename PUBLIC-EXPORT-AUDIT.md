Title: Public export audit
Type: Generated Audit / Public Release Hygiene
Context: Documents how the public mdr-kernel export was sanitized and verified.

# Public export audit - mdr-kernel

This repository is a **sanitized, fresh-history** export of the coordination pattern from a private
studio system. It was produced mechanically by an exporter, not hand-copied.

## How it was produced (2026-07-13)

- **Two content sources:** purpose-built public files (a cold-clone-passing doctor, a
  de-studio-ified contract/runbook, a bootstrap that scaffolds your own memory, the README +
  Floor Guide, publish hygiene, and the public enforcement scripts) copied verbatim; plus any
  still-generic private files that have not yet been templated, copied through a sanitize map.
- **Sanitize map** genericizes machine, user, brand, network, owner, email, and host strings to
  placeholders like `C:\Users\<you>` and `<your-github>`.
- **Hard post-scan:** the export is DELETED if any forbidden personal/operational string survives,
  so a bad edit cannot silently leak. This export passed the scan.
- **Fresh history:** the private commit log is discarded. This repo starts at one commit.
- **No secret values:** no credential-shaped strings are present; the pre-commit secret scan and a
  standalone scan both pass.

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