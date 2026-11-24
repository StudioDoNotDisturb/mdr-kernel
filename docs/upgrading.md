Title: Upgrade the kernel while preserving your memory
Type: Operator Guide / Upgrade
Context: Use when upgrading a private 0.1.0 or 0.2.0 memory while preserving its history, settings, and private remote.

# Upgrade an existing memory

The public repository distributes tools and examples. Your private repository holds your work.
An upgrade copies the release's tools into your memory and merges the few files you customized,
in one reviewed commit. It is not another first-time setup, not a history merge, and never a
replacement of STATE, decisions, handoffs, references, local gates or your remote.

## Choose an upgrade

- From `0.1.0`, apply `0.2.0` first using [The 0.1.0 to 0.2.0 procedure](#the-procedure), including
  the decision-authority migration in U4 and U8, then apply `2.0.0`.
- From `0.2.0`, follow [Upgrading 0.2.0 to 2.0.0](#upgrading-020-to-200) below. Keep your existing
  decision authority, ratifications, and recall gates. Review the `BRAIN-CONTRACT v2` changes with
  your local rules, install `bin/mdr-kernel`, and regenerate the doors and maps.

## Hearing about a release is separate from applying it

- **Notification.** Each release is a tagged commit with a GitHub Release. On the public
  repository choose **Watch > Custom > Releases** to be notified; a newsletter, if you read one,
  is only an announcement. Neither one changes your files.
- **There is no updater.** No script or subcommand fetches releases. Re-running `setup-private` or
  `bootstrap-clone` does not upgrade: they only create files that are missing.
- **Your private origin does not carry releases.** `git pull` from your private repository only
  synchronizes your own memory. The release must be fetched from the public repository, read-only,
  as shown below.

## Which shape is your memory?

- **(A) Template copy - the common case.** GitHub's "Use this template" creates a new repository
  whose first commit has no shared ancestry with the public repository. Never run
  `git merge --allow-unrelated-histories` against the release, and never replace your tree with a
  fresh export: either would mix release history into your memory or overwrite it.
- **(B) Retained-history clone - rare.** A private mirror that kept the public commits. It shares
  ancestry, but its doors, maps and decision log still diverge from the release, so it uses the
  same path-by-path procedure.

| Your `VERSION` | Public commit | Tag |
|---|---|---|
| `0.1.0` | `8397591e8cd42ec25a09a311df160b74c0af2afb` | none (first export) |
| `0.2.0` | `82b6b6a8efbf89242c53ee516d65a4b6fe9c36a3` | `v0.2.0` |
| `2.0.0` | Verify the commit ID against the published `v2.0.0` release | `v2.0.0` |

## Upgrading 0.2.0 to 2.0.0

### What changes in 2.0.0

- **One binary replaces the PowerShell scripts.** `bin/mdr-kernel` (`bin/mdr-kernel.exe` on
  Windows) implements the entire kernel lifecycle (`setup-private`, `bootstrap-clone`,
  `start-session`, `session-stop`, `close-session`, `sync-private`, `state-lock`, `remote-privacy`,
  `doctor`, `state-budget`, `secret-audit`, `sync-doors`, `brain-recall`, `hook-recall`,
  `prepare-work`, `add-decision`, `check-decision-history`, `build-rulings`, `ratify-decision`,
  `initialize-decision-authority`, `write-approval`, `update-node-map`, `envelope`, `find`,
  `find-stale`, `archive-adopted`, and `mcp`). PowerShell 7 is no longer required, and the retired
  `bin/*.ps1` scripts, POSIX wrapper twins, and PowerShell test suites are removed.
- **Git hooks call the binary.** `.githooks/pre-commit`, `.githooks/prepare-commit-msg`,
  `.githooks/commit-msg`, and `.githooks/pre-push` invoke `bin/mdr-kernel` directly. `bin/mdr-kernel`
  is gitignored and carries a build-time source fingerprint of `native/`; hooks refuse if the binary
  does not match the staged `native/` sources.
- **New 2.0 behavior in the contract and tools:** `protocol/CONTRACT.md` uses the compact
  `BRAIN-CONTRACT v2` block and names `Least-code:` / `Least-file:` trailers (enforced by
  `.githooks/commit-msg`); `hook-recall` stores per-session delivery receipts with `SessionStart`
  compaction re-arming; `mdr-kernel mcp` serves read-only recall and search tools over stdio
  without Node or Bun; `find-stale` and `archive-adopted` manage document retirement.
- **Cargo requirement during the upgrade commit:** Even if you install the prebuilt release
  binary, upgrading from `0.2.0` to `2.0.0` commits new files under `native/` and `.githooks/`.
  The `2.0.0` pre-commit hook runs Cargo regression suites against staged `native/` and hook
  changes, so Rust, `cargo`, and a C compiler (for bundled SQLite) are required on the machine
  performing the upgrade commit. After the upgrade lands, daily sessions using a prebuilt binary
  do not invoke Cargo unless you change `native/` or `.githooks/`.
- **Preserved untouched:** `STATE.md`, `DECISIONS.md`, `handoffs/`, `inbox/`, your `reference/`
  notes and `reference/INDEX.md`, `protocol/recall-gates.json`,
  `protocol/decision-authority.json`, `protocol/decision-ratifications.jsonl`, local git config
  (`core.hooksPath`, `brain.mode`, `brain.approvedRemote`), and your private remote.

### Rehearse in a throwaway clone first

Before modifying your live checkout, test the full upgrade and a local session close in an
isolated, remote-free copy:

```sh
git clone . /tmp/mdr-upgrade-test
cd /tmp/mdr-upgrade-test
git remote remove origin
# Run Steps 1-8 below in /tmp/mdr-upgrade-test, then run a clean local close check:
./bin/mdr-kernel doctor
./bin/mdr-kernel start-session -Model '<your-model-id>'
./bin/mdr-kernel close-session -NoHandoff -Model '<your-model-id>'
```

On Windows PowerShell, clone to `"$env:TEMP\mdr-upgrade-test"`, remove `origin`, run Steps 1–8,
and run `.\bin\mdr-kernel.exe doctor`, `.\bin\mdr-kernel.exe start-session -Model '<id>'`, and
`.\bin\mdr-kernel.exe close-session -NoHandoff -Model '<id>'`.

- **What `close-session -NoHandoff` verifies:** lock acquisition and release, STATE budget checks,
  and hard doctor checks on a clean tree (without needing a dummy handoff or `handoffs/INDEX.md`
  row). When nothing is modified, it skips node-map regeneration and passes without creating a
  commit; standalone `./bin/mdr-kernel doctor` checks node-map parity.
- **Exercising the commit hooks separately:** Step 7's upgrade commit in the throwaway clone
  exercises `.githooks/pre-commit`, `.githooks/prepare-commit-msg`, and `.githooks/commit-msg`.
  To also exercise a normal session commit through the hooks before deleting the throwaway clone,
  commit a throwaway non-coordination note (such as `inbox/upgrade-smoke.md` with `BRAIN_MODEL`
  set) or create `handoffs/<YYYY-MM-DD>-upgrade-smoke.md` plus its `handoffs/INDEX.md` row and
  run `./bin/mdr-kernel close-session -Topic upgrade-smoke -Model '<your-model-id>'`.
- **Pre-push limit:** a remote-free clone (`git remote remove origin`) cannot verify
  `.githooks/pre-push` or `sync-private` through a normal close; those run when you sync to your
  private remote in Step 8 of your live checkout.

Delete the throwaway clone when finished, then run the same upgrade in your live memory checkout.

### Exact steps (0.2.0 to 2.0.0)

Run every step from the root of your private memory in one shell session so `BRAIN_MODEL` and
`BRAIN_LOCK_TOKEN` stay set across commands. Close every other active session first.

#### 1. Seal identity, verify a clean tree, and create a rollback branch

Linux (`bash`):

```sh
export BRAIN_MODEL='<your-model-id>' &&
  test -z "$(git status --porcelain)" &&
  test ! -e STATE.lock &&
  test "$(tr -d '\r\n' < VERSION)" = "0.2.0" &&
  git diff --quiet HEAD -- DECISIONS.md &&
  git branch mdr-before-2.0.0
```

Windows (PowerShell):

```powershell
$model = '<your-model-id>'
$env:BRAIN_MODEL = $model
if (git status --porcelain) { throw 'STOP: commit or close current work first; the tree must be clean' }
if (Test-Path -LiteralPath STATE.lock) { throw 'STOP: STATE.lock is held' }
if ((Get-Content -Raw VERSION).Trim() -ne '0.2.0') { throw 'STOP: expected VERSION 0.2.0' }
git diff --quiet HEAD -- DECISIONS.md
if ($LASTEXITCODE -ne 0) { throw 'STOP: DECISIONS.md must be committed unchanged' }
git branch mdr-before-2.0.0
```

#### 2. Fetch `v2.0.0` read-only and pin `v0.2.0`

Verify the fetched `v2.0.0` commit ID against the published release before continuing. Any fetch
error or commit mismatch stops the block before release files are applied:

Linux (`bash`):

```sh
mdr='https://github.com/StudioDoNotDisturb/mdr-kernel.git'
expected='<full-40-char-commit-id-from-v2.0.0-release>'
printf '%s\n' "$expected" | grep -Eq '^[0-9a-fA-F]{40}$' || { echo 'STOP: set expected to the full 40-char release commit ID' >&2; false; } &&
  git fetch --no-tags "$mdr" refs/tags/v2.0.0:refs/mdr/v2.0.0 &&
  actual=$(git rev-parse 'refs/mdr/v2.0.0^{commit}') &&
  { test "$actual" = "$expected" || { git update-ref -d refs/mdr/v2.0.0; echo "STOP: release tag commit $actual does not match expected $expected" >&2; false; }; } &&
  git update-ref refs/mdr/v0.2.0 82b6b6a8efbf89242c53ee516d65a4b6fe9c36a3 &&
  git diff --stat refs/mdr/v0.2.0 HEAD -- bin .githooks tests docs protocol/CONTRACT.md protocol/coordination-files.txt
```

Windows (PowerShell):

```powershell
$mdr = 'https://github.com/StudioDoNotDisturb/mdr-kernel.git'
$expected = '<full-40-char-commit-id-from-v2.0.0-release>'
if ($expected -notmatch '^[a-fA-F0-9]{40}$') { throw 'STOP: set $expected to the full 40-char release commit ID' }
git fetch --no-tags $mdr 'refs/tags/v2.0.0:refs/mdr/v2.0.0'
if ($LASTEXITCODE -ne 0) { throw 'STOP: failed to fetch v2.0.0' }
$actual = git rev-parse 'refs/mdr/v2.0.0^{commit}'
if ($LASTEXITCODE -ne 0 -or $actual -ne $expected) {
  git update-ref -d refs/mdr/v2.0.0 2>$null
  throw "STOP: release tag commit '$actual' does not match expected '$expected'"
}
git update-ref refs/mdr/v0.2.0 82b6b6a8efbf89242c53ee516d65a4b6fe9c36a3
if ($LASTEXITCODE -ne 0) { throw 'STOP: failed to pin refs/mdr/v0.2.0' }
git diff --stat refs/mdr/v0.2.0 HEAD -- bin .githooks tests docs protocol/CONTRACT.md protocol/coordination-files.txt
```

#### 3. Acquire `STATE.lock` with the 0.2.0 script before changing files

Capture the printed `session token:` value into `BRAIN_LOCK_TOKEN` (keep it in the same shell):

Windows (PowerShell):

```powershell
$acq = pwsh -NoProfile -File bin/state-lock.ps1 -Acquire -Model $model -Objective 'upgrade mdr-kernel 0.2.0 to 2.0.0'
if ($LASTEXITCODE -ne 0) { throw 'STOP: acquire lock failed' }
$env:BRAIN_LOCK_TOKEN = ([regex]::Match(($acq -join "`n"), '(?m)^session token: ([a-f0-9]+)')).Groups[1].Value
if (-not $env:BRAIN_LOCK_TOKEN) { throw 'STOP: acquire did not return a session token' }
```

Linux (`bash`):

```sh
acq=$(pwsh -NoProfile -File bin/state-lock.ps1 -Acquire -Model "$BRAIN_MODEL" -Objective 'upgrade mdr-kernel 0.2.0 to 2.0.0')
printf '%s\n' "$acq"
export BRAIN_LOCK_TOKEN=$(printf '%s\n' "$acq" | sed -n 's/^session token: \([a-f0-9]\+\)$/\1/p')
test -n "$BRAIN_LOCK_TOKEN"
```

#### 4. Apply distribution changes, three-way merge edits, and remove retired scripts

Review the diff between `refs/mdr/v0.2.0` and `refs/mdr/v2.0.0` first: confirm no protected
memory path (`STATE.md`, `DECISIONS.md`, `.brain-root`, `handoffs/`, `inbox/`, `.claude/`,
`.codex/`, `reference/INDEX.md`, `protocol/recall-gates.json`, `protocol/decision-authority.json`,
`protocol/decision-ratifications.jsonl`) is modified, and inspect deleted paths (the retired
PowerShell scripts and wrappers):

```sh
git diff --name-only refs/mdr/v0.2.0 refs/mdr/v2.0.0
git diff --name-only --diff-filter=D refs/mdr/v0.2.0 refs/mdr/v2.0.0
```

Unpack both release trees into a temporary folder (`git archive`), three-way merge any
distribution files you customized, take unedited files directly from `refs/mdr/v2.0.0`, and remove
the retired files deleted between `v0.2.0` and `v2.0.0` (skipping `CLAUDE.md`, `AGENTS.md`,
`GEMINI.md`, `protocol/advisory.md`, `protocol/rulings.json`, and `maps/*`, which Step 6
regenerates). This block runs in your existing 0.2.0 PowerShell 7 shell on Windows or Linux:

```powershell
# PowerShell (Windows or Linux)
$work = Join-Path ([IO.Path]::GetTempPath()) ('mdr-upgrade-' + [guid]::NewGuid().ToString('N'))
foreach ($v in 'v0.2.0', 'v2.0.0') {
  New-Item -ItemType Directory -Path (Join-Path $work $v) | Out-Null
  git archive --format=tar -o (Join-Path $work "$v.tar") "refs/mdr/$v"
  tar -xf (Join-Path $work "$v.tar") -C (Join-Path $work $v)
}
$regenerated = 'CLAUDE.md', 'AGENTS.md', 'GEMINI.md', 'protocol/advisory.md', 'protocol/rulings.json'
$protected = '^(STATE[.]md$|DECISIONS[.]md$|[.]brain-root$|handoffs/|inbox/|[.]claude/|[.]codex/|reference/INDEX[.]md$|protocol/(recall-gates[.]json|decision-authority[.]json|decision-ratifications[.]jsonl)$)'
$all = @(git diff --name-only refs/mdr/v0.2.0 refs/mdr/v2.0.0)
if ($all -match $protected) { throw 'STOP: release touches a protected memory path' }
$deleted = @(git diff --name-only --diff-filter=D refs/mdr/v0.2.0 refs/mdr/v2.0.0 | Where-Object { $_ -notin $regenerated -and $_ -notlike 'maps/*' })
$changed = @(git diff --name-only --diff-filter=ACMR refs/mdr/v0.2.0 refs/mdr/v2.0.0 | Where-Object { $_ -notin $regenerated -and $_ -notlike 'maps/*' })
$empty = Join-Path $work 'empty'; New-Item -ItemType File -Path $empty -Force | Out-Null
$merged = @(); $conflicts = @()
foreach ($p in $changed) {
  $mine = git rev-parse -q --verify "HEAD:$p" 2>$null
  $base = git rev-parse -q --verify "refs/mdr/v0.2.0:$p" 2>$null
  if ($mine -eq $base) { git checkout refs/mdr/v2.0.0 -- $p; continue }
  $baseFile = if ($base) { Join-Path $work "v0.2.0/$p" } else { $empty }
  git merge-file --diff3 -L yours -L v0.2.0 -L v2.0.0 $p $baseFile (Join-Path $work "v2.0.0/$p")
  if ($LASTEXITCODE -lt 0 -or $LASTEXITCODE -gt 127) { throw "STOP: merge-file failed on $p" }
  $merged += $p; if ($LASTEXITCODE -gt 0) { $conflicts += $p }
}
foreach ($d in $deleted) { git rm -f --ignore-unmatch -- $d }
"Conflicts to resolve: $(if ($conflicts) { $conflicts -join ', ' } else { 'none' })"
```

Resolve any conflict markers in `$merged`: in `protocol/CONTRACT.md`, adopt `BRAIN-CONTRACT v2`
and re-apply any local rules; in `protocol/coordination-files.txt`, keep your custom entries and
keep the 2.0 entries. Verify no conflict markers remain before proceeding.

#### 5. Install `bin/mdr-kernel` (prebuilt release binary or source build)

**Prebuilt release binary:** download the `v2.0.0` platform binary and `SHA256SUMS.txt`, verify
the checksum, and place it at `bin/mdr-kernel` (`bin/mdr-kernel.exe` on Windows) in one chained
step so any checksum failure stops before installing:

- Windows (PowerShell):

  ```powershell
  $expected = ((Select-String -Path SHA256SUMS.txt -Pattern 'mdr-kernel-windows-x86_64\.exe$').Line -split '\s+')[0]
  $actual = (Get-FileHash -Path mdr-kernel-windows-x86_64.exe -Algorithm SHA256).Hash.ToLowerInvariant()
  if (-not $expected -or $actual -ne $expected) { throw "Checksum mismatch: expected $expected, got $actual" }
  New-Item -ItemType Directory -Force -Path bin | Out-Null
  Move-Item -Force mdr-kernel-windows-x86_64.exe bin/mdr-kernel.exe
  ```

- Linux (`bash`):

  ```sh
  grep -E '^[0-9a-f]{64}[[:space:]]+\*?mdr-kernel-linux-x86_64$' SHA256SUMS.txt | sha256sum -c - &&
    mkdir -p bin &&
    mv mdr-kernel-linux-x86_64 bin/mdr-kernel &&
    chmod +x bin/mdr-kernel
  ```

**Or build from source:**

```sh
cargo build --locked --release --manifest-path native/Cargo.toml &&
  ./native/target/release/mdr-kernel install   # .\native\target\release\mdr-kernel.exe install on Windows
```

#### 6. Validate gates and regenerate rulings, doors, and node map with the new binary

Run these with `./bin/mdr-kernel` (or `.\bin\mdr-kernel.exe` on Windows). Regenerate
`protocol/rulings.json` before running `-Check`, because a `0.2.0` rulings file can report
`STALE` against the `2.0.0` compiler:

```sh
./bin/mdr-kernel state-lock -CheckOwner -Model "$BRAIN_MODEL"
./bin/mdr-kernel check-decision-history
./bin/mdr-kernel build-rulings
./bin/mdr-kernel build-rulings -Check
./bin/mdr-kernel brain-recall -CheckGates
./bin/mdr-kernel sync-doors
./bin/mdr-kernel update-node-map
```

#### 7. Run tests, stage, and commit the upgrade

Cargo is required here even with a prebuilt binary, because pre-commit runs Cargo regression
suites when `native/` and `.githooks/` are staged. Run the suite first to verify your toolchain:

```sh
cargo test --locked --manifest-path native/Cargo.toml
```

Stage every updated distribution file from Step 4 (taken directly, merged cleanly, or resolved by
hand) and the regenerated doors and maps (`git rm` already staged the retired scripts). Each block
below stops if any tracked file is still unstaged, because a commit without a merged file would
leave the upgrade half applied. Then review `git status` and `git diff --cached --stat`, and
confirm `bin/mdr-kernel*` is ignored and unstaged. Because the
upgrade adds `native/` code and new distribution files, `.githooks/commit-msg` requires **both**
`Least-code:` and `Least-file:` trailers (in one contiguous trailer block at the end of the
message, alongside `Brain-Model:` from `BRAIN_MODEL`):

Windows (PowerShell):

```powershell
git add -- $changed CLAUDE.md AGENTS.md GEMINI.md protocol/advisory.md protocol/rulings.json maps
git diff --quiet
if ($LASTEXITCODE -ne 0) { git diff --name-only; throw 'STOP: tracked files above are still unstaged' }
git status
git diff --cached --stat
git commit -m "Upgrade mdr-kernel 0.2.0 to 2.0.0" -m "Least-code: apply upstream 2.0.0 native Rust engine and Git hooks.`nLeast-file: apply 2.0.0 release files and remove retired PowerShell scripts."
```

Linux (`bash`):

Step 4's `$changed` list lives in PowerShell, so this block stages every tracked change instead.
That is exact because Step 1 proved the tree was clean before the upgrade began:

```sh
git add -u -- . &&
  git add -- CLAUDE.md AGENTS.md GEMINI.md protocol/advisory.md protocol/rulings.json maps &&
  { git diff --quiet || { git diff --name-only; echo 'STOP: tracked files above are still unstaged' >&2; false; }; }
git status
git diff --cached --stat
git commit -m "Upgrade mdr-kernel 0.2.0 to 2.0.0" -m $'Least-code: apply upstream 2.0.0 native Rust engine and Git hooks.\nLeast-file: apply 2.0.0 release files and remove retired PowerShell scripts.'
```

#### 8. Release lock, sync privately, and verify

```sh
./bin/mdr-kernel state-lock -Release -Model "$BRAIN_MODEL"
unset BRAIN_LOCK_TOKEN
# If brain.mode is private-remote:
./bin/mdr-kernel remote-privacy -RequireApproved
./bin/mdr-kernel sync-private
```

Verify the upgraded memory:

1. `./bin/mdr-kernel doctor` must report `doctor: ok.` and exit 0 (including node-map parity).
2. `./bin/mdr-kernel start-session -Model '<your-model-id>'` must end with `STARTUP_STATUS=READY`.
3. `./bin/mdr-kernel close-session -NoHandoff -Model '<your-model-id>'` confirms clean-tree lock
   acquisition/release, STATE budget, and hard doctor checks without regenerating maps or creating a
   commit (a remote-free throwaway clone cannot exercise `.githooks/pre-push`; `sync-private` above
   exercises it on a live `private-remote` memory, and your first session close with a handoff
   exercises all hooks end to end).

If your host config (`.claude/settings.json`, `.codex/hooks.json`, `.mcp.json`) still points at
`pwsh -NoProfile -File bin/hook-recall.ps1`, update it to invoke `bin/mdr-kernel hook-recall`
(or `bin/mdr-kernel mcp` for MCP hosts; see `docs/recall-hooks.md`).

### Rollback (2.0.0 to 0.2.0)

Roll back by reverting the upgrade commit (never `git reset` past later memory commits). Keep the
decision authority boundary, ratifications, and any memory commits made after the upgrade:

```sh
upgrade=$(git log -1 --format=%H --grep '^Upgrade mdr-kernel 0\.2\.0 to 2\.0\.0$')
test -n "$upgrade"
acq=$(./bin/mdr-kernel state-lock -Acquire -Model "$BRAIN_MODEL" -Objective 'roll back mdr-kernel 2.0.0 upgrade')
printf '%s\n' "$acq"
export BRAIN_LOCK_TOKEN=$(printf '%s\n' "$acq" | sed -n 's/^session token: \([a-f0-9]\+\)$/\1/p')
git revert --no-commit "$upgrade"
for keep in protocol/decision-authority.json protocol/rulings.json protocol/decision-ratifications.jsonl; do
  if git cat-file -e "HEAD:$keep" 2>/dev/null; then git checkout HEAD -- "$keep"; fi
done
rm -f bin/mdr-kernel bin/mdr-kernel.exe
pwsh -NoProfile -File bin/sync-doors.ps1
pwsh -NoProfile -File bin/update-node-map.ps1
git add -- CLAUDE.md AGENTS.md GEMINI.md protocol/advisory.md maps
git commit -m "Revert mdr-kernel 2.0.0 upgrade ($upgrade)"
pwsh -NoProfile -File bin/state-lock.ps1 -Release -Model "$BRAIN_MODEL"
unset BRAIN_LOCK_TOKEN
```

Removing the gitignored `bin/mdr-kernel` / `bin/mdr-kernel.exe` ensures the restored 0.2.0 hooks
run against the restored PowerShell scripts rather than an older 2.0 binary.

## The procedure (0.1.0 to 0.2.0)

Run every block in **one** PowerShell 7 window, from the root of your private memory, in order.
The commands are the same on Windows and Linux; `tar` ships with both (on Windows 10 1803 and
later it is `C:\Windows\System32\tar.exe`; if `Get-Command tar` names a different one, such as
a POSIX `tar` from another toolset, which can misread `C:` as a host name, replace `tar` in U3
with that full path). Each block stops with `STOP at ...` when a step fails; fix that cause, then continue from
the same block. Allow about 30 minutes; the suites and the final commit take most of it.

### U1 - Seal identity and checkpoint

Close every other session first (their normal close releases `STATE.lock`). Then:

```powershell
# [U1] identity, clean checkpoint, rollback point
$model = '<your model id>'
$env:BRAIN_MODEL = $model
function Must([string]$What) { if ($LASTEXITCODE -ne 0) { throw "STOP at '$What' (exit $LASTEXITCODE)" } }
if (git status --porcelain) { throw 'STOP: commit or close current work first; the tree must be clean' }
if (Test-Path -LiteralPath STATE.lock) { throw 'STOP: STATE.lock is held; let its session close, or recover it by your current rules' }
$installed = (Get-Content -Raw VERSION).Trim()
if ($installed -ne '0.1.0') { throw "STOP: use the 0.2.0 to 2.0.0 section above for VERSION $installed" }
$target = '0.2.0'
$fromRef = "refs/mdr/v$installed"
$toRef = "refs/mdr/v$target"
git diff --quiet HEAD -- DECISIONS.md; Must 'DECISIONS.md must be committed unchanged'
git branch "mdr-before-$target"; Must 'rollback branch'
```

### U2 - Fetch the release read-only

No remote is added; the release lands in two private refs under `refs/mdr/`, which your sync
never pushes.

```powershell
# [U2] fetch the tagged release and name the installed version
$mdr = 'https://github.com/StudioDoNotDisturb/mdr-kernel.git'
$baseCommit = '8397591e8cd42ec25a09a311df160b74c0af2afb'
$expected = '82b6b6a8efbf89242c53ee516d65a4b6fe9c36a3'
git fetch --no-tags $mdr "refs/tags/v${target}:${toRef}"; Must "fetch v$target"
if ((git rev-parse "${toRef}^{commit}") -ne $expected) { throw 'STOP: release tag does not match its published commit' }
git update-ref $fromRef $baseCommit; Must "name $installed"
$shared = @(git rev-list HEAD) -contains $baseCommit
"Shape: $(if ($shared) { 'B - retained history' } else { 'A - template copy, no shared history' })"
git diff --stat $fromRef HEAD -- bin .githooks tests docs protocol/CONTRACT.md protocol/coordination-files.txt
```

The last command lists the release files you customized. An empty list means you never edited
them.

### U3 - Unpack both releases for comparison

```powershell
# [U3] read-only copies of the installed and target releases
$work = Join-Path ([IO.Path]::GetTempPath()) ('mdr-upgrade-' + [guid]::NewGuid().ToString('N'))
foreach ($v in "v$installed", "v$target") {
  New-Item -ItemType Directory -Path (Join-Path $work $v) | Out-Null
  git archive --format=tar -o (Join-Path $work "$v.tar") "refs/mdr/$v"; Must "archive $v"
  tar -xf (Join-Path $work "$v.tar") -C (Join-Path $work $v); Must "unpack $v"
}
$work
```

### U4 - Check the decision log before any new hook arrives

If you have 0.2.0, skip U4 and U4b. U8 checks the existing authority boundary.

0.2's pre-commit validates `DECISIONS.md` on every commit. Run its checker from the unpacked
copy first, while your 0.1 hooks are still in charge:

```powershell
# [U4] preflight the decision log with the 0.2 checker (reads only)
$ids = @([regex]::Matches((Get-Content -Raw DECISIONS.md), '(?m)^## D(\d+)\b') | ForEach-Object { [int]$_.Groups[1].Value })
$next = if ($ids.Count) { ($ids | Measure-Object -Maximum).Maximum + 1 } else { 1 }
pwsh -NoProfile -File (Join-Path $work 'v0.2.0/bin/check-decision-history.ps1') -Path (Join-Path $PWD 'DECISIONS.md') -AuthorityFrom $next
$preflight = $LASTEXITCODE
Select-String -Path DECISIONS.md -Pattern '^\*\*(Authority|Operator said)'
```

- `DECISION_HISTORY_STATUS=OK` - go to U5.
- `DECISION_HISTORY_STATUS=FAIL` - 0.1 never prescribed an entry format, so free-form logs fail
  here. Do U4b, then run U4 again.
- The `Select-String` lines are old entries that already claim operator authority. An old entry
  with `**Authority:** operator-ruled` and an `**Operator said (...)**` line will compile as a
  ruling; check each one is genuine before migrating. Never add such lines to make anything pass.

### U4b - Only if U4 failed: repair structure, not meaning

Edit `DECISIONS.md` by hand. Give every entry a heading `## D<n> - YYYY-MM-DD - <title>`,
newest first and numbered down to `D1` without gaps, and mark exactly one `**Decision:**` line
in each entry. Change no wording, add no `**Authority:**` line or quotation, delete nothing.
Commit this repair on its own with the 0.1 tools that are still installed:

```powershell
# [U4b] commit the structural repair under your 0.1 lock
$acq = pwsh -NoProfile -File bin/state-lock.ps1 -Acquire -Model $model -Objective 'normalize decision log before 0.2'
$acq; Must 'acquire 0.1 lock'
$env:BRAIN_LOCK_TOKEN = ([regex]::Match(($acq -join "`n"), '(?m)^session token: ([a-f0-9]+)')).Groups[1].Value
git add -- DECISIONS.md
git commit -m 'Normalize decision log structure before the 0.2 upgrade (no wording change)'; Must 'commit repair'
pwsh -NoProfile -File bin/state-lock.ps1 -Release -Model $model; Must 'release 0.1 lock'
Remove-Item Env:\BRAIN_LOCK_TOKEN
```

This commit is the "unchanged old decision log" checkpoint: from here on the log is only
appended to. If U4 passed, your existing commit already is that checkpoint.

### U5 - Acquire your lock before editing release files

Run this with the installed tool and keep the token in this shell through U11.

```powershell
$acq = pwsh -NoProfile -File bin/state-lock.ps1 -Acquire -Model $model -Objective "upgrade mdr-kernel $installed to $target"
Must 'acquire lock'
$env:BRAIN_LOCK_TOKEN = ([regex]::Match(($acq -join "`n"), '(?m)^session token: ([a-f0-9]+)')).Groups[1].Value
if (-not $env:BRAIN_LOCK_TOKEN) { throw 'STOP: acquire did not return a session token' }
```

### U6 - Bring the release files, hooks and helpers together

New hooks are active as soon as they are on disk, and the 0.2 pre-commit refuses when any helper
it calls is missing. This block therefore brings every changed release file in one step.

```powershell
# [U6] take unedited release files, three-way merge edited ones
if ($installed -eq '0.1.0' -and $preflight -ne 0) { throw 'STOP: U4 must report DECISION_HISTORY_STATUS=OK first' }
$regenerated = 'CLAUDE.md', 'AGENTS.md', 'GEMINI.md', 'protocol/advisory.md', 'protocol/rulings.json'
$releasePaths = @(git diff --name-only $fromRef $toRef); Must 'list release paths'
$protected = '^(STATE[.]md$|DECISIONS[.]md$|[.]brain-root$|handoffs/|inbox/|[.]claude/|[.]codex/|reference/INDEX[.]md$|protocol/(recall-gates[.]json|decision-authority[.]json|decision-ratifications[.]jsonl)$)'
if ($releasePaths -match $protected) { throw 'STOP: release changes protected memory paths' }
$deleted = @(git diff --name-only --diff-filter=D $fromRef $toRef); Must 'check release deletions'
if ($deleted.Count) { throw 'STOP: review release deletions before proceeding' }
$changed = @($releasePaths | Where-Object { $_ -notin $regenerated -and $_ -notlike 'maps/*' })
$empty = Join-Path $work 'empty'; New-Item -ItemType File -Path $empty -Force | Out-Null
$merged = @(); $conflicts = @()
foreach ($p in $changed) {
  $mine = git rev-parse -q --verify "HEAD:$p" 2>$null
  $base = git rev-parse -q --verify "${fromRef}:$p" 2>$null
  if ($mine -eq $base) { git checkout $toRef -- $p; Must "take $p"; continue }
  $baseFile = if ($base) { Join-Path $work "v$installed/$p" } else { $empty }
  git merge-file --diff3 -L yours -L "v$installed" -L "v$target" $p $baseFile (Join-Path $work "v$target/$p")
  if ($LASTEXITCODE -lt 0 -or $LASTEXITCODE -gt 127) { throw "STOP: merge-file failed on $p" }
  $merged += $p; if ($LASTEXITCODE -gt 0) { $conflicts += $p }
}
"Taken from the release: $($changed.Count - $merged.Count); merged: $($merged -join ', ')"
"Conflicts to resolve by hand: $(if ($conflicts) { $conflicts -join ', ' } else { 'none' })"
```

Both upgrades in this guide preserve installed paths. The copy step refuses release deletions.

### U7 - Resolve and review your merges

Open every file named under "Conflicts". Each conflict shows your text after `<<<<<<< yours`,
your installed release after `||||||| v…`, and the release after `=======`. Keep the release text,
re-add your own lines, and delete the three markers. Then confirm and review:

```powershell
# [U7] no markers remain; each merged file differs from the release only by your adaptations
if ($merged -and (Select-String -Path $merged -Pattern '^(<<<<<<< yours|\|\|\|\|\|\|\| v|>>>>>>> v)')) { throw 'STOP: conflict markers remain' }
foreach ($p in $merged) { git diff --no-index -- (Join-Path $work "v$target/$p") $p }
```

In `protocol/coordination-files.txt` keep the paths you added and the release entries. In
`protocol/CONTRACT.md` keep your local rules in the contract block. U9 builds the doors from the
merged contract.

### U8 - Verify the lock and preserve decision authority

```powershell
# [U8] initialize only a 0.1 memory without an existing boundary
pwsh -NoProfile -File bin/state-lock.ps1 -CheckOwner -Model $model; Must 'prove lock ownership'
$authority = 'protocol/decision-authority.json'
if ($installed -eq '0.1.0' -and -not (Test-Path -LiteralPath $authority)) {
  pwsh -NoProfile -File bin/initialize-decision-authority.ps1 -PreserveLegacy; Must 'initialize authority'
} elseif (-not (Test-Path -LiteralPath $authority)) {
  throw 'STOP: the existing authority boundary is missing'
}
pwsh -NoProfile -File bin/build-rulings.ps1 -Check; Must 'rulings current'
```

On first migration from 0.1, expect `AUTHORITY_INITIALIZED from=D<newest+1> legacy=unclassified`. Old entries stay exactly as
written and are not rulings unless they already carried an operator quotation (U4). To adopt an
old idea as a rule, ask the operator to rule on it in a new entry. A memory that never recorded
a decision gets `from=D1`.

### U9 - Gates, doors and map

```powershell
# [U9] validate your own gates, regenerate doors and map from your merged sources
pwsh -NoProfile -File bin/brain-recall.ps1 -CheckGates; Must 'your recall gates'
pwsh -NoProfile -File bin/sync-doors.ps1; Must 'regenerate doors'
pwsh -NoProfile -File bin/update-node-map.ps1; Must 'regenerate node map'
```

### U10 - Test, inspect, commit

```powershell
# [U10] suites from .github/workflows/doctor.yml, then one reviewed commit, then doctor
$keepToken = $env:BRAIN_LOCK_TOKEN; Remove-Item Env:\BRAIN_LOCK_TOKEN, Env:\BRAIN_MODEL   # suites run as in CI
foreach ($suite in 'remote-privacy-unit', 'skill-structure', 'envelope-check', 'decision-authority', 'recall') {
  pwsh -NoProfile -File "tests/$suite.ps1"; Must "tests/$suite"
}
$pristine = Join-Path $work 'release-checkout'
git init -q -b unborn $pristine; Must 'pristine repository'
git -C $pristine fetch -q $PWD $toRef; Must 'pristine fetch'
git -C $pristine switch -q -c main FETCH_HEAD; Must 'pristine main'
foreach ($suite in 'private-onboarding', 'node-map-retrieval', 'session-safety') {
  pwsh -NoProfile -File (Join-Path $pristine "tests/$suite.ps1"); Must "release tests/$suite"
}
$env:BRAIN_LOCK_TOKEN = $keepToken; $env:BRAIN_MODEL = $model
git add -- $changed CLAUDE.md AGENTS.md GEMINI.md protocol/advisory.md maps protocol/decision-authority.json protocol/rulings.json; Must 'stage'
$left = @(git status --porcelain | Where-Object { $_ -notmatch '^[AMD] ' })
if ($left) { $left; throw 'STOP: unstaged or untracked paths remain; review them before committing' }
git diff --cached --stat
git commit -m "Upgrade mdr-kernel $installed to $target"; Must 'upgrade commit'
pwsh -NoProfile -File bin/brain-doctor.ps1; Must 'doctor'
```

Five suites build their own fixtures and run against your upgraded files. `private-onboarding`,
`node-map-retrieval` and `session-safety` copy or clone the whole repository and expect a
never-set-up template (phase-zero doors, a scaffold-only first commit), so they fail by design
inside any memory that already has STATE; they run in a throwaway checkout of the release under
`$work` that has no remote. The suites run without your identity and token, as in CI. The
commit takes several minutes: pre-commit reruns the affected suites against your staged
scripts. `BRAIN_MODEL` in this window adds your `Brain-Model:` trailer.

### U11 - Release, sync privately, start fresh

```powershell
# [U11] release the lock, push only to your approved private destination, require READY
pwsh -NoProfile -File bin/state-lock.ps1 -Release -Model $model; Must 'release lock'
Remove-Item Env:\BRAIN_LOCK_TOKEN
if ((git config --local --get brain.mode) -eq 'private-remote') {
  pwsh -NoProfile -File bin/remote-privacy.ps1 -RequireApproved; Must 'privacy verifier'
  pwsh -NoProfile -File bin/sync-private.ps1; Must 'private sync'
}
$upgradeTempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
$upgradeWorkPath = [IO.Path]::GetFullPath($work).TrimEnd([IO.Path]::DirectorySeparatorChar)
$upgradePathComparison = if ($IsWindows) { [StringComparison]::OrdinalIgnoreCase } else { [StringComparison]::Ordinal }
if (-not $upgradeWorkPath.StartsWith($upgradeTempRoot, $upgradePathComparison) -or
    [IO.Path]::GetFileName($upgradeWorkPath) -notmatch '^mdr-upgrade-[0-9a-f]{32}$') {
  throw 'STOP: upgrade cleanup path is outside the generated temporary workspace'
}
Remove-Item -LiteralPath $upgradeWorkPath -Recurse -Force
pwsh -NoProfile -File bin/start-session.ps1 -Model $model; Must 'start-session'
```

The last command must end with `STARTUP_STATUS=READY`. Keep the fetched release ref as the base for the next upgrade. `git config --local --unset brain.model` removes the 0.1 identity setting
that 0.2 no longer reads; doctor warns about it otherwise.

## Expected refusals and what they mean

| Message contains | Cause | Fix |
|---|---|---|
| `set BRAIN_MODEL or include an explicit Brain-Model trailer` | A commit identified only by `git config brain.model` | Set `$env:BRAIN_MODEL` in the committing window (U1). |
| `LOCK_OWNER=UNPROVEN` or `BRAIN_LOCK_TOKEN does not match` | A lock taken by another session, or a token that is not yours | Use the token from your own `-Acquire` (U5). Never copy one from the lock file. |
| `required gate 'bin/...' is missing` | Hooks arrived without their helpers | Finish U6; it brings both together. |
| `staged decision history/authority is invalid` | New hooks with an unmigrated decision log | Complete U8 before committing. |
| `Commit existing history unchanged before migration` | `DECISIONS.md` edited after its last commit | Commit or restore it (U4, U4b) first. |
| `Existing history is structurally invalid; migration refused` | A log U4 would have failed, for example free-form 0.1 headings | Go back to U4b. |
| `Authority boundary already exists; it is immutable` | U8 already ran | Keep the committed boundary; never edit or delete it. |
| `staged paths outside this close set` | `close-session.ps1` saw staged paths that belong to other work | Commit your own work separately, or name it with `-Also`. |
| `refusing to merge unrelated histories` | A merge of the release into a template copy | Do not merge; use U6. |

## Rollback

Roll back by reverting the upgrade commit. Never `git reset` past it: later memory commits
would be lost. Keep the authority boundary; it is durable history.

```powershell
# [R1] revert the upgrade, keep later memory and the authority boundary
$upgrade = git log -1 --format=%H --grep '^Upgrade mdr-kernel [0-9].* to [0-9].*$'
if (-not $upgrade) { throw 'STOP: no upgrade commit found' }
$acq = pwsh -NoProfile -File bin/state-lock.ps1 -Acquire -Model $model -Objective 'roll back the mdr-kernel upgrade'
Must 'acquire lock'
$env:BRAIN_LOCK_TOKEN = ([regex]::Match(($acq -join "`n"), '(?m)^session token: ([a-f0-9]+)')).Groups[1].Value
git revert --no-commit $upgrade
$keep = @('protocol/decision-authority.json', 'protocol/rulings.json', 'protocol/decision-ratifications.jsonl') |
  Where-Object { git cat-file -e "HEAD:$_" 2>$null; $LASTEXITCODE -eq 0 }
git checkout HEAD -- $keep; Must 'keep the authority boundary and its records'
pwsh -NoProfile -File bin/sync-doors.ps1; Must 'doors from the restored contract'
pwsh -NoProfile -File bin/update-node-map.ps1; Must 'node map'
git add -- CLAUDE.md AGENTS.md GEMINI.md protocol/advisory.md maps; Must 'stage regenerated files'
$open = @(git diff --name-only --diff-filter=U)
if ($open) { $open; throw 'STOP: resolve these by hand (keep your later memory), then git add them' }
git commit -m "Revert mdr-kernel upgrade ($upgrade)"; Must 'revert commit'
pwsh -NoProfile -File bin/state-lock.ps1 -Release -Model $model; Must 'release lock'
```

The revert restores the previous tools, contract, and doors. STATE, handoffs, decisions, the
authority boundary, compiled rulings, and ratification records remain. A later upgrade reuses the
existing boundary.

## Upgrade notes: 0.1 to 0.2

These 0.2 behaviours break habits that worked in 0.1. Each one is deliberate.

- **Every commit needs an explicit identity.** The commit-message hook refuses a commit that has
  neither `BRAIN_MODEL` in the committing shell nor a `Brain-Model:` trailer, and a trailer that
  disagrees with `BRAIN_MODEL`; `unknown` is refused as a name. It no longer falls back to
  `git config brain.model`, and setup no longer writes that setting. This includes a human's hand
  commits.
- **Tokenless locks no longer prove ownership.** Heartbeat, release, `-CheckOwner` and
  coordination commits need explicit identity plus the token printed by your own
  `bin/state-lock.ps1 -Acquire` (`BRAIN_LOCK_TOKEN` or `-Token`). A lock written by 0.1 without a
  token must be recovered explicitly, not adopted.
- **Forced release needs a recorded reason.** `-Release -Force` now requires
  `-AbandonmentReason '<independent evidence or operator direction>'`; a fresh heartbeat also
  needs `MDR_FORCE_LIVE_LOCK=1`. A stale heartbeat alone is not proof that the holder is gone.
- **Closeout keeps to its close set.** `bin/close-session.ps1` refuses when unrelated paths are
  staged and leaves them staged. It no longer adopts Markdown notes under `inbox/`; name the notes
  your session owns with `-Also`.
- **An existing decision log blocks commits until migrated.** Pre-commit checks decision history
  on every commit once `DECISIONS.md` exists. Commit the old log unchanged, then run
  `bin/initialize-decision-authority.ps1 -PreserveLegacy` under your own lock (see
  `docs/decision-authority.md`). Old entries are preserved, not ratified.
- **Pre-commit needs `pwsh` and every gate helper.** The hook refuses when `pwsh` or a required
  utility is missing, or when any of `bin/state-lock.ps1`, `bin/write-approval.ps1`,
  `bin/sync-doors.ps1`, `bin/envelope-check.ps1`, `bin/check-decision-history.ps1`,
  `bin/build-rulings.ps1`, `bin/test-staged.ps1` or `protocol/coordination-files.txt` is absent.
  Bring the helpers over in the same commit as the hooks.
- **The staged test runner must match its staged copy.** Pre-commit refuses when
  `bin/test-staged.ps1` in the working tree differs from the staged version; stage or restore it.
- **Some commits take minutes.** Pre-commit runs the affected suites against the staged scripts.
  A commit that changes a lifecycle script (`state-lock`, `close-session`, `bootstrap-clone`,
  `setup-private` or the commit hooks) runs `tests/session-safety.ps1`, about 2-3 minutes. Each
  run keeps its snapshot in a `mdr-staged-*` folder under the system temp directory for
  diagnosis; those folders can be deleted.
- **`protocol/coordination-files.txt` gained entries.** The decision tools, `bin/hook-recall.ps1`,
  `bin/prepare-work.ps1`, `bin/test-staged.ps1`, `protocol/decision-authority.json`,
  `protocol/decision-ratifications.jsonl` and `protocol/rulings.json` now need `STATE.lock`.
  Merge the new lines into your own list; keep the entries you added yourself.

After the upgrade, configure prompt recall only for supported host events and verify an actual
firing receipt as described in `docs/recall-hooks.md`; use `bin/prepare-work.ps1` until then.

## Optional project patterns

Long projects can keep their own packet with current scope, asset references, local traps and
review status. Promote a trap into global recall only when it is broadly useful; the generic
kernel does not ship a video production system. Likewise, a project's explicit phase can guard
unapproved work, but its policy belongs to that project. These are patterns to adopt deliberately,
not extra background services or inherited studio policies.

---

## SYSTEM NODE LINKS

Directory Path: docs/
Next Sequential File: decision-authority.md
Related Workflows: CHANGELOG.md, docs/decision-authority.md, docs/recall-hooks.md, protocol/RUNBOOK.md
AI Execution Prompts: protocol/CONTRACT.md
