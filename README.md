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
- **Focused Reading:** STATE and the reference index select a small reading slice; generated
  maps check link health.
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

See [Document lifecycle](docs/document-lifecycle.md) for finding stale records and archiving adopted documents.

## [ MANIFEST ] What You Get

```
mdr-kernel/
|- README.md                  public orientation
|- CHANGELOG.md               changes by version
|- VERSION                    distribution source version
|- native/                    Rust command for local history search
|- protocol/CONTRACT.md       the model-facing contract, generated into front doors
|- protocol/RUNBOOK.md        human startup and closeout workflow
|- protocol/coordination-files.txt  the files that need STATE.lock (pre-commit enforces it)
|- protocol/recall-gates.example.json  example trap gates; setup seeds your own gate file
|- mdr-kernel setup-private      the one adopter entry: verify private destination -> scaffold -> push
|- mdr-kernel bootstrap-clone    internal scaffold/repair primitive (setup-private calls it)
|- mdr-kernel start-session      canonical startup: doctor -> safe pull -> STATE readback
|- mdr-kernel close-session      closeout: verify, commit the close set, push, release the lock
|- mdr-kernel sync-private       the one push path (setup, close, offline recovery)
|- mdr-kernel doctor       health check for stale state, links, locks, hooks, secrets
|- mdr-kernel state-lock         heartbeat lock for coordination files
|- mdr-kernel brain-recall       active-recall gate authority for recurring mistakes
|- mdr-kernel hook-recall        prompt-hook adapter around brain-recall (configured != fired)
|- mdr-kernel prepare-work       portable manual recall fallback
|- mdr-kernel add-decision       append a proposed or quoted operator-ruled decision
|- mdr-kernel check-decision-history  append-only history and authority check (pre-commit runs it)
|- mdr-kernel build-rulings      compile protocol/rulings.json for recall; never hand-edit it
|- mdr-kernel ratify-decision    human-only, hash-bound ratification
|- mdr-kernel initialize-decision-authority  one-time authority boundary (-PreserveLegacy for old logs)
|- .githooks/pre-commit         runs affected Cargo tests against an isolated index snapshot
|- mdr-kernel update-node-map    generated link-health map and node-link checker
|- mdr-kernel sync-doors         regenerate the front doors from the contract
|- mdr-kernel write-approval     approval gate for new reference notes and skills
|- mdr-kernel waffle-party       optional themed wrapper around brain-doctor
|- CLAUDE.md / AGENTS.md / GEMINI.md  phase-zero agent doors; setup regenerates them from the contract
|- maps/node-roots.json       registry of roots to scan
|- maps/NODE-MAP.md           generated human link-health dashboard
|- maps/node-map.json         generated machine-readable link inventory
|- .githooks/                 pre-commit, commit-message and pre-push enforcement
|- native/tests/                regression suites; the doctor workflow runs the offline ones on Windows and Linux
|- specs/node-map.md          node-map subsystem contract
|- docs/adoption-guide.md     first-hour path for messy existing work
|- docs/upgrading.md          merge a new version into an existing private memory
|- docs/decision-authority.md proposals, operator rulings, ratification, legacy migration
|- docs/recall-hooks.md       wiring prompt recall per host, and how to prove it fired
|- docs/node-map-usage-guide.md link-health workflow and what selects the reading slice
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
3. `mdr-kernel start-session` is the only startup path. It runs `mdr-kernel doctor`, refuses unsafe pulls
   from a dirty tree, and prints the full current `STATE.md`.
4. `STATE.md` and `reference/INDEX.md` choose the smallest relevant file set before broad search.
   Generated node maps check link integrity.
5. `mdr-kernel hook-recall` or the portable `mdr-kernel prepare-work` fallback runs before assigned work. When an objective matches a known trap, the
   agent must read the evidence before acting.
6. `STATE.lock` protects coordination files. The pre-commit hook blocks protected edits unless
   the session holds the lock.
7. Handoffs record work. Decisions distinguish proposals from quoted operator rulings; human
   ratification binds exact entry bytes. Compiled rulings reach the next matching recall gate.

Deliberately boring infrastructure: the point is not to make agents *sound* coordinated, but to
make uncoordinated work difficult to commit.

## Find a past decision or handoff

After setting up your private memory, run from its root:

```powershell
mdr-kernel find --query "D7" --limit 3
```

The command returns file and line references from `STATE.md`, decision logs, and handoffs.
Use `--json` for structured output. The search is optional. It requires Rust and a C compiler
for bundled SQLite when building from source. The installed binary provides search and the session lifecycle.

The index contains excerpts from your memory. It stays in your user cache outside the repository:
`%LOCALAPPDATA%\mdr-kernel\history` on Windows and `~/.cache/mdr-kernel/history` on Linux
(`$XDG_CACHE_HOME/mdr-kernel/history` when that is set). Keep that cache private. Source files
stay in your private memory checkout.

The cache rebuilds itself when a source changes or the cache file is damaged. To clear it, delete
that folder; the next search rebuilds it. A handoff that is not UTF-8 is left out with a warning
that names the file; re-save it as UTF-8 to make it searchable.

### Native commit guards (optional in this release)

The same binary can run the git hooks. Install it into your memory's `bin/` folder:

```powershell
cargo run --locked --release --manifest-path native/Cargo.toml -- install
```

With `bin/mdr-kernel` installed, the hooks run the native engine. It applies the same pre-commit
gates as the shell hooks, plus a `commit-msg` check for the `Least-code:` and `Least-file:`
trailers that the contract asks for. Without the binary, the shell hooks run as before and the
trailers are not checked. If you update the kernel's `native/` sources, the engine refuses to
guard commits until you rebuild and reinstall it. To use the shell hooks for one commit, set
`MDR_ENGINE=legacy`.

For version changes and instructions for existing memories, see [CHANGELOG.md](CHANGELOG.md)
and the [upgrade guide](docs/upgrading.md).

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
mdr-kernel setup-private -Model 'alex'
```

`-Model` is the attribution name for setup's own lock and scaffold commit — type it without
angle brackets. Setup does not store it; later sessions name themselves with `-Model` or
`BRAIN_MODEL`. Setting up by hand, no agent driving? Pick any short name yourself (your own
works fine). An AI agent running setup passes its real model id instead, and every agent that
works here later identifies itself the same way.

Setup verifies that GitHub
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
mdr-kernel start-session -Model '<your-model-id>'
```

It should end with `STARTUP_STATUS=READY`. (An agent passes its real model id; a human uses
the short name they chose at setup.)

Every commit in a set-up clone needs an explicit identity. Committing by hand: set
`$env:BRAIN_MODEL = '<name>'` or end the message with `Brain-Model: <name>`. A commit with
neither, or with `unknown`, is refused. `git config brain.model` is display-only, and setup does
not write it:

```powershell
$env:BRAIN_MODEL = 'alex'
```

If you must stop while offline, add `-OfflineCheckpoint` to the normal close-session command.
This exception works only after private-remote setup: it commits locally, never tries to push,
and makes every later startup print a loud pending-sync warning. When connectivity returns, run:

```powershell
mdr-kernel sync-private
```

That command is the one push implementation used by setup, normal close, and recovery. It
re-verifies the destination's live privacy, rebases safely, pushes explicit `origin/main`, and
allows only one bounded retry if the remote moves again. The stored destination pin is never
treated as permission to push. Local-only users close normally and do not use either offline flag
or sync-private.

For deliberately local-only memory, use `mdr-kernel setup-private ... -LocalOnly`; setup removes
`origin` before memory is created. Non-GitHub private hosts require `-ConfirmPrivateRemote`
because their visibility cannot be checked generically.

Do not dump existing notes, repos, and half-sorted project folders into this repo wholesale. After
quick start, follow the [Adoption Guide](docs/adoption-guide.md): inventory first, then file each
item as live state, reference note, spec, handoff, inbox note, or skill. The shipped
[mdr-kernel skill](skills/mdr-kernel/SKILL.md) is the repeatable procedure an agent runs for that
intake.

For a compliance reading of a chaotic environment, run the optional diagnostic wrapper:

```powershell
mdr-kernel waffle-party
```

It is `mdr-kernel doctor` with a themed success footer. If the floor is not in compliance, it returns
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
  improvements only by applying a release deliberately — there is no update command, and pulling
  from your private origin never fetches one. [`docs/upgrading.md`](docs/upgrading.md) is the
  exact, rehearsed walkthrough.
- It is a **pattern, not a product.** No daemon, no service, no telemetry — just scripts, git, and
  a contract. You adapt it to your stack rather than installing it.

## [ VERSION ] Release and Upgrade Policy

The current version is in [`VERSION`](VERSION). Releases use semantic versioning. A version
with an `-rc` suffix is a prerelease for testing. Interfaces and upgrade details may change
before the final release. Stable releases are tagged on `main`.

Your private memory is a snapshot. To apply a release, follow [`docs/upgrading.md`](docs/upgrading.md).
The guide fetches the release read-only, copies distribution changes, and merges your edits to
files such as `protocol/CONTRACT.md`. It preserves your STATE, decisions, handoffs, gates, and
remote. GitHub's **Watch > Custom > Releases** sends release notifications.

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
