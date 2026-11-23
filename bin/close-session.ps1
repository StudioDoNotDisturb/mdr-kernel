#requires -Version 7.0
<#
.SYNOPSIS
  Automated session close-out. Verifies the close-out invariants, stages, commits with the
  Brain-Model trailer, pushes, and releases STATE.lock only after the push succeeds.

  The narrative work is still yours — update STATE.md, write the handoff + INDEX row, append
  decisions — this script makes the mechanical tail (verify/commit/push/release) unmissable, so
  a session that dies late loses nothing silently.
.EXAMPLE
  pwsh -NoProfile -File bin/close-session.ps1 -Topic feature-audit -Model '<your-model-id>'
  pwsh -NoProfile -File bin/close-session.ps1 -Topic old-work -Model '<your-model-id>' -SessionDate 2026-06-14
  pwsh -NoProfile -File bin/close-session.ps1 -Topic audit-step1 -Model '<your-model-id>' -DryRun
  pwsh -NoProfile -File bin/close-session.ps1 -Topic audit-step1 -Model '<your-model-id>' -Message 'Session close: audit step 1'
  pwsh -NoProfile -File bin/close-session.ps1 -NoHandoff -Model '<your-model-id>'   # pure-integration session, no new handoff
  pwsh -NoProfile -File bin/close-session.ps1 -Topic offline-work -Model '<your-model-id>' -OfflineCheckpoint
  pwsh -NoProfile -File bin/close-session.ps1 -Topic my-work -Model '<your-model-id>' -Token '<token from -Acquire>'  # own lock, new shell
#>
[CmdletBinding()]
param(
  [string]$Topic,      # handoff slug: handoffs/<today>-<Topic>.md must exist (with an INDEX row)
  [string]$Model,      # committing identity; explicit (-Model or BRAIN_MODEL) — config is display-only
  [string]$Token,      # session lock token printed by state-lock -Acquire; shell-safe alternative to $env:BRAIN_LOCK_TOKEN
  [string]$Message,    # commit subject; default "Session close: <Topic or today>"
  [ValidatePattern('^\d{4}-\d{2}-\d{2}$')]
  [string]$SessionDate, # explicit handoff date when outside the accepted UTC-today/local-today/UTC±1 window
  [string[]]$Also = @(), # extra files/directories this session owns and wants in the close commit
  [string]$LeastCode,  # Least-code trailer text, when the close adds a bin/ file or >60 code lines
  [string]$LeastFile,  # Least-file trailer text, when the close adds files beyond one dated handoff and inbox/ notes
  [switch]$OfflineCheckpoint, # commit locally, mark pending sync, never push (private-remote mode only)
  [switch]$DryRun,     # run all checks, stage/commit/push nothing
  [switch]$NoHandoff   # explicit opt-out when this session wrote no new handoff
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
# The native engine runs this same close when bin/mdr-kernel is installed (README).
$engine = @('mdr-kernel.exe', 'mdr-kernel') | ForEach-Object { Join-Path $PSScriptRoot $_ } | Where-Object { Test-Path -LiteralPath $_ } | Select-Object -First 1
if ($engine -and $env:MDR_ENGINE -ne 'legacy') {
  # Arrays (-Also) travel comma-joined; the engine splits them as this script does.
  $forward = @('-Root', $root) + @(foreach ($p in $PSBoundParameters.GetEnumerator()) {
    if ($p.Value -is [switch]) { if ($p.Value) { "-$($p.Key)" } } else { "-$($p.Key)"; (@($p.Value) -join ',') }
  })
  & $engine close-session @forward
  exit $LASTEXITCODE
}
Set-Location $root

$fail = 0
$autoAcquiredLock = $false
$releasedLock = $false
$pushSucceeded = $false
$keepLock = $false
$localCommitMade = $false
$originalLockToken = $env:BRAIN_LOCK_TOKEN
function Ok  ([string]$m) { Write-Host "[ ok ] $m" }
function Warn([string]$m) { Write-Host "[warn] $m" }
function Fail([string]$m) { Write-Host "[FAIL] $m"; $script:fail++ }
function Get-InboxNotes([string]$InboxPath) {
  if (-not (Test-Path $InboxPath)) { return @() }
  return @(Get-ChildItem $InboxPath -Recurse -File -ErrorAction SilentlyContinue | Where-Object {
    # write-approval-audit.jsonl is the gate's durable audit log, not a foldable note — the
    # doctor already exempts it (CP3); a close-side count that still warns on it every close
    # trains adopters to ignore warnings.
    if ($_.Name -in @('README.md', 'write-approval-audit.jsonl')) { return $false }
    $relative = [IO.Path]::GetRelativePath($root, $_.FullName).Replace('\', '/')
    & git check-ignore --quiet --no-index -- $relative
    return ($LASTEXITCODE -ne 0)
  })
}
function Read-LockState([string]$Path) {
  if (-not (Test-Path -LiteralPath $Path)) { return $null }
  $raw = (Get-Content -Raw -LiteralPath $Path).Trim()
  if (-not $raw) { return $null }
  try { return ($raw | ConvertFrom-Json) } catch { return $null }
}

# Committing identity must be explicit (-Model or process BRAIN_MODEL). The clone's git config
# brain.model stays a display fallback only: on a shared clone it can name another surface, and a
# close-out commit written under it silently misattributes the session (the D274-class trap).
# PowerShell variable names are case-insensitive: assigning $model would overwrite the -Model
# parameter itself, so snapshot the parameter under a distinct name first.
$paramModel = if ($Model) { $Model.Trim() } else { '' }
$model = if ($paramModel) { $paramModel } elseif ($env:BRAIN_MODEL) { ([string]$env:BRAIN_MODEL).Trim() } else { (git config brain.model 2>$null) }
$modelSource = if ($paramModel) { 'param' } elseif ($env:BRAIN_MODEL) { 'env' } elseif ($model) { 'config' } else { 'unknown' }
if (-not $model) { $model = 'unknown' }
if ($modelSource -notin @('param', 'env')) {
  Fail "close-out identity must be explicit (resolved from '$modelSource': '$model') — pass -Model '<model-id>' or set `$env:BRAIN_MODEL for this session; git config brain.model can name another surface"
}
if ($fail) { exit 1 }
# Children (state-lock -Acquire, git commit's pre-commit gate) read BRAIN_MODEL from this process,
# so a -Model given only as a parameter must be exported for the duration of the close.
$originalBrainModel = $env:BRAIN_MODEL
if ($modelSource -eq 'param') { $env:BRAIN_MODEL = $model }
# -Token mirrors -Model: env vars do not survive shell boundaries and agents demonstrably fumble
# cross-shell $env: quoting, then reach for -Release -Force — the unsafe path must not be easier
# than the safe one. A -Token parameter feeds the holder check and the pre-commit child the same
# way BRAIN_LOCK_TOKEN would; the finally below restores the original env value either way.
# (Case-insensitive names: snapshot the parameter, never assign a lowercase $token local.)
$paramToken = if ($Token) { $Token.Trim() } else { '' }
if ($paramToken) { $env:BRAIN_LOCK_TOKEN = $paramToken }

try {
Write-Host "mdr-kernel close-session — $root (model: $model)`n"

# A remote-backed memory has one boring canonical path: its approved PRIVATE origin/main.
$pushRemoteUrl = (git remote get-url --push origin 2>$null)
$hasRemote = [bool]$pushRemoteUrl
$brainMode = (git config --local --get brain.mode 2>$null)
$approvedRemote = (git config --local --get brain.approvedRemote 2>$null)
# A memory approved for a private remote must never silently degrade to local-only: with origin
# gone, this close used to commit locally, release the lock, and record nothing durable. Refuse
# instead, and leave a durable pending-sync marker so doctor keeps naming the recovery until the
# remote is restored (or the operator deliberately re-runs setup-private -LocalOnly).
if (-not $hasRemote -and ($brainMode -eq 'private-remote' -or $approvedRemote)) {
  if (-not $DryRun) { git config --local brain.pendingSync true }
  Fail 'this memory is in private-remote mode but origin is missing — close refused, durable pending-sync recorded. Restore the remote (git remote add origin <approved-url>) or rerun bin/setup-private.ps1 (-LocalOnly to deliberately accept local-only), then rerun close'
}
# The canonical memory branch is main in every mode; a local-only close from a side branch would
# strand coordination truth exactly like a remote-backed one.
$branch = (git branch --show-current 2>$null)
if ($branch -ne 'main') { Fail "memory closes only from main; current branch is '$branch'" }
$offlineApprovalReady = $false
if ($OfflineCheckpoint -and $brainMode -eq 'local-only') {
  Fail 'this memory is already local-only; close normally without -OfflineCheckpoint'
} elseif ($OfflineCheckpoint -and -not $hasRemote) {
  Fail '-OfflineCheckpoint requires an already-approved private remote; no origin is configured'
} elseif ($OfflineCheckpoint -and ($brainMode -ne 'private-remote' -or -not $approvedRemote)) {
  Fail '-OfflineCheckpoint requires a destination approved earlier by setup-private while online'
} elseif ($OfflineCheckpoint) {
  . (Join-Path $PSScriptRoot 'remote-identity.ps1')
  $currentRemoteIdentity = Get-RemoteIdentity $pushRemoteUrl
  # Case-sensitive: canonicalized pins must not re-conflate case-distinct paths via default -ne.
  if ($approvedRemote -cne $currentRemoteIdentity) {
    Fail '-OfflineCheckpoint refused because origin changed since approval; reconnect and rerun bin/setup-private.ps1 before closing'
  } else {
    $offlineApprovalReady = $true
  }
}
if ($hasRemote) {
  if ($OfflineCheckpoint) {
    if ($offlineApprovalReady) {
      Ok 'offline checkpoint: existing private destination approval found; no live verification or push will be attempted'
    }
  } else {
    $privacyScript = Join-Path $PSScriptRoot 'remote-privacy.ps1'
    $privacyOut = @(& pwsh -NoProfile -File $privacyScript -RequireApproved)
    if ($LASTEXITCODE -ne 0) {
      $privacyOut | Write-Host
      Fail 'private push destination is not verified and approved — run bin/setup-private.ps1 before closing'
    } else {
      $privacyOut | Write-Host
      Ok 'push destination: approved private origin/main'
    }
  }
}

# 0) Close set and foreign-staging refusal BEFORE any side effect (lock acquire, handoff repair,
# staging). Another workstream's staged paths are never unstaged, committed, or locked over.
$utcToday   = (Get-Date).ToUniversalTime().ToString('yyyy-MM-dd')
$localToday = (Get-Date).ToString('yyyy-MM-dd')
$dateCandidates = if ($SessionDate) { @($SessionDate) } else {
  @(
    $utcToday
    $localToday
    (Get-Date).ToUniversalTime().AddDays(-1).ToString('yyyy-MM-dd')
    (Get-Date).ToUniversalTime().AddDays(1).ToString('yyyy-MM-dd')
  ) | Select-Object -Unique
}
function Test-InCloseSet([string]$path, [string[]]$coordPaths, [string[]]$handoff, [string[]]$alsoSpecs) {
  $n = $path.Replace('\', '/')
  if ($coordPaths -contains $n) { return $true }
  if ($n -like 'maps/*') { return $true }
  # The write-approval audit is the gate's own artifact and must ride the close that commits an
  # approved file (pre-commit gate 4 requires them staged together).
  if ($n -eq 'inbox/write-approval-audit.jsonl') { return $true }
  if ($handoff -and ($handoff -contains $n)) { return $true }
  foreach ($a in $alsoSpecs) {
    $an = $a.Replace('\', '/').TrimEnd('/')
    if ($n -eq $an -or $n -like "$an/*") { return $true }
  }
  return $false
}
# pwsh -File binds "-Also a,b" as ONE literal string (comma splitting only happens in full
# command parsing), so the documented multi-path form silently matched nothing. Split here.
$Also = @($Also | ForEach-Object { ([string]$_) -split ',' } |
  ForEach-Object { ($_.Trim() -replace '^[''\"]+', '' -replace '[''\"]+$', '').Trim() } | Where-Object { $_ })
$coordList = Join-Path $root 'protocol/coordination-files.txt'
if (-not (Test-Path -LiteralPath $coordList)) { Fail 'coordination policy is missing; restore protocol/coordination-files.txt'; exit 1 }
$coordPaths = @(Get-Content $coordList | Where-Object { $_.Trim() -and $_ -notmatch '^\s*#' } | ForEach-Object { $_.Trim() })
$handoffCandidates = if ($Topic -and -not $NoHandoff) { @($dateCandidates | ForEach-Object { "handoffs/$_-$Topic.md" }) } else { @() }
$earlyForeign = @(git diff --cached --name-only | Where-Object { -not (Test-InCloseSet $_ $coordPaths $handoffCandidates $Also) })
if ($earlyForeign.Count) {
  Fail 'staged paths outside this close set belong to another workstream; nothing unstaged or committed'
  $earlyForeign | ForEach-Object { Write-Host "    $_" }
  exit 1
}

# 1) Resolve shared STATE.lock path.
# Shared lock across canonical + worktrees (dirname(--git-common-dir)); a per-tree lock would
# break single-writer when a session runs from a linked worktree.
$lockCommon = (git -C $root rev-parse --path-format=absolute --git-common-dir 2>$null)
if (-not $lockCommon) { $lockCommon = (git -C $root rev-parse --git-common-dir 2>$null) }
$lockFile = if ($lockCommon -and (Test-Path -LiteralPath (Split-Path -Parent $lockCommon))) {
  Join-Path (Split-Path -Parent $lockCommon) 'STATE.lock'
} else { Join-Path $root 'STATE.lock' }
$holder = $null
$preHeldLockState = $null
if (Test-Path $lockFile) {
  $preHeldLockState = Read-LockState $lockFile
  if ($preHeldLockState) { $holder = $preHeldLockState.model } else { $holder = ((Get-Content -Raw $lockFile).Trim() -split '\s+')[0] }
}
# Prove ownership before any handoff repair or staging. Capture only the token returned by OUR
# atomic acquire; reading another session's token out of STATE.lock never proves ownership.
if ($fail) { exit 1 }
if (-not (Test-Path -LiteralPath $lockFile)) {
  if ($DryRun) { Warn 'STATE.lock free — a real close would acquire it' }
  else {
    $acquireOutput = @(& pwsh -NoProfile -File (Join-Path $root 'bin/state-lock.ps1') -Acquire -Model $model -Objective "session close: $Topic" 2>&1)
    $acquireExit = $LASTEXITCODE
    $acquireOutput | Out-Host
    if ($acquireExit -ne 0) { Fail 'could not acquire STATE.lock; wait for the holder'; exit 1 }
    $autoAcquiredLock = $true
    $holder = $model
    $tokenLine = @($acquireOutput | Where-Object { $_ -match '^session token:\s*(\S+)' }) | Select-Object -First 1
    if ($tokenLine -and $tokenLine -match '^session token:\s*(\S+)') { $env:BRAIN_LOCK_TOKEN = $Matches[1] }
  }
}
if (Test-Path -LiteralPath $lockFile) {
  & pwsh -NoProfile -File (Join-Path $root 'bin/state-lock.ps1') -CheckOwner -Model $model | Out-Host
  if ($LASTEXITCODE -ne 0) { Fail 'STATE.lock ownership unproven — carry YOUR acquired token with -Token, or wait for the holder'; exit 1 }
  Ok "STATE.lock owned by this $model session (token verified)"
}

# 2) Session handoff + INDEX row.
# UTC is the system's default date (state-lock, node-map, STATE headers), but a session closing in
# a western-hemisphere evening legitimately dates its handoff with the host-local day. A UTC-only
# check failed those honest closes — and the 2026-07-12 drive showed the failure mode: the agent
# renamed the handoff (file, title, INDEX row) into the demanded date until the check passed.
# Accept the first existing match among UTC today / local today / UTC±1 and say which was used;
# -SessionDate stays the explicit override for anything outside that window.
$today = $dateCandidates[0]
$handoffRel = $null
if ($NoHandoff) { Warn 'NoHandoff: skipping the handoff check (explicit opt-out)' }
elseif (-not $Topic) { Fail 'missing -Topic (or pass -NoHandoff): expected handoffs/<today>-<topic>.md' }
else {
  $matchedDates = @($dateCandidates | Where-Object { Test-Path -LiteralPath (Join-Path $root "handoffs/$_-$Topic.md") })
  if ($matchedDates.Count) {
    $today = $matchedDates[0]
    if (-not $SessionDate -and $today -ne $utcToday) {
      $flavor = if ($today -eq $localToday) { 'host-local today' } else { 'UTC±1 day' }
      Ok "handoff date $today accepted ($flavor; UTC today is $utcToday)"
    }
  }
  $handoffRel = "handoffs/$today-$Topic.md"
  $handoffPath = Join-Path $root $handoffRel
  if (Test-Path $handoffPath) {
    Ok "handoff present: $handoffRel"
    $handoffText = Get-Content -Raw -LiteralPath $handoffPath

    # Auto-repair the one narrow mechanical envelope defect that otherwise costs a close-and-rerun:
    # a correct terminal `## SYSTEM NODE LINKS` block missing only the required `---` + blank line
    # before it. Only this exact case is healed; a truly malformed handoff still fails below.
    if (
      $handoffText.TrimEnd() -notmatch '---\r?\n\r?\n## SYSTEM NODE LINKS' -and
      $handoffText.TrimEnd() -match '(?m)^## SYSTEM NODE LINKS\r?\n\r?\nDirectory Path: [^\r\n]+\r?\nNext Sequential File: [^\r\n]+\r?\nRelated Workflows: [^\r\n]+\r?\nAI Execution Prompts: [^\r\n]+\s*\z'
    ) {
      $repaired = [regex]::Replace($handoffText, '(?s)\r?\n\s*(## SYSTEM NODE LINKS\b)', "`n`n---`n`n`$1", 1)
      $handoffText = $repaired
      if ($DryRun) { Warn 'DryRun: would insert missing handoff separator' }
      else {
        Set-Content -LiteralPath $handoffPath -Value $repaired -NoNewline
        Ok 'handoff envelope auto-repaired: inserted the missing `---` separator before SYSTEM NODE LINKS'
      }
    }

    $nonEmpty = @($handoffText -split "`r?`n" | Where-Object { $_.Trim() })
    if (
      $nonEmpty.Count -ge 3 -and
      $nonEmpty[0] -match '^Title:\s+.+' -and
      $nonEmpty[1] -match '^Type:\s+.+' -and
      $nonEmpty[2] -match '^Context:\s+.+' -and
      $handoffText.TrimEnd() -match '---\r?\n\r?\n## SYSTEM NODE LINKS\r?\n\r?\nDirectory Path: [^\r\n]+\r?\nNext Sequential File: [^\r\n]+\r?\nRelated Workflows: [^\r\n]+\r?\nAI Execution Prompts: [^\r\n]+\z'
    ) { Ok 'handoff Markdown retrieval envelope present' }
    else { Fail "handoff missing Markdown retrieval envelope — start from _templates/handoff-template.md" }
  } else { Fail "handoff missing: $handoffRel — write it before closing (accepted dates for -Topic '$Topic': $($dateCandidates -join ', '); a handoff dated outside that window needs -SessionDate <yyyy-MM-dd>, never a rename)" }
  $idx = Get-Content -Raw (Join-Path $root 'handoffs/INDEX.md') -ErrorAction SilentlyContinue
  if ($idx -and $idx -match [regex]::Escape("$today-$Topic.md")) {
    Ok 'handoffs/INDEX.md row present'
    # Advisory: INDEX promises "one line per handoff"; paragraph rows duplicate the handoff body
    # and tax every future INDEX read.
    $row = @(Get-Content (Join-Path $root 'handoffs/INDEX.md') | Where-Object { $_ -match [regex]::Escape("$today-$Topic.md") })[0]
    if ($row -and $row.Length -gt 600) { Warn "INDEX row is $($row.Length) chars — keep it to a sentence or two; the detail belongs in the handoff itself" }
  }
  else { Fail "handoffs/INDEX.md has no row for $today-$Topic.md" }
}

# 3) STATE.md freshness: pending changes now, or already committed today.
$stateDirty = git status --porcelain -- STATE.md
$stateLast  = git log -1 --format=%cs -- STATE.md 2>$null
if ($stateDirty) { Ok 'STATE.md has pending updates (will be committed)' }
elseif ($stateLast -eq $today) { Ok "STATE.md already committed today" }
else { Warn "STATE.md untouched today (last commit $stateLast) — is it still accurate?" }

# 3b) STATE budget: surface headroom/overage with the targeting breakdown at close time, so an
# over-budget STATE costs one targeted edit, not an edit/measure/re-read trim loop.
$budgetOut = @(& pwsh -NoProfile -File (Join-Path $root 'bin/state-budget.ps1'))
if ($LASTEXITCODE -eq 0) { Ok "STATE budget: $($budgetOut[0])" }
else {
  Warn "STATE budget: $($budgetOut[0])"
  $budgetOut | Select-Object -Skip 1 | Write-Host
}

# 4) inbox should be folded (or consciously left).
$notes = @(Get-InboxNotes (Join-Path $root 'inbox'))
if ($notes.Count) { Warn "inbox/ has $($notes.Count) unfolded note(s) — fold them or say in the handoff why they wait" }
else { Ok 'inbox/ empty' }

  # 5) Doctor: only its hard failures (e.g. committed secret) block a close, so run just the
  # hazard gates (-HardOnly) — the full advisory sweep already ran at session start and its
  # output is discarded here anyway.
  & pwsh -NoProfile -File (Join-Path $root 'bin/brain-doctor.ps1') -HardOnly | Out-Null
  if ($LASTEXITCODE -ne 0) { Fail 'brain-doctor reports a hard failure — run it and fix before closing' }
  else { Ok 'brain-doctor: no hard failures' }

  # Close-set staging: coordination files, this handoff, generated maps,
  # and paths explicitly named with -Also. Stray downloads and another session's work stay out.
  # --untracked-files=all: plain porcelain collapses a fully-untracked directory into one 'dir/'
  # entry, so an -Also naming a file INSIDE it matched nothing and the file silently missed the
  # close. Enumerating every file keeps -Also exact-match semantics for new directories too.
  function Get-PendingPaths {
    @(git status --porcelain --untracked-files=all | ForEach-Object {
      $p = $_.Substring(3)
      if ($p -match ' -> ') { $p = ($p -split ' -> ')[-1] }
      $p.Trim('"')
    })
  }
  # A typo'd -Also path silently stages nothing and the session believes its file rode the close.
  # Name every spec that matches no pending change so the miss is loud before the commit.
  function Write-UnmatchedAlso([string[]]$alsoSpecs, [string[]]$pending) {
    foreach ($a in $alsoSpecs) {
      $an = $a.Replace('\', '/').TrimEnd('/')
      $hit = @($pending | Where-Object { $n = $_.Replace('\', '/'); ($n -eq $an) -or ($n -like "$an/*") })
      if (-not $hit.Count) { Warn "-Also '$a' matched no pending change - check the path (already committed, or a typo?)" }
    }
  }
  function Add-ClosePathToIndex([string]$path) {
    git add -A -- $path 2>$null | Out-Null
    if ($LASTEXITCODE -ne 0) {
      # A pre-staged deletion can leave a path matching neither worktree nor index pathspec. Its
      # intent is already satisfied only when the deletion is present in the index.
      if (-not @(git diff --cached --name-only -- ($path.Replace('\', '/'))).Count) {
        Fail "could not stage '$path' - stage everything yourself (git add) or nothing, then rerun close"
        exit 1
      }
    }
  }
  $pendingPaths = Get-PendingPaths
  $foreignStaged = @(git diff --cached --name-only | Where-Object { -not (Test-InCloseSet $_ $coordPaths $handoffRel $Also) })
  if ($foreignStaged.Count) {
    Fail 'staged paths outside this close set belong to another workstream; nothing unstaged or committed'
    $foreignStaged | ForEach-Object { Write-Host "    $_" }
    exit 1
  }
  if ($DryRun) {
    Write-Host "`n-- DryRun: would stage (close set) --"
    Write-UnmatchedAlso $Also $pendingPaths
    $stagePaths = @($pendingPaths | Where-Object { Test-InCloseSet $_ $coordPaths $handoffRel $Also })
    $leftover = @($pendingPaths | Where-Object { -not (Test-InCloseSet $_ $coordPaths $handoffRel $Also) })
    if ($stagePaths.Count) { $stagePaths | Write-Host } else { Write-Host '(close set clean)' }
    if ($leftover.Count) { Write-Host "-- would leave in tree ($($leftover.Count)) --"; $leftover | Write-Host }
    Write-Host "`nclose-session DryRun: $fail failing check(s)."
    exit ([int]($fail -gt 0))
  }
  if ($fail) { Write-Host "`nclose-session: $fail failing check(s) — nothing committed."; exit 1 }

  # 6) Stage + commit (trailer explicit; prepare-commit-msg backstops, never double-stamps).
  if (@($pendingPaths | Where-Object { Test-InCloseSet $_ $coordPaths $handoffRel $Also }).Count) {
    # Retrieval truth is defined over git-tracked Markdown. Pre-stage only Markdown already in
    # this session's close set, then generate; unrelated lanes remain untouched and unindexed.
    $preMapMarkdown = @($pendingPaths | Where-Object {
      (Test-InCloseSet $_ $coordPaths $handoffRel $Also) -and $_.Replace('\', '/').EndsWith('.md', [StringComparison]::OrdinalIgnoreCase)
    })
    foreach ($sp in $preMapMarkdown) { Add-ClosePathToIndex $sp }
    $nodeMapScript = Join-Path $root 'bin/update-node-map.ps1'
    if (Test-Path -LiteralPath $nodeMapScript) {
      & pwsh -NoProfile -File $nodeMapScript | Out-Host
      if ($LASTEXITCODE -ne 0) { Fail 'node-map regeneration failed after staging — see output above'; exit 1 }
      $pendingPaths = Get-PendingPaths
    }
    Write-UnmatchedAlso $Also $pendingPaths
    $stagePaths = @($pendingPaths | Where-Object { Test-InCloseSet $_ $coordPaths $handoffRel $Also })
    $leftover = @($pendingPaths | Where-Object { -not (Test-InCloseSet $_ $coordPaths $handoffRel $Also) })
    foreach ($sp in $stagePaths) {
      Add-ClosePathToIndex $sp
    }
    if ($leftover.Count) {
      Warn "left in tree, NOT committed ($($leftover.Count) path(s)); pass -Also only for paths this session owns:"
      $leftover | Select-Object -First 15 | ForEach-Object { Write-Host "    $_" }
      # A staged INDEX row must never point at a file this close leaves behind: that pushes a
      # repo broken for every fresh clone while the local doctor stays green.
      $stagedIndexes = @(git diff --cached --name-only | Where-Object { $_ -match '(^|/)INDEX\.md$' })
      foreach ($idx in $stagedIndexes) {
        $addedLines = @(git diff --cached -U0 -- $idx | Where-Object { $_ -match '^\+' })
        foreach ($lo in $leftover) {
          $loName = [IO.Path]::GetFileName(($lo.Replace('\', '/')))
          if ($loName -and ($addedLines | Where-Object { $_ -match [regex]::Escape($loName) })) {
            Fail "$idx gains a row referencing left-behind '$lo' — commit them together: rerun with -Also '$lo' (a new reference/skill file also needs its bin/write-approval.ps1 audit entry, which rides automatically)"
            exit 1
          }
        }
      }
    }
    $staged = @(git diff --cached --name-only)
    if ($staged.Count) {
      Write-Host '-- staging --'
      git status --short | Write-Host
      $subject = if ($Message) { $Message } elseif ($Topic) { "Session close: $Topic" } else { "Session close: $today" }
      $trailers = @(if ($LeastCode) { "Least-code: $LeastCode" }; if ($LeastFile) { "Least-file: $LeastFile" }; "Brain-Model: $model")
      git commit -m "$subject`n`n$($trailers -join "`n")" | Out-Host
      if ($LASTEXITCODE -ne 0) { Fail 'git commit failed (pre-commit gate?) — see output above'; exit 1 }
      $localCommitMade = $true
      Ok 'committed'
    } else { Ok 'close set already clean — nothing to commit' }
  } else {
    Write-UnmatchedAlso $Also @()
    Ok 'close set already clean — nothing to commit'
    if ($pendingPaths.Count) { Warn "left $($pendingPaths.Count) unrelated path(s) untouched" }
  }

  # 7) Push when a remote exists. OfflineCheckpoint is an explicit escape hatch: it commits
  # locally, records loud pending-sync state, and releases the lock without attempting a push.
  if ($OfflineCheckpoint) {
    git config --local brain.pendingSync true
    Warn 'OFFLINE CHECKPOINT: committed locally and did not push. Recovery: pwsh -NoProfile -File bin/sync-private.ps1'
  } elseif (-not $hasRemote) {
    if ($brainMode -eq 'local-only') { Ok 'local-only memory: committed locally (no push by design)' }
    else { Warn 'no git remote configured — committed locally, skipping push. Add a remote and push when ready.' }
  } else {
    & pwsh -NoProfile -File (Join-Path $root 'bin/sync-private.ps1') -PreserveWorkingTree
    if ($LASTEXITCODE -ne 0) {
      $keepLock = $true
      git config --local brain.pendingSync true
      Fail 'private sync failed — STATE.lock kept. Resolve safely, run bin/sync-private.ps1, then release the lock'
      exit 1
    }
    $pushSucceeded = $true
    Ok 'pushed'
  }
  & pwsh -NoProfile -File (Join-Path $root 'bin/state-lock.ps1') -Release | Out-Host
  if ($LASTEXITCODE -ne 0) { Fail 'could not release this session lock'; exit 1 }
  $releasedLock = $true

  # Untracked-files=no: this close intentionally leaves stray untracked paths behind (another
  # lane's scratch output, an -Also path that matched nothing) — only uncommitted tracked changes
  # after the commit/push above are a real problem here.
  if (-not (git status --porcelain --untracked-files=no)) { Ok 'working tree clean (tracked files)' } else { Warn 'tracked changes remain uncommitted after close — inspect git status' }
  Write-Host "`nclose-session: done."
  # The one sanctioned close ceremony (docs/brand-manual.md): a clean exit earns the line.
  # Offline checkpoints skip it — the reward waits until the pending sync lands.
  if (-not $OfflineCheckpoint) {
    Write-Host 'Waffle Party protocol authorized. Your continued dedication to the Work has been recorded.' -ForegroundColor Yellow
  }
  exit 0
} finally {
  if ($autoAcquiredLock -and -not $releasedLock -and -not $keepLock -and (Test-Path -LiteralPath $lockFile)) {
    & pwsh -NoProfile -File (Join-Path $root 'bin/state-lock.ps1') -Release | Out-Null
  }
  if ($null -eq $originalLockToken) { Remove-Item Env:\BRAIN_LOCK_TOKEN -ErrorAction SilentlyContinue }
  else { $env:BRAIN_LOCK_TOKEN = $originalLockToken }
  if ($modelSource -eq 'param') {
    if ($null -eq $originalBrainModel) { Remove-Item Env:\BRAIN_MODEL -ErrorAction SilentlyContinue }
    else { $env:BRAIN_MODEL = $originalBrainModel }
  }
}
