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

  # Complete prompts travel on stdin, avoiding operating-system command-line limits.
  [switch]$FromStdin,
  [switch]$Brief,
  [switch]$CheckGates,
  # Validate a staged/example document without redirecting any matching or write action.
  [string]$GatesPath,
  [string]$Ruling,

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

$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$gatePath = Join-Path $root 'protocol/recall-gates.json'
$Utf8NoBom = New-Object System.Text.UTF8Encoding($false)

function Stop-Recall([string]$Reason) {
  [Console]::Error.WriteLine("RECALL_STATUS=BLOCKED reason=$Reason")
  exit 2
}
if ($PSBoundParameters.ContainsKey('GatesPath')) {
  $otherActions = @('Objective', 'FromStdin', 'Brief', 'Ruling', 'Json', 'Quiet', 'GateId', 'Header', 'Increment')
  if (-not $CheckGates -or [string]::IsNullOrWhiteSpace($GatesPath) -or @($otherActions | Where-Object { $PSBoundParameters.ContainsKey($_) }).Count) {
    Stop-Recall 'gates-path-requires-check-only'
  }
  $gatePath = $GatesPath
}

# Read one character beyond the budget to detect overflow; never silently match a prefix.
# Decode stdin as UTF-8 explicitly: on Windows [Console]::In uses the OEM code page, which
# would turn a host's UTF-8 non-ASCII trigger into mojibake and silently report CLEAR.
if ($FromStdin) {
  $buffer = [char[]]::new(131073)
  $stdin = [IO.StreamReader]::new([Console]::OpenStandardInput(), $Utf8NoBom, $true)
  $length = $stdin.ReadBlock($buffer, 0, $buffer.Length)
  if ($length -gt 131072) { Stop-Recall 'objective-too-large (maximum 131072 characters)' }
  $Objective = [string]::new($buffer, 0, $length)
}
if ($Objective.Length -gt 131072) { Stop-Recall 'objective-too-large (maximum 131072 characters)' }
if (-not $Increment -and -not $GateId -and -not $Ruling -and -not $CheckGates -and [string]::IsNullOrWhiteSpace($Objective)) {
  Stop-Recall 'Provide -Objective, -FromStdin, -GateId, -Ruling, -CheckGates or -Increment.'
}

function As-Array($value) {
  if ($null -eq $value) { return @() }
  if ($value -is [System.Array]) { return @($value) }
  return @($value)
}

function Text([string]$value) {
  if ($null -eq $value) { return '' }
  return [string]$value
}

# The literal prefilter avoids regex work for absent terms. The same hyphen-aware boundary
# applies to phrases and tokens: "post-launch analysis" must not match "launch analysis".
function Term-Hits([string]$objectiveLower, [string]$term) {
  $t = (Text $term).Trim().ToLowerInvariant()
  if (-not $t -or -not $objectiveLower.Contains($t)) { return $false }
  return [System.Text.RegularExpressions.Regex]::IsMatch(
    $objectiveLower, ('(?<![\w-])' + [System.Text.RegularExpressions.Regex]::Escape($t) + '(?![\w-])'))
}

# Validate known v1 fields before matching or mutation. Unknown fields remain available for
# adopter extensions; malformed arrays/counts must never silently produce a healthy CLEAR.
function Require-Object($Value, [string]$At) {
  if ($null -eq $Value -or $Value.GetType() -ne [System.Management.Automation.PSCustomObject]) { throw "$At must be an object" }
}
function Require-String($Value, [string]$At) {
  if ($Value -isnot [string] -or [string]::IsNullOrWhiteSpace($Value)) { throw "$At must be a nonempty string" }
}
function Require-Strings($Value, [string]$At) {
  if ($Value -isnot [array]) { throw "$At must be an array of strings" }
  foreach ($item in $Value) { Require-String $item $At }
}
function Require-Count($Value, [string]$At) {
  if (($Value -isnot [int] -and $Value -isnot [long]) -or $Value -lt 0 -or $Value -gt [int]::MaxValue) {
    throw "$At must be a nonnegative Int32 integer"
  }
}
function Require-Id($Value, [string]$At) {
  Require-String $Value $At
  if ($Value -cnotmatch '^[A-Za-z0-9][A-Za-z0-9_-]{0,79}$') { throw "$At must be a safe identifier of at most 80 characters" }
}
function Validate-Gates($Data) {
  Require-Object $Data 'root'
  if ($Data.version -isnot [long] -and $Data.version -isnot [int]) { throw 'version must be integer 1' }
  if ($Data.version -ne 1) { throw 'unsupported version (expected 1)' }
  if ($Data.gates -isnot [array] -or $Data.gates.Count -gt 128) { throw 'gates must be an array of at most 128 gates' }
  $gateIds = @{}; $trapIds = @{}
  foreach ($gate in $Data.gates) {
    Require-Object $gate 'gate'
    Require-Id $gate.id 'gate.id'
    if ($gateIds.ContainsKey($gate.id)) { throw "duplicate gate id $($gate.id)" }
    $gateIds[$gate.id] = $true
    Require-String $gate.label 'gate.label'
    Require-Count $gate.times_recurred 'gate.times_recurred'
    foreach ($field in @('triggers', 'required_reads', 'stop_signs')) { Require-Strings $gate.$field "gate.$field" }
    foreach ($field in @('aliases', 'rulings')) {
      if ($gate.PSObject.Properties[$field]) { Require-Strings $gate.$field "gate.$field" }
    }
    foreach ($id in @($gate.rulings)) {
      if ($null -ne $id -and $id -cnotmatch '^D[1-9][0-9]*$') { throw 'gate.rulings must contain decision IDs such as D1' }
    }
    if ($gate.PSObject.Properties['context_rules']) {
      if ($gate.context_rules -isnot [array]) { throw 'gate.context_rules must be an array' }
      foreach ($rule in $gate.context_rules) {
        Require-Object $rule 'context rule'
        Require-Strings $rule.any 'context rule.any'; Require-Strings $rule.with 'context rule.with'
        if (-not $rule.any.Count -or -not $rule.with.Count) { throw 'context rule sides must not be empty' }
      }
    }
    if ($gate.traps -isnot [array]) { throw 'gate.traps must be an array' }
    foreach ($trap in $gate.traps) {
      Require-Object $trap 'trap'
      Require-Id $trap.id 'trap.id'
      if ($trapIds.ContainsKey($trap.id)) { throw "duplicate trap id $($trap.id)" }
      $trapIds[$trap.id] = $true
      Require-Count $trap.times_recurred 'trap.times_recurred'
      foreach ($field in @('symptom', 'wrong_move', 'right_move')) { Require-String $trap.$field "trap.$field" }
      foreach ($field in @('evidence_refs', 'triggers')) {
        if ($trap.PSObject.Properties[$field]) { Require-Strings $trap.$field "trap.$field" }
      }
    }
  }
}

if (-not (Test-Path -LiteralPath $gatePath)) {
  Stop-Recall 'missing-gate-data (run bin/bootstrap-clone.ps1)'
}

try {
  if ((Get-Item -LiteralPath $gatePath).Length -gt 2097152) { throw 'gate data exceeds 2 MiB' }
  $data = Get-Content -Raw -LiteralPath $gatePath | ConvertFrom-Json -NoEnumerate -ErrorAction Stop
  Validate-Gates $data
} catch {
  # Parse errors can contain raw document contents. Emit a stable code, not a data echo.
  Stop-Recall 'invalid-gate-data (check v1 field types, unique IDs, arrays and nonnegative counts)'
}
if ($CheckGates) { Write-Output "RECALL_GATES_STATUS=VALID gates=$($data.gates.Count)"; exit 0 }

# A generated ruling index is optional until a gate cites one or a lookup is requested.
# Once referenced, a missing/invalid/unknown ruling is a visible failure, never a silent drop.
$rulingsById = @{}
$hasCitations = @($data.gates | Where-Object { @($_.rulings).Count -gt 0 -and $null -ne $_.rulings }).Count -gt 0
if ($Ruling -or $hasCitations) {
  try {
    $rulingsDoc = Get-Content -Raw -LiteralPath (Join-Path $root 'protocol/rulings.json') | ConvertFrom-Json -NoEnumerate -ErrorAction Stop
    Require-Object $rulingsDoc 'rulings root'
    Require-Object $rulingsDoc.sources 'rulings sources'
    $sourcePaths = @{ decisions = 'DECISIONS.md'; ratifications = 'protocol/decision-ratifications.jsonl'; gates = 'protocol/recall-gates.json' }
    foreach ($key in $sourcePaths.Keys) {
      $path = Join-Path $root $sourcePaths[$key]
      $source = if (Test-Path -LiteralPath $path -PathType Leaf) { [IO.File]::ReadAllText($path) } else { '' }
      $bytes = $Utf8NoBom.GetBytes(($source -replace "`r`n", "`n").TrimEnd())
      $sha = [Security.Cryptography.SHA256]::Create()
      try { $digest = [BitConverter]::ToString($sha.ComputeHash($bytes)).Replace('-', '').ToLowerInvariant() }
      finally { $sha.Dispose() }
      if ($rulingsDoc.sources.$key -cne $digest) { throw 'stale ruling sources' }
    }
    if ($rulingsDoc.rulings -isnot [array]) { throw 'rulings must be an array' }
    foreach ($r in $rulingsDoc.rulings) {
      Require-Object $r 'ruling'
      if ($r.id -cnotmatch '^D[1-9][0-9]*$' -or $rulingsById.ContainsKey($r.id)) { throw 'invalid/duplicate ruling id' }
      Require-String $r.rule 'ruling.rule'
      if ($r.class -notin @('operator-ruled', 'operator-ratified')) { throw 'unsupported ruling class' }
      $rulingsById[$r.id] = $r
    }
    foreach ($gate in $data.gates) {
      foreach ($id in @(As-Array $gate.rulings)) {
        if (-not $rulingsById.ContainsKey($id)) { throw 'unknown cited ruling' }
      }
    }
  } catch { Stop-Recall 'invalid-rulings (run bin/build-rulings.ps1 -Check and repair before acting)' }
}
function Ruling-Line($Value) { '{0} [{1}] {2}' -f $Value.id, $Value.class, $Value.rule }
if ($Ruling) {
  $query = $Ruling.Trim()
  $hits = if ($query -match '^D?\d+$') {
    @($rulingsById['D' + $query.TrimStart('D', 'd')] | Where-Object { $_ })
  } else {
    @($rulingsById.Values | Where-Object { $_.rule.IndexOf($query, [StringComparison]::OrdinalIgnoreCase) -ge 0 } | Sort-Object id)
  }
  if (-not $hits.Count) { Write-Output 'RULING_STATUS=NONE (no live ruling matched)'; exit 1 }
  Write-Output "RULING_STATUS=FOUND count=$($hits.Count)"
  foreach ($r in $hits) {
    Write-Output (Ruling-Line $r)
    if ($r.rules_out) { Write-Output "  rules out: $($r.rules_out)" }
  }
  exit 0
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
    Stop-Recall 'unknown-trap-id'
  }
  if ($foundTrap.times_recurred -eq [int]::MaxValue -or $foundGate.times_recurred -eq [int]::MaxValue) { Stop-Recall 'recurrence-count-overflow' }
  $foundTrap.times_recurred = [int]$foundTrap.times_recurred + 1
  $foundGate.times_recurred = [int]$foundGate.times_recurred + 1
  # Keep an existing `updated` in place (Add-Member -Force would move it to the end) and write
  # at full depth so adopter extension fields are never flattened into strings.
  $today = (Get-Date).ToString('yyyy-MM-dd')
  if ($data.PSObject.Properties['updated']) { $data.updated = $today }
  else { $data | Add-Member -NotePropertyName updated -NotePropertyValue $today }
  [IO.File]::WriteAllText($gatePath, (($data | ConvertTo-Json -Depth 100) + "`n"), $Utf8NoBom)
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
    foreach ($rule in @(As-Array $gate.context_rules)) {
      $left = @($rule.any | Where-Object { Term-Hits $objectiveLower $_ })
      $right = @($rule.with | Where-Object { Term-Hits $objectiveLower $_ })
      if ($left.Count -and $right.Count) { $matchedTriggers += ('context:' + $left[0] + ' + ' + $right[0]) }
    }
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
    rulings = @(As-Array $gate.rulings | ForEach-Object { $rulingsById[$_] })
  }
}

if ($GateId -and $matched.Count -eq 0) {
  Stop-Recall 'unknown-gate-id'
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

if ($Brief) {
  # Healthy CLEAR emits nothing. Reserve every matched gate and the full-read instruction
  # before optional detail; a substring cut must never erase later gates or the recovery path.
  if ($matched.Count -eq 0) { exit 0 }
  $lines = @("RECALL_STATUS=MATCHED gates=$($matched.Count) - read before acting")
  foreach ($gate in $matched) {
    $line = '- {0} {1}x ({2} traps, {3} reads)' -f $gate.id, $gate.times_recurred, $gate.traps.Count, $gate.required_reads.Count
    if ($gate.rulings.Count) { $line += '; rulings: ' + (($gate.rulings | ForEach-Object { $_.id }) -join ',') }
    $lines += $line
  }
  $lines += 'For EACH gate ID: pwsh -NoProfile -File bin/brain-recall.ps1 -GateId <ID>'
  $lines += 'NEEDS_READ=Full output has all traps, rulings and required reads; this reminder is not write authorization.'
  $banner = $lines -join "`n"
  if ($banner.Length -gt 1400) { Stop-Recall 'compact-budget-exceeded (use full recall; no matched gate was silently omitted)' }
  foreach ($gate in $matched) {
    foreach ($trap in $gate.traps) {
      $detail = '  {0}/{1} {2}x' -f $gate.id, $trap.id, $trap.times_recurred
      if ($banner.Length + $detail.Length + 1 -le 1400) { $banner += "`n$detail" }
    }
  }
  Write-Output $banner
  exit 0
}

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
    foreach ($r in $gate.rulings) { Write-Output ('  ruling: ' + (Ruling-Line $r)) }
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
  foreach ($r in $gate.rulings) { Write-Output ('Ruling in force: ' + (Ruling-Line $r)) }
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
