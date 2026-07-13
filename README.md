Title: mdr-kernel

Type: Public README / Coordination Kernel

Context: Public orientation for a git-backed shared-memory kernel for AI coding agents.

# mdr-kernel

*A shared-memory kernel for AI agents that wake up with no trustworthy private context.*

> **The work is the data. The data is the work.**

`mdr-kernel` is a shared-memory environment for AI coding agents. It enforces context isolation by
treating the repository — not the chat memory — as the canonical state.

## The Protocol

- **Severed State:** Agents wake up with no private memory. They trust the repository record over
  the chat history.
- **Node-Map Routing:** Agents route through an explicit node map, preventing broad, erratic repo
  crawls.
- **Active Recall:** Agents must verify their reasoning against past traps (the "Break Room")
  before implementing new objectives.
- **Authoritative Commits:** Every change is attributed to a specific model, audited for security,
  and locked by heartbeat.

## Design Philosophy

`mdr-kernel` is engineered for clinical precision. It is built to make uncoordinated work difficult
and standardized, state-aware refinement the path of least resistance. The metaphor of "severance"
is used to describe this context isolation; the implementation is pure, plain, and intentionally
boring engineering.

For the full philosophical mapping, see [FLOOR-GUIDE.md](FLOOR-GUIDE.md).

## [ MANIFEST ] What You Get

```
mdr-kernel/
|- README.md                  public orientation
|- protocol/CONTRACT.md       the model-facing contract, generated into front doors
|- protocol/RUNBOOK.md        human startup and closeout workflow
|- bin/start-session.ps1      canonical startup: doctor -> safe pull -> STATE readback
|- bin/brain-doctor.ps1       health check for stale state, links, locks, hooks, secrets
|- bin/brain-recall.ps1       active-recall gate for recurring mistakes
|- bin/state-lock.ps1         heartbeat lock for coordination files
|- bin/update-node-map.ps1    generated retrieval map and node-link checker
|- bin/setup-private.ps1      the one adopter entry: verify private destination -> scaffold -> push
|- bin/bootstrap-clone.ps1    internal scaffold/repair primitive (setup-private calls it)
|- CLAUDE.md / AGENTS.md / GEMINI.md  phase-zero agent doors; setup regenerates them from the contract
|- bin/waffle-party.ps1       optional themed wrapper around brain-doctor
|- maps/node-roots.json       registry of roots to scan
|- maps/NODE-MAP.md           generated human routing dashboard
|- maps/node-map.json         generated machine-readable routing database
|- .githooks/                 pre-commit and commit-message enforcement
|- specs/node-map.md          retrieval-layer contract
|- docs/adoption-guide.md     first-hour path for messy existing work
|- docs/node-map-usage-guide.md practical routing workflow
|- docs/brand-manual.md       visual, voice, and terminal UX rules
|- skills/mdr-kernel/         starter skill for adopting and operating the kernel
|- _templates/                Markdown, handoff, and skill starters
`- FLOOR-GUIDE.md             contained metaphor map, not the product surface
```

Setup creates the live memory files that should be yours, not mine: `STATE.md`,
`DECISIONS.md`, `handoffs/INDEX.md`, `reference/INDEX.md`, `specs/INDEX.md`,
`protocol/recall-gates.json`, the fully generated model doors, and refreshed node maps.

## [ MACRODATA ] The Loop

The refinement cycle — `mdr` is *Macrodata Refinement*, and this is the refinement. Each step is
logged; none is skippable.

1. `STATE.md` is the live truth. If anything disagrees with it, `STATE.md` wins and the stale
   source gets fixed.
2. `protocol/CONTRACT.md` is the one doctrine every model receives. Claude, Codex, Gemini, local
   models, or any other CLI agent all start from the same rules.
3. `bin/start-session.ps1` is the only startup path. It runs `brain-doctor`, refuses unsafe pulls
   from a dirty tree, and prints the full current `STATE.md`.
4. `maps/NODE-MAP.md` and `maps/node-map.json` route the agent to the smallest relevant file set
   before broad search.
5. `bin/brain-recall.ps1` runs before assigned work. When an objective matches a known trap, the
   agent must read the evidence before acting.
6. `STATE.lock` protects coordination files. The pre-commit hook blocks protected edits unless
   the session holds the lock.
7. Handoffs and decisions keep history useful without stuffing the live state file.

Deliberately boring infrastructure: the point is not to make agents *sound* coordinated, but to
make uncoordinated work difficult to commit.

## [ PRIVACY ] Start With Your Own Private Repository

This public repository is distribution, **not** the place where your memory grows. The supported
GitHub path is deliberately singular:

1. Click **Use this template** -> **Create a new repository**.
2. Choose **Private** visibility. Do not fork; forks of a public repository are public.
3. Clone the new private repository you own, then run setup inside that clone.

`gh` users can do steps 1-2 in one command:
`gh repo create <you>/<your-private-repo> --private --template StudioDoNotDisturb/mdr-kernel`

**No GitHub at all? Also supported.** Clone or copy the template, then run setup with
`-LocalOnly`: it removes `origin` before any memory exists and the kernel stays entirely on
your machine. Those are the two venues — a private repository you own, or no remote at all.
Never grow memory in a clone of the public template itself.

```text
PUBLIC TEMPLATE
      |  Use this template -> Private
      v
YOUR PRIVATE REPOSITORY
      |  clone
      v
YOUR LOCAL CANONICAL CLONE -> close-session -> YOUR PRIVATE origin/main
```

Your private repository's `main` branch is the shared brain. You normally do not run `git push`
yourself: `close-session` checkpoints the session and pushes that private `origin/main` after
re-verifying its visibility and destination pin. If another private checkpoint landed first,
close-session performs one safe pull/rebase and one push retry; it never loops.

Everything the kernel grows can contain private paths and work history. Setup refuses the public
template, public GitHub repos, forks, changed remotes, and unverifiable GitHub visibility.
On GitHub Enterprise, keep `gh` installed and authenticated so setup can distinguish PRIVATE from
INTERNAL; the no-`gh` fallback proves only that a repository is not publicly visible.

## [ ORIENTATION ] Which File Do I Follow?

**This README, top to bottom, is the only file you follow on first launch.** Everything else
comes after setup:

- `protocol/RUNBOOK.md` — the daily session loop (start, work, close) once you are set up.
- `CLAUDE.md` / `AGENTS.md` / `GEMINI.md` — front doors for your AI tools, written for the agent,
  not for you. A fresh clone ships them as **phase-zero doors** that route an agent straight to
  setup; setup regenerates them into the full contract doors.
- `docs/adoption-guide.md` — read before moving existing work in.
- `FLOOR-GUIDE.md` and `employee-handbook.md` — philosophy and flavor; nothing operational.

## [ INTAKE ] From Template to READY

Requires **PowerShell 7+** (`pwsh`) and **git** — install those first. The scripts are
intentionally plain PowerShell so the pattern can move between Windows, macOS, Linux, local
models, and subscription CLI agents.

```text
Windows:  winget install Microsoft.PowerShell Git.Git
macOS:    brew install powershell git
Linux:    see Microsoft's "Installing PowerShell on Linux" page for your distro; git via your package manager
```

On macOS/Linux, every `bin/*.ps1` ships with an extension-less POSIX twin — call
`./bin/setup-private`, `./bin/start-session`, and so on, and they launch `pwsh` for you.

The whole path is: **template -> private clone -> open agent -> setup -> one startup.**

**Let the agent drive (recommended).** Open your AI tool with the fresh clone as its working
directory and hand it:

```text
Read your front door (CLAUDE.md / AGENTS.md / GEMINI.md) and follow it: run setup with your
model id, run the one startup, and report STARTUP_STATUS. Then ingest <one small folder>
following docs/adoption-guide.md — inventory first, file each item deliberately, no wholesale
moves. Close the session with a handoff when done.
```

The phase-zero door walks the agent through the same two commands below. Startup runs **once**;
setup does not require a separate human pass first.

**Or set up by hand.** Inside the private repository you cloned, run:

```powershell
pwsh -NoProfile -File bin/setup-private.ps1 -Model 'chad'
```

`-Model` is the attribution name stamped on this clone's commits and locks — type it without
angle brackets. Setting up by hand, no agent driving? Pick any short name yourself (your own
works fine). An AI agent running setup passes its real model id instead, and every agent that
works here later identifies itself the same way.

On macOS/Linux, run `./bin/setup-private -Model 'chad'`. Setup verifies that GitHub
reports the destination private, pins it in local git config, arms hooks, creates the starter
memory, and commits the scaffold. **Run setup once in every clone**, including a re-clone or a
second computer: the destination approval and hooks intentionally live in that clone's local
`.git/config`. Setup is safe to rerun. Use a real clone; ZIP/no-`.git` copies are unsupported.

Expected result:

```text
READY
Mode: private-remote
Canonical branch: main
Approved destination: github:<you>/<your-private-repo>
```

Then the one startup — run it yourself to verify, or let your agent run it as its session start
(its front door is now the full contract door):

```powershell
pwsh -NoProfile -File bin/start-session.ps1 -Model '<your-model-id>'
```

It should end with `STARTUP_STATUS=READY`. (An agent passes its real model id; a human uses
the short name they chose at setup.)

If you must stop while offline, add `-OfflineCheckpoint` to the normal close-session command.
This exception works only after private-remote setup: it commits locally, never tries to push,
and makes every later startup print a loud pending-sync warning. When connectivity returns, run:

```powershell
pwsh -NoProfile -File bin/sync-private.ps1
```

That command is the one push implementation used by setup, normal close, and recovery. It
re-verifies the destination's live privacy, rebases safely, pushes explicit `origin/main`, and
allows only one bounded retry if the remote moves again. The stored destination pin is never
treated as permission to push. Local-only users close normally and do not use either offline flag
or sync-private.

For deliberately local-only memory, use `setup-private.ps1 ... -LocalOnly`; setup removes
`origin` before memory is created. Non-GitHub private hosts require `-ConfirmPrivateRemote`
because their visibility cannot be checked generically.

Do not dump existing notes, repos, and half-sorted project folders into this repo wholesale. After
quick start, follow the [Adoption Guide](docs/adoption-guide.md): inventory first, then file each
item as live state, reference note, spec, handoff, inbox note, or skill. The shipped
[mdr-kernel skill](skills/mdr-kernel/SKILL.md) is the repeatable procedure an agent runs for that
intake.

For a compliance reading of a chaotic environment, run the optional diagnostic wrapper:

```powershell
pwsh -NoProfile -File bin/waffle-party.ps1
```

It is `brain-doctor` with a themed success footer. If the floor is not in compliance, it returns
the same non-zero exit code as the doctor.

## Why Not a Vector Database?

Because the hard problem here is not retrieval. It is authority.

A vector store can find related notes. It does not decide which note wins, stop two agents from
editing the same live state, force a model to read the trap it keeps repeating, or prove which
model made a commit.

mdr-kernel does include a database where one helps: the node map is a generated JSON index over
explicit Markdown envelopes and node links. It is inspectable, regenerable, and committed with the
repo. It improves retrieval without moving authority out of git.

Git already gives us history, diff, authorship, branches, hooks, and a shared artifact every tool
can read.

mdr-kernel uses git as the coordination substrate and keeps the rules small enough that a model
can actually obey them.

## [ COMPLIANCE ] Known Limits

Honest edges, disclosed for the record. A refinement floor that hides its limits is not one you
should trust:

- The lock is **cooperative and machine-local** — a heartbeat file, not a distributed lease. It
  stops two *cooperating* sessions from co-writing state on the same clone; it is not a defense
  against a malicious actor. `STATE.lock` is git-ignored, so it never syncs between machines: two
  agents on two clones (or one on your laptop and one on a server) coordinate only through git
  push/pull, not through a shared live lock. Run sessions sequentially against one clone when
  they might touch the same coordination files; a second clone racing a push is caught by git's
  own conflict/rebase handling, not by `STATE.lock`.
- **Hooks are clone-local and bypassable by design in Git.** A fresh clone has neither the local
  `core.hooksPath` setting nor the approved-destination pin until setup runs in that clone. Git
  also permits an operator to bypass hooks. That is why setup, doctor, close-session, and
  sync-private verify privacy independently; keep those checks even if the hook appears redundant.
- **Setup does not auto-upgrade an initialized memory.** Bootstrap only scaffolds files that are
  missing; it never overwrites `bin/*`, `protocol/CONTRACT.md`, or any file your memory already
  has. If the public template improves after your private repo was created, you get those
  improvements only by deliberately merging them in yourself (e.g. add the template as a second
  git remote and cherry-pick or merge) — there is no update command.
- It is a **pattern, not a product.** No daemon, no service, no telemetry — just scripts, git, and
  a contract. You adapt it to your stack rather than installing it.

## [ VERSION ] Release and Upgrade Policy

The current version is in [`VERSION`](VERSION) (semantic versioning: `0.x` means the pattern can
still change shape between releases; `1.0.0` is the first release considered stable). Each release
is a tagged commit on this repository's `main`.

Your private memory is a **snapshot, not a subscription** — see "Setup does not auto-upgrade" in
Known Limits above. There is no update command and no compatibility promise across versions yet
(pre-`1.0.0`). To pull in template improvements after your own repo was created, add this
repository as a second git remote in your private clone and merge or cherry-pick deliberately;
review the diff first, since your memory's `bin/*` and `protocol/*` may have your own local
adaptations.

## What This Is Not

- Not a prompt pack.
- Not an agent framework.
- Not a vector-memory product.
- Not a dump of a private workspace.
- Not affiliated with the series that inspired the metaphor.

It is a portable pattern for running multiple AI workers against one shared engineering memory.

## License

MIT licensed (see [LICENSE](LICENSE)). Issues and adaptations welcome — see
[CONTRIBUTING.md](CONTRIBUTING.md) — but this is a working studio artifact shared as-is, with no
support commitment. The "Studio Do Not Disturb" name and marks are not covered by the MIT grant.

---

*Not affiliated with, endorsed by, or derived from any work of Apple Inc. or its subsidiaries.
"Severance" the series is theirs; the engineering here is mine. This is an homage in tone only:
no marks, no logos, no art.*

---

## SYSTEM NODE LINKS

Directory Path: ./
Next Sequential File: FLOOR-GUIDE.md
Related Workflows: protocol/CONTRACT.md, protocol/RUNBOOK.md, docs/adoption-guide.md, skills/mdr-kernel/SKILL.md, docs/brand-manual.md
AI Execution Prompts: protocol/CONTRACT.md
