Title: AGENTS.md - mdr-kernel phase-zero door
Type: Phase-Zero Agent Front Door
Context: Pre-setup instructions for an agent opened in a fresh clone; setup regenerates this file into the full contract door.

# AGENTS.md - mdr-kernel phase-zero door

_This clone has not been set up yet. When `bin/setup-private.ps1` runs, this file is regenerated
from `protocol/CONTRACT.md` into the full agent front door._

## You woke up in a fresh mdr-kernel clone

There is no memory here yet (no `STATE.md`). Do not invent one, and do not work from chat
context. The safe first run is one setup command, then one startup:

1. **Confirm the venue with the operator.** This folder must be a clone of a PRIVATE repository
   the operator created from the mdr-kernel template (**Use this template -> Private**). Never
   grow memory in a clone of the public template itself, and never in a fork.
2. **Run setup once, identifying yourself.** Pass your real model id (a human driving by hand
   picks any short name instead):

   ```powershell
   pwsh -NoProfile -File bin/setup-private.ps1 -Model '<your model id>'
   ```

   For memory with no remote backup, add `-LocalOnly`. Setup verifies the destination is
   private, pins it, arms hooks, scaffolds the memory, regenerates the front doors, commits, and
   pushes the first checkpoint (a `-LocalOnly` memory commits without pushing).
3. **Run the one startup and require readiness:**

   ```powershell
   pwsh -NoProfile -File bin/start-session.ps1 -Model '<your model id>'
   ```

   It must end with `STARTUP_STATUS=READY` before you write anything.

`bin/setup-private.ps1` is the only adopter entry point. `bin/bootstrap-clone.ps1` is an
internal setup/repair primitive - do not run it directly on first run. If `STATE.md` exists but
you are still reading this phase-zero text, the doors were never regenerated: run
`pwsh -NoProfile -File bin/sync-doors.ps1`.

---

## SYSTEM NODE LINKS

Directory Path: ./
Next Sequential File: protocol/CONTRACT.md
Related Workflows: README.md, bin/setup-private.ps1, protocol/CONTRACT.md
AI Execution Prompts: protocol/CONTRACT.md
