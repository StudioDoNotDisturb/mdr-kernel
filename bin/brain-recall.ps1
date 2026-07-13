#requires -Version 7.0
<#
.SYNOPSIS
  Surfaces action-time recall gates for an operator objective.
.DESCRIPTION
  Matches an objective against protocol/recall-gates.json and prints the required prior
  sources and recurrence-counted traps before a model acts. This closes the stored-but-unused
  knowledge gap by making recall a command, not a memory note.
#>
[CmdletBinding()]
param(
  # Objective is required for normal matching, but optional when -GateId selects a gate
  # directly (tool-time surfacing: a script names its own gate, there is no objective to match).
  [string]$Objective,

  [switch]$Json,
  [switch]$Quiet,
  [string]$GateId,

  # Print a COMPACT trap banner for -GateId, sized to sit at the top of a domain tool's
  # output. The model cannot un-see output its own command produced — that is the seam.
  [switch]$Header,

  # Increment a trap's (and its gate's) times_recurred by one command, so recording a repeat
  # is a one-liner instead of hand-editing protected JSON. The write still needs STATE.lock to
  # COMMIT (pre-commit enforces it); this just removes the editing friction.
  [string]$Increment
)

if (-not $Increment -and -not $GateId -and [string]::IsNullOrWhiteSpace($Objective)) {
  Write-Error 'Provide -Objective for matching, -GateId to select a gate, or -Increment <trapId> to record a repeat.'
  exit 2
}

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$gatePath = Join-Path $root 'protocol/recall-gates.json'
$Utf8NoBom = New-Object System.Text.UTF8Encoding($false)

function As-Array($value) {
  if ($null -eq $value) { return @() }
  if ($value -is [System.Array]) { return @($value) }
  return @($value)
}

function Text([string]$value) {
  if ($null -eq $value) { return '' }
  return [string]$value
}

# Tolerant matching. A term hits the objective when:
#  - multi-word term ("same song", "docker compose"): phrase appears as a substring, OR
#  - single token ("loop", "tun", "again"): appears on a WORD BOUNDARY (regex \bterm\b).
# Word-boundary matching for single tokens kills false positives (fortune->"tun",
# champagne->"again") that raw .Contains() produced, while still catching the real word.
function Term-Hits([string]$objectiveLower, [string]$term) {
  $t = (Text $term).Trim().ToLowerInvariant()
  if (-not $t) { return $false }
  if ($t -match '\s') {
    return $objectiveLower.Contains($t)
  }
  return [System.Text.RegularExpressions.Regex]::IsMatch(
    $objectiveLower, ('\b' + [System.Text.RegularExpressions.Regex]::Escape($t) + '\b'))
}

if (-not (Test-Path -LiteralPath $gatePath)) {
  Write-Error "Missing recall gate data: $gatePath (copy protocol/recall-gates.example.json, or run bin/bootstrap-clone.ps1)"
  exit 2
}

try {
  $data = Get-Content -Raw -LiteralPath $gatePath | ConvertFrom-Json -ErrorAction Stop
} catch {
  Write-Error "Invalid recall gate data: $($_.Exception.Message)"
  exit 2
}

# Record a recurrence. Bump the named trap's times_recurred and its parent gate's, write the
# JSON back, and tell the caller the lock/commit follow-through. This replaces hand-editing the
# protected file; the pre-commit hook still requires STATE.lock to COMMIT the change.
if ($Increment) {
  $foundGate = $null
  $foundTrap = $null
  foreach ($gate in (As-Array $data.gates)) {
    foreach ($trap in (As-Array $gate.traps)) {
      if ($trap.id -eq $Increment) { $foundGate = $gate; $foundTrap = $trap; break }
    }
    if ($foundTrap) { break }
  }
  if (-not $foundTrap) {
    Write-Error "Unknown trap id: $Increment (no gate.traps[].id matches)"
    exit 2
  }
  $foundTrap.times_recurred = [int]$foundTrap.times_recurred + 1
  $foundGate.times_recurred = [int]$foundGate.times_recurred + 1
  $data.updated = (Get-Date).ToString('yyyy-MM-dd')
  [IO.File]::WriteAllText($gatePath, (($data | ConvertTo-Json -Depth 12) + "`n"), $Utf8NoBom)
  Write-Output ("INCREMENTED trap {0} -> {1}x (gate {2} -> {3}x)" -f $foundTrap.id, $foundTrap.times_recurred, $foundGate.id, $foundGate.times_recurred)
  Write-Output "NEXT: commit protocol/recall-gates.json under STATE.lock (pre-commit enforces it) and record the evidence in DECISIONS.md or a handoff - a count without evidence is just another note."
  exit 0
}

$objectiveText = Text $Objective
$objectiveLower = $objectiveText.ToLowerInvariant()
$matched = @()

foreach ($gate in (As-Array $data.gates)) {
  if ($GateId) {
    if ($gate.id -ne $GateId) { continue }
    $matchedTriggers = @('explicit-gate-id')
  } else {
    # Triggers and aliases are matched identically. Aliases live in the gate data so
    # maintainers grow synonym coverage in JSON, not by editing this script.
    $terms = @(As-Array $gate.triggers) + @(As-Array $gate.aliases)
    $matchedTriggers = @(
      $terms | Where-Object { Term-Hits $objectiveLower (Text $_) }
    )
    if ($matchedTriggers.Count -eq 0) { continue }
  }

  $matched += [pscustomobject]@{
    id = $gate.id
    label = $gate.label
    times_recurred = $gate.times_recurred
    matched_triggers = @($matchedTriggers)
    required_reads = @(As-Array $gate.required_reads)
    traps = @(As-Array $gate.traps)
    stop_signs = @(As-Array $gate.stop_signs)
  }
}

if ($GateId -and $matched.Count -eq 0) {
  Write-Error "Unknown recall gate id: $GateId"
  exit 2
}

$status = if ($matched.Count -gt 0) { 'MATCHED' } else { 'CLEAR' }
$result = [pscustomobject]@{
  status = $status
  objective = $objectiveText
  matched_count = $matched.Count
  gates = @($matched)
}

if ($Json) {
  $result | ConvertTo-Json -Depth 12
  exit 0
}

if ($Quiet) { exit 0 }

# Tool-time banner. Compact by design (no boot-style dump that gets skimmed): one worst-recurrence
# headline + one wrong/right line per trap + the resume command. Emitted by the fragile domain
# script itself, so it is the first thing the model sees at the moment of action.
if ($Header) {
  foreach ($gate in $matched) {
    $worst = 0
    foreach ($t in $gate.traps) { if ([int]$t.times_recurred -gt $worst) { $worst = [int]$t.times_recurred } }
    Write-Output ("##### KNOWN TRAPS [{0}] - gate recurred {1}x, worst trap {2}x. Read before acting. #####" -f $gate.id, $gate.times_recurred, $worst)
    foreach ($trap in $gate.traps) {
      Write-Output ("- [{0}] {1}x  WRONG: {2}  RIGHT: {3}" -f $trap.id, $trap.times_recurred, (Text $trap.wrong_move), (Text $trap.right_move))
    }
    Write-Output ("  reads: {0}" -f (($gate.required_reads | ForEach-Object { Text $_ }) -join '; '))
    Write-Output ("  full: pwsh -NoProfile -File bin/brain-recall.ps1 -GateId {0} -Objective '(tool)'" -f $gate.id)
    Write-Output ("##### END KNOWN TRAPS [{0}] #####" -f $gate.id)
  }
  exit 0
}

Write-Output "RECALL_STATUS=$status"
Write-Output "OBJECTIVE=$objectiveText"
if ($matched.Count -eq 0) {
  # CLEAR is not "all clear." It is a prompt to rule out a reworded recurring task before
  # acting - the blind session's objective often will not contain trap-language by construction.
  Write-Output "NOTE=No recall gate matched. CLEAR is not a green light: confirm this work is genuinely novel, not a reworded recurring/stuck task. If it touches a known-fragile domain or the operator called it recurring, rephrase with the domain words and run again, or read the gate it should have hit."
  exit 0
}

Write-Output ("MATCHED_GATES=" + (($matched | ForEach-Object { $_.id }) -join ', '))
Write-Output "ACTION_REQUIRED=Read required sources before acting; state the matched traps and how they change the next move."

foreach ($gate in $matched) {
  Write-Output ""
  Write-Output ("== {0}: {1} ==" -f $gate.id, $gate.label)
  Write-Output ("RECURRED={0}" -f $gate.times_recurred)
  Write-Output ("Matched triggers: {0}" -f (($gate.matched_triggers | ForEach-Object { Text $_ }) -join ', '))
  Write-Output "Required reads:"
  foreach ($read in $gate.required_reads) {
    Write-Output ("- {0}" -f $read)
  }
  Write-Output "Known traps:"
  foreach ($trap in $gate.traps) {
    Write-Output ("- [{0}] RECURRED {1}x" -f $trap.id, $trap.times_recurred)
    if ($trap.symptom) { Write-Output ("  symptom: {0}" -f $trap.symptom) }
    if ($trap.wrong_move) { Write-Output ("  wrong: {0}" -f $trap.wrong_move) }
    if ($trap.right_move) { Write-Output ("  right: {0}" -f $trap.right_move) }
  }
  Write-Output "Stop signs:"
  foreach ($sign in $gate.stop_signs) {
    Write-Output ("- {0}" -f $sign)
  }
}

exit 0
