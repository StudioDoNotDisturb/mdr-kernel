Title: GEMINI.md - mdr-kernel phase-zero door
Type: Phase-Zero Agent Front Door
Context: Pre-setup instructions for an agent opened in a fresh clone; setup regenerates this file into the full contract door.

# GEMINI.md - mdr-kernel phase-zero door

_This clone has not been set up yet. When `mdr-kernel setup-private` runs, this file is regenerated
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
   mdr-kernel setup-private -Model '<your model id>'
   ```

   For memory with no remote backup, add `-LocalOnly`. Setup verifies the destination is
   private, pins it, arms hooks, scaffolds the memory, regenerates the front doors, commits, and
   pushes the first checkpoint (a `-LocalOnly` memory commits without pushing).
3. **Run the one startup and require readiness:**

   ```powershell
   mdr-kernel start-session -Model '<your model id>'
   ```

   It must end with `STARTUP_STATUS=READY` before you write anything.

`mdr-kernel setup-private` is the only adopter entry point. `mdr-kernel bootstrap-clone` is an
internal setup/repair primitive - do not run it directly on first run. If `STATE.md` exists but
you are still reading this phase-zero text, the doors were never regenerated: run
`mdr-kernel sync-doors`.

---

## SYSTEM NODE LINKS

Directory Path: ./
Next Sequential File: protocol/CONTRACT.md
Related Workflows: README.md, native/src/onboard.rs, protocol/CONTRACT.md
AI Execution Prompts: protocol/CONTRACT.md
