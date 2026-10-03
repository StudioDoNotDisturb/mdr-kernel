#requires -Version 7.0
<#
.SYNOPSIS
  Optional Stop-hook body for CLI agents (Claude Code, Codex, ...). Read-only nudge: when the agent
  stops while the repo has uncommitted work or a held STATE.lock, emit a systemMessage pointing at
  the close-out path (bin/close-session.ps1). Never writes, commits, or blocks.

  Wire it into your tool's Stop hook (e.g. .claude/settings.json or .codex/hooks.json). It is not
  required for CLI-only use; close-session works the same whether or not this nudge fires.
#>
$root = Split-Path -Parent $PSScriptRoot
$msgs = @()

$dirty = git -C $root status --porcelain 2>$null
if ($dirty) {
  $msgs += 'mdr-kernel has uncommitted changes — close out with: pwsh -NoProfile -File bin/close-session.ps1 -Topic <handoff-slug> -Model <your-model-id> (verifies handoff/STATE, commits with your trailer, pushes, releases the lock).'
}

# Shared lock across canonical + worktrees (dirname(--git-common-dir)) so a worktree session sees
# the same lock the canonical clone holds.
$lockCommon = (git -C $root rev-parse --path-format=absolute --git-common-dir 2>$null)
if (-not $lockCommon) { $lockCommon = (git -C $root rev-parse --git-common-dir 2>$null) }
$lock = if ($lockCommon -and (Test-Path -LiteralPath (Split-Path -Parent $lockCommon))) {
  Join-Path (Split-Path -Parent $lockCommon) 'STATE.lock'
} else { Join-Path $root 'STATE.lock' }
if (Test-Path $lock) {
  $holder = $null
  $raw = Get-Content -Raw $lock -ErrorAction SilentlyContinue
  if ($raw) { try { $holder = ($raw | ConvertFrom-Json).model } catch { $holder = ($raw.Trim() -split '\s+')[0] } }
  $msgs += "STATE.lock is still held by '$holder' — fine mid-session; close-session releases it after a successful push; a lock you hold outside close: pwsh -NoProfile -File bin/state-lock.ps1 -Release -Model <your-model-id> -Token <your acquired token>."
}

if ($msgs.Count) {
  @{ systemMessage = ($msgs -join ' ') } | ConvertTo-Json -Compress | Write-Output
} else {
  # Stop hooks expect JSON on a successful exit; an empty object is the explicit no-op.
  Write-Output '{}'
}
