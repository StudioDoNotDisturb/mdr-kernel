#requires -Version 7.0
<#
.SYNOPSIS
  Structured advisory lock for mdr-kernel coordination files.
  STATE.lock body is JSON: model, host, pid, objective, acquired_at, heartbeat_at, base_commit, token.

  - Acquire is atomic (FileMode CreateNew — no test-then-write race).
  - Acquire requires an explicit identity (-Model or $env:BRAIN_MODEL) — the git-config fallback
    on a shared clone can name another surface, so a claim of ownership refuses it.
  - Re-acquiring your own lock refreshes the heartbeat (idempotent for stateless CLI sessions).
  - Per-session token: a session carries its lock token in $env:BRAIN_LOCK_TOKEN (or -Token). When a
    lock record has a token, "yours" means model(+host) AND token match — so a SECOND same-model
    session on the same host cannot silently re-acquire, heartbeat, or release the first session's
    lock. Legacy tokenless locks still fall back to model+host during transition.
  - -Acquire -Wait polls every 5s until the lock frees (default budget -TimeoutSec 300), replacing
    ad-hoc sleep/monitor loops in sessions queued behind a live holder.
  - Release/heartbeat verify ownership. -Force takes over a lock whose heartbeat is STALE (>120m,
    the brain-doctor threshold) and refuses a fresh-heartbeat lock: a live partner's lock is never
    destroyed on a hunch. MDR_FORCE_LIVE_LOCK=1 is the operator valve for a holder confirmed dead
    despite a fresh heartbeat (e.g. a crash seconds after a commit).
  - Heartbeat is refreshed automatically by every commit the holder makes (.githooks/pre-commit),
    so staleness is judged by heartbeat age, not acquire age. Checkpoint commits keep it fresh.
  - pid is forensic only: each pwsh invocation is short-lived, so liveness cannot be inferred
    from it; heartbeat age is the liveness signal.

  NOT a hard lock — a courtesy flag brain-doctor and pre-commit enforce cooperatively. STATE.lock
  is git-ignored (transient/local); cross-machine serialization happens through git pull/push.
.EXAMPLE
  pwsh -NoProfile -File bin/state-lock.ps1 -Acquire -Model '<your-model-id>' -Objective 'fold inbox into STATE'
  pwsh -NoProfile -File bin/state-lock.ps1 -Acquire -Wait -Model '<your-model-id>'   # queue behind a live holder
  $env:BRAIN_LOCK_TOKEN='<token printed by -Acquire>'   # seal same-model concurrency
  pwsh -NoProfile -File bin/state-lock.ps1 -Heartbeat
  pwsh -NoProfile -File bin/state-lock.ps1 -Release
  pwsh -NoProfile -File bin/state-lock.ps1 -Release -Token '<your token>'   # own lock from a fresh shell
  pwsh -NoProfile -File bin/state-lock.ps1 -Release -Force   # take over a STALE lock only (heartbeat >120m)
  pwsh -NoProfile -File bin/state-lock.ps1                   # status
#>
[CmdletBinding()]
param(
  [switch]$Acquire,
  [switch]$Release,
  [switch]$Heartbeat,
  [switch]$Force,
  [switch]$Wait,          # with -Acquire: poll until the lock frees (or -TimeoutSec elapses)
  [int]$TimeoutSec = 300, # -Wait budget; ~5 min default keeps one tool call sufficient
  [string]$Model,
  [string]$Token,
  [string]$Objective
)

$root  = Split-Path -Parent $PSScriptRoot
# STATE.lock must be ONE shared file across the canonical clone and all its linked worktrees, or
# each working tree gets its own lock and the single-writer invariant breaks. Anchor it to
# dirname(--git-common-dir) = the canonical repo top, which every worktree resolves to identically.
# Fall back to $root only when git can't answer (non-git context).
function Resolve-SharedLock([string]$fallbackRoot) {
  $cd = (git -C $fallbackRoot rev-parse --path-format=absolute --git-common-dir 2>$null)
  if (-not $cd) { $cd = (git -C $fallbackRoot rev-parse --git-common-dir 2>$null) }
  if ($cd) {
    $top = Split-Path -Parent $cd
    if ($top -and (Test-Path -LiteralPath $top)) { return (Join-Path $top 'STATE.lock') }
  }
  return (Join-Path $fallbackRoot 'STATE.lock')
}
$lock  = Resolve-SharedLock $root
# Identity resolution: -Model param, then process BRAIN_MODEL, then the clone's git-config
# fallback. The source is tracked because -Acquire refuses the config fallback below: on a
# shared clone that value can name another surface (the classic attribution trap).
# PowerShell variable names are case-insensitive: assigning $model would overwrite the -Model
# parameter itself, so snapshot the parameter under a distinct name first.
$paramModel = if ($Model) { $Model.Trim() } else { '' }
$model = $null; $modelSource = 'unknown'
if ($paramModel) { $model = $paramModel; $modelSource = 'param' }
elseif ($env:BRAIN_MODEL) { $model = ([string]$env:BRAIN_MODEL).Trim(); $modelSource = 'env' }
else {
  $model = (git -C $root config brain.model 2>$null)
  if ($model) { $modelSource = 'config' }
}
if (-not $model) { $model = 'unknown' }
$hostName = [Environment]::MachineName
# Per-session token carrier: -Token wins, else $env:BRAIN_LOCK_TOKEN, else none (legacy).
$sessionToken = if ($Token) { $Token.Trim() } elseif ($env:BRAIN_LOCK_TOKEN) { ([string]$env:BRAIN_LOCK_TOKEN).Trim() } else { $null }
$sessionObjective = if ($Objective) { $Objective.Trim() } elseif ($env:BRAIN_OBJECTIVE) { ([string]$env:BRAIN_OBJECTIVE).Trim() } else { $null }
# Heartbeat older than this is a dead session (same threshold brain-doctor reports as stale).
# Below it, a holder must be presumed ALIVE and its lock is never force-released without the valve.
$staleAfterMinutes = 120

function Read-Lock {
  if (-not (Test-Path $lock)) { return $null }
  $raw = (Get-Content -Raw -LiteralPath $lock).Trim()
  if (-not $raw) { return $null }
  try { return ($raw | ConvertFrom-Json) } catch {}
  # Legacy single-line "model timestamp" body: tolerate during transition.
  $parts = $raw -split '\s+'
  return [pscustomobject]@{
    model = $parts[0]; host = $null; pid = $null; objective = $null
    acquired_at = $parts[-1]; heartbeat_at = $parts[-1]; base_commit = $null; token = $null
  }
}

# ConvertFrom-Json (PS7) auto-parses ISO strings into DateTime; normalize every timestamp
# back to a DateTimeOffset so we never re-serialize in locale format.
function To-Dto([object]$v) {
  if ($v -is [DateTimeOffset]) { return $v }
  if ($v -is [DateTime])       { return [DateTimeOffset]$v }
  try { return [DateTimeOffset]::Parse("$v") } catch { return $null }
}
function To-Iso([object]$v, [string]$fallback) {
  $dto = To-Dto $v
  if ($null -ne $dto) { return $dto.ToUniversalTime().ToString('o') }
  return $fallback
}

function Heartbeat-AgeMinutes([object]$l) {
  $ts = if ($l.heartbeat_at) { $l.heartbeat_at } else { $l.acquired_at }
  $dto = To-Dto $ts
  if ($null -eq $dto) { return $null }
  return [math]::Round(([DateTimeOffset]::UtcNow - $dto).TotalMinutes)
}

function Describe([object]$l) {
  $age = Heartbeat-AgeMinutes $l
  $hb  = if ($null -ne $age) { "heartbeat ${age}m ago" } else { 'heartbeat unreadable' }
  $on  = if ($l.host) { " on $($l.host)" } else { '' }
  $obj = if ($l.objective) { "; objective: $($l.objective)" } else { '; objective: MISSING' }
  "$($l.model)$on (acquired $(To-Iso $l.acquired_at '?'), $hb, base $($l.base_commit)$obj)"
}

# Same model (+host when recorded). A legacy lock has no host; match on model alone there.
function Test-BaseMatch([object]$l) {
  ($l.model -eq $model) -and ((-not $l.host) -or ($l.host -eq $hostName))
}
# "Mine" = base match AND, for a sealed lock, explicit token match.
function Is-Mine([object]$l) {
  if (-not (Test-BaseMatch $l)) { return $false }
  if ($l.token) { return [bool]$sessionToken -and ($l.token -eq $sessionToken) }
  return $true
}
# A DIFFERENT session of the same model: base matches, but the sealed lock token is absent here or disagrees.
function Is-SameModelOtherSession([object]$l) {
  if (-not (Test-BaseMatch $l)) { return $false }
  if (-not $l.token) { return $false }
  return ((-not $sessionToken) -or ($l.token -ne $sessionToken))
}

function New-Body([object]$AcquiredAt, [string]$Token, [string]$ExistingObjective) {
  $now = [DateTimeOffset]::UtcNow.ToString('o')
  $AcquiredAt = To-Iso $AcquiredAt $now
  if (-not $Token) { $Token = [guid]::NewGuid().ToString('N') }
  $base = (git -C $root rev-parse --short HEAD 2>$null)
  $objectiveText = if ($sessionObjective) { $sessionObjective } else { $ExistingObjective }
  [ordered]@{
    model        = $model
    host         = $hostName
    pid          = $PID
    objective    = $objectiveText
    acquired_at  = $AcquiredAt
    heartbeat_at = $now
    base_commit  = "$base"
    token        = $Token
  } | ConvertTo-Json
}

if ($Acquire) {
  # An acquire is a claim of NEW exclusive ownership — the identity must be explicit.
  # Status/-Release/-Heartbeat keep the config fallback; only the claim itself is fail-closed.
  if ($modelSource -notin @('param', 'env')) {
    Write-Host "state-lock: refusing -Acquire — identity resolved from '$modelSource' ('$model'), not from this session."
    Write-Host 'On a shared clone the config fallback can name another surface. Pass identity explicitly:'
    Write-Host "  PowerShell:  pwsh -NoProfile -File bin/state-lock.ps1 -Acquire -Model '<model-id>' -Objective '<objective>'"
    Write-Host "  bash:        BRAIN_MODEL=<model-id> pwsh -NoProfile -File bin/state-lock.ps1 -Acquire -Objective '<objective>'"
    exit 1
  }
  $tok = if ($sessionToken) { $sessionToken } else { [guid]::NewGuid().ToString('N') }
  $deadline = [DateTimeOffset]::UtcNow.AddSeconds([Math]::Max(5, $TimeoutSec))
  $announced = $false
  while ($true) {
    try {
      $fs = [System.IO.File]::Open($lock, [System.IO.FileMode]::CreateNew, [System.IO.FileAccess]::Write)
      try {
        $bytes = [System.Text.Encoding]::UTF8.GetBytes((New-Body $null $tok $null))
        $fs.Write($bytes, 0, $bytes.Length)
      } finally { $fs.Close() }
      Write-Host "acquired STATE.lock as $model@$hostName"
      if ($sessionObjective) { Write-Host "objective: $sessionObjective" }
      else { Write-Host 'warning: no objective recorded; pass -Objective or set BRAIN_OBJECTIVE so parallel sessions know your scope' }
      if (-not $sessionToken) {
        Write-Host "session token: $tok"
        Write-Host "to seal same-model concurrency, export it for this session:"
        Write-Host "  PowerShell:  `$env:BRAIN_LOCK_TOKEN='$tok'"
        Write-Host "  bash:        export BRAIN_LOCK_TOKEN=$tok"
      }
      exit 0
    } catch [System.IO.IOException] {
      $l = Read-Lock
      if ($l -and (Is-Mine $l)) {
        New-Body $l.acquired_at $l.token $l.objective | Set-Content -LiteralPath $lock -NoNewline
        Write-Host "STATE.lock already yours ($model@$hostName) — heartbeat refreshed"
        exit 0
      }
      # Queue politely before printing any refusal: one supported call replaces every improvised
      # sleep/monitor loop, and a waiter never needs the takeover recipe at all.
      if ($Wait -and [DateTimeOffset]::UtcNow -lt $deadline) {
        if (-not $announced) {
          Write-Host "STATE.lock busy -> $(if ($l) { Describe $l } else { '(unreadable)' }); polling every 5s (budget ${TimeoutSec}s)..."
          $announced = $true
        }
        Start-Sleep -Seconds 5
        continue
      }
      $age = if ($l) { Heartbeat-AgeMinutes $l } else { $null }
      $isStale = ($null -ne $age -and $age -gt $staleAfterMinutes)
      if ($l -and (Is-SameModelOtherSession $l)) {
        # The refusal is the ONLY guidance an un-briefed agent has at this moment, so it must
        # lead with the safe moves and never hand a live-lock takeover recipe to the refused
        # party (2026-07-12 drive: refusal -> bare -Release -Force on a 21-second-old live lock).
        Write-Host "STATE.lock held by another $model session (token mismatch) -> $(Describe $l)"
        Write-Host 'Same-model acquires are fail-closed: a live partner lock is never silently taken over.'
        Write-Host '  your own lock?      re-run with -Token <token>   (printed by YOUR -Acquire; if you cannot produce it, treat the lock as not yours)'
        Write-Host '  waiting your turn?  re-run with -Acquire -Wait   (polls every 5s until the holder releases)'
        if ($isStale) {
          Write-Host "  holder dead?        heartbeat is ${age}m old (stale past ${staleAfterMinutes}m). If brain-doctor agrees: bin/state-lock.ps1 -Release -Force, then -Acquire"
        } else {
          Write-Host "  holder dead?        NOT PROVEN — heartbeat is $(if ($null -ne $age) { "only ${age}m" } else { 'not readable as' }) old. The holder is presumed ALIVE; -Release -Force refuses fresh-heartbeat locks. Queue with -Wait instead."
        }
      } else {
        Write-Host "STATE.lock already held -> $(if ($l) { Describe $l } else { '(unreadable)' })"
        if ($isStale) {
          Write-Host "heartbeat is ${age}m old — likely a dead session. If brain-doctor agrees, take it over with: bin/state-lock.ps1 -Release -Force, then -Acquire"
        } elseif (-not $Wait) {
          Write-Host 'to queue for it instead of failing: re-run with -Acquire -Wait'
        }
      }
      if ($Wait) { Write-Host "gave up after ${TimeoutSec}s (-Wait budget); re-run to keep waiting" }
      exit 1
    }
  }
}

if ($Heartbeat) {
  $l = Read-Lock
  if (-not $l) { Write-Host 'no STATE.lock to heartbeat'; exit 1 }
  if (-not (Is-Mine $l)) {
    if (Is-SameModelOtherSession $l) { Write-Host "STATE.lock held by another $model session (token mismatch) — not refreshing" }
    else { Write-Host "STATE.lock held by $($l.model), not $model — not refreshing" }
    exit 1
  }
  New-Body $l.acquired_at $l.token $l.objective | Set-Content -LiteralPath $lock -NoNewline
  Write-Host "heartbeat refreshed ($model@$hostName)"
  exit 0
}

if ($Release) {
  $l = Read-Lock
  if (-not $l) { Write-Host 'no STATE.lock to release'; exit 0 }
  if (Is-Mine $l) {
    Remove-Item -LiteralPath $lock
    Write-Host 'released STATE.lock'
    exit 0
  }
  if ($Force) {
    # Force is dead-session recovery, not a tiebreaker: refuse while the heartbeat says the
    # holder is alive (2026-07-12 drive: a bare -Release -Force destroyed a live partner's
    # 21-second-old lock). An unreadable heartbeat cannot prove liveness, so it stays forcible.
    $age = Heartbeat-AgeMinutes $l
    if ($null -ne $age -and $age -le $staleAfterMinutes -and $env:MDR_FORCE_LIVE_LOCK -ne '1') {
      Write-Host "refusing -Release -Force: heartbeat is only ${age}m old (stale threshold ${staleAfterMinutes}m) — the holder is presumed ALIVE:"
      Write-Host "  $(Describe $l)"
      Write-Host '  queue instead:      bin/state-lock.ps1 -Acquire -Wait'
      Write-Host '  your own lock?      bin/state-lock.ps1 -Release -Token <token>   (printed by YOUR -Acquire)'
      Write-Host '  holder confirmed dead despite the fresh heartbeat (crash after a commit)? set MDR_FORCE_LIVE_LOCK=1 and re-run.'
      exit 1
    }
    Remove-Item -LiteralPath $lock
    Write-Host "FORCE-released STATE.lock held by $(Describe $l) — make sure that session was dead"
    exit 0
  }
  if (Is-SameModelOtherSession $l) {
    Write-Host "STATE.lock held by another $model session (token mismatch) — not releasing."
    Write-Host 'Set BRAIN_LOCK_TOKEN / pass -Token <token> to release your own, or -Force if that session is dead.'
    exit 1
  }
  Write-Host "STATE.lock held by $(Describe $l) — not yours ($model@$hostName)."
  Write-Host 'If brain-doctor reports it stale/dead, re-run with -Force.'
  exit 1
}

# Status
$l = Read-Lock
if ($l) {
  Write-Host "STATE.lock held -> $(Describe $l)"
  $age = Heartbeat-AgeMinutes $l
  if ($null -ne $age -and $age -gt $staleAfterMinutes) { Write-Host "heartbeat is ${age}m old — likely stale; see brain-doctor" }
} else { Write-Host 'STATE.lock: free' }
exit 0
