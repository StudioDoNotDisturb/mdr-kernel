Title: Contributing to mdr-kernel
Type: Public Contribution Guide
Context: Explains how to report cold-clone issues, portability gaps, and useful adaptations.

# Contributing to mdr-kernel

This is a **pattern**, not a product — a working skeleton for multi-model shared memory that you
copy and grow your own memory inside: always in a **private repo you own**, never in a public
fork (GitHub forks of public repos are always public, and your memory will contain your file
names, paths, and work history). Two ways to contribute:

## Adaptations (the main thing)

Generate a private repo using **Use this template**, clone your new repo, run
`mdr-kernel setup-private`, and change what does not fit. Fork only to send template
changes back — never to grow your own memory in. The pattern makes strong
assumptions (PowerShell, git, one live `STATE.md`, a heartbeat lock); if you adapt those to your
stack, an issue describing what you changed and why is genuinely useful. It tells the next person
which assumptions are load-bearing and which are just how this particular setup happened to work.

## Issues

Report things that should be reliable but are not:

- **Cold-clone breakage** — anything that fails on a fresh clone after the standard sequence:
  private repo from **Use this template** →
  `mdr-kernel setup-private -Model '<your model id>'` →
  `mdr-kernel start-session -Model '<your model id>'`.
  This path should always be green. If it is not, that is a bug.
- **Dangling references** — a doc that points at a file this repo does not ship.
- **Portability** — a script that assumes Windows paths where it should not, or a doctor check
  that hard-fails where it should guide.
- **Adoption gaps** — places where `docs/adoption-guide.md` or `skills/mdr-kernel/SKILL.md` leave
  a new user without a clear next step.

## The one rule

Every change keeps the core promise intact: **the repo is the only shared brain.** Nothing a
collaborator needs may live in a model's private memory, a past chat, or your head — it goes in a
file here, or it does not exist. If a contribution weakens that, it is out of scope regardless of
how useful it seems in isolation.

## Maintainer publishing path

The public repository is generated distribution, not an editing or adoption workspace. Maintainers
change the canonical template source, run the exporter into a fresh staging repository, run the
required hermetic battery and cold-clone tests there, review the candidate, and only then perform
the operator-gated promotion. The manually triggered `privacy live smoke` workflow is optional
integration evidence; push/PR CI does not depend on GitHub visibility APIs. Never bootstrap memory
in the launch repository, edit generated output in place, bypass its hooks, or use an adopter-only
escape valve to push upstream.

## Ground rules

- No secrets in tracked files — ever. The pre-commit scan enforces it.
- Keep `STATE.md` short and current; put detail in `handoffs/` and durable choices in
  `DECISIONS.md` (append-only, newest on top).
- Follow `docs/brand-manual.md` for terminal, docs, and visual style. Do not theme parser-facing
  status lines, exit codes, protected filenames, or required workflow names.
- Run `mdr-kernel doctor` before you open a PR. Green, or explain why not.

MIT licensed. By contributing you agree your contribution is licensed the same way.

---

## SYSTEM NODE LINKS

Directory Path: ./
Next Sequential File: README.md
Related Workflows: README.md, docs/brand-manual.md, protocol/RUNBOOK.md, native/src/doctor.rs
AI Execution Prompts: protocol/CONTRACT.md
