#requires -Version 7.0
<#
.SYNOPSIS
  Health check for an mdr-kernel brain. Runs at every session start (via bin/start-session.ps1).
  Advisory: warns on drift; records non-compliance only for real hazards (a committed secret, or
  broken canonical startup wiring). Designed to pass on a cold clone AFTER bin/bootstrap-clone.ps1
  has scaffolded the create-your-own files. If a check warns about a missing file, bootstrap
  creates it.

  This is the PUBLIC doctor. The private studio it was extracted from carries extra checks
  (production freshness, doctrine drift, local-model digest pins, a read-only-surface command
  registry). Those are studio-specific — add your own as your system grows. Keep the core:
  a fresh clone should reach a clean doctor, or tell you exactly which one command fixes it.

  -HardOnly: run only the cheap tree checks plus the hazard gates that can actually fail
  (git working tree, canonical startup wiring, tracked-secret scan). close-session uses this —
  it only consumes the exit code; the full advisory sweep belongs to session START.
#>
[CmdletBinding()]
param(
  [int]$StaleDays = 14,
  [int]$StateMaxLines = 90,
  [int]$StateMaxKB = 10,
  [int]$NodeMapStaleDays = 7, # retained for CLI compatibility; exact parity supersedes age
  [switch]$HardOnly
)

$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$fail = 0
function Ok  ([string]$m) { Write-Host "[ ok ] $m" }
function Warn([string]$m) { Write-Host "[warn] $m" }
function Fail([string]$m) { Write-Host "[non-compliance] $m"; $script:fail++ }

if ($HardOnly) { Write-Host "mdr-kernel doctor - $root (HardOnly: hazard gates only)`n" }
else { Write-Host "mdr-kernel doctor - $root`n" }
if (-not (git rev-parse --is-inside-work-tree 2>$null)) {
  Fail 'this folder is not a git working tree - use git clone (preferred) or git init before running bootstrap/start-session'
  Write-Host ''
  Write-Host "doctor: $fail non-compliance event(s)."
  Write-Host '[ALERT]: MDR floor non-compliance detected. Correction is required before continuing.' -ForegroundColor Red
  exit 1
}

# 1) Canonical root (warning only). A linked git worktree of the canonical repo is canonical: it
# shares the canonical .git, so its commits land in the same store. We tell a worktree of canonical
# apart from a foreign clone by resolving dirname(--git-common-dir).
$marker = Join-Path $root '.brain-root'
$top = (git rev-parse --show-toplevel 2>$null)
function Norm([string]$s) { ($s -replace '\\','/').TrimEnd('/').ToLower() }
if (Test-Path $marker) {
  $want = Norm ((Get-Content -Raw $marker).Trim())
  $got  = Norm $top
  if ($want -eq $got) { Ok "canonical root ($top)" }
  else {
    $commonDir = (git rev-parse --path-format=absolute --git-common-dir 2>$null)
    if (-not $commonDir) { $commonDir = (git rev-parse --git-common-dir 2>$null) }
    $canonTop = if ($commonDir) { Norm (Split-Path -Parent $commonDir) } else { $null }
    if ($canonTop -and $canonTop -eq $want) { Ok "canonical root - linked worktree" }
    else { Warn "not in canonical root - here: $top ; canonical: $((Get-Content -Raw $marker).Trim()). You may be in a mirror clone; avoid writing." }
  }
} else { Warn ".brain-root marker missing - run bin/bootstrap-clone.ps1" }

# 2) Working tree
# Untracked files are not a hazard by design: close-session intentionally leaves stray untracked
# paths in the tree (another lane's scratch output, a partial -Also match), and start-session's
# pull --rebase gate uses the same tracked-only definition of "dirty". Only uncommitted changes to
# tracked files block a clean report here.
$dirty = git status --porcelain --untracked-files=no
if ([string]::IsNullOrWhiteSpace($dirty)) { Ok "working tree clean (tracked files)" }
else { Warn "uncommitted changes to tracked files present (commit STATE/DECISIONS/handoff before stopping)" }

# 3) STATE staleness
$ct = git log -1 --format=%ct -- STATE.md 2>$null
if ($ct) {
  $age = [math]::Floor(([DateTimeOffset]::UtcNow.ToUnixTimeSeconds() - [int64]$ct) / 86400)
  if ($age -le $StaleDays) { Ok "STATE.md updated ${age}d ago" }
  else { Warn "STATE.md not touched in ${age}d (> $StaleDays) - is it still current?" }
} else { Warn "could not read STATE.md git history (new repo? commit your first STATE.md)" }

# 3b) STATE budget - STATE is read at every session start; keep the context tax low.
# Byte-exact against MaxKB*1024 so this verdict and bin/state-budget.ps1 always agree.
$sp = Join-Path $root 'STATE.md'
if (Test-Path $sp) {
  $stateLines = @(Get-Content $sp).Count
  $stateBytes = (Get-Item $sp).Length
  $stateMaxBytes = $StateMaxKB * 1024
  if ($stateLines -le $StateMaxLines -and $stateBytes -le $stateMaxBytes) {
    Ok "STATE.md within budget ($stateLines/$StateMaxLines lines, $stateBytes/$stateMaxBytes bytes; headroom $($stateMaxBytes - $stateBytes) bytes / $($StateMaxLines - $stateLines) lines)"
  } else {
    Warn "STATE.md over budget ($stateLines/$StateMaxLines lines, $stateBytes/$stateMaxBytes bytes) - run bin/state-budget.ps1 for the per-section/fattest-line breakdown; drop completed items into handoffs"
  }
} else { Warn "STATE.md missing - first run: bin/setup-private.ps1 -Model '<your model id>'; repairing an already-set-up clone: bin/bootstrap-clone.ps1" }

# 4) Hooks armed for this clone. Git does not version-control core.hooksPath, so a fresh clone
# can silently skip the safety gates.
$hooksPath = (git config --get core.hooksPath 2>$null)
if ($hooksPath -eq '.githooks') { Ok "git hooks armed (core.hooksPath=.githooks)" }
elseif ($hooksPath) { Warn "core.hooksPath is '$hooksPath', expected .githooks - run bin/bootstrap-clone.ps1" }
else { Warn "core.hooksPath not set - run: git config core.hooksPath .githooks (or bin/bootstrap-clone.ps1)" }

# 4a) Remote privacy: verify live visibility and the approved destination pin.
$privacyScript = Join-Path $root 'bin/remote-privacy.ps1'
if (-not (Test-Path -LiteralPath $privacyScript)) { Warn 'remote privacy verifier missing - restore bin/remote-privacy.ps1' }
else {
  $privacyOut = @(& pwsh -NoProfile -File $privacyScript -RequireApproved)
  $privacyStatus = (($privacyOut | Where-Object { $_ -like 'REMOTE_PRIVACY_STATUS=*' } | Select-Object -First 1) -replace '^REMOTE_PRIVACY_STATUS=', '')
  $privacyMessage = (($privacyOut | Where-Object { $_ -like 'REMOTE_PRIVACY_MESSAGE=*' } | Select-Object -First 1) -replace '^REMOTE_PRIVACY_MESSAGE=', '')
  if ($privacyStatus -in @('PRIVATE','LOCAL_ONLY')) { Ok "remote privacy: $privacyMessage" }
  elseif ($privacyStatus -eq 'MISSING_ORIGIN') {
    # Hazard, not drift: a private-remote memory without origin silently stops backing up while
    # looking local-only. Hard-fail so startup reports BLOCKED and close refuses until repaired.
    Fail "remote privacy: $privacyMessage"
  }
  else { Warn "remote privacy NOT READY ($privacyStatus): $privacyMessage" }
}

# 4aa) A deliberately offline close must be impossible to forget. The config marker gives an
# exact recovery command; the graph-derived count catches pending commits even if that marker was
# removed. Neither one authorizes a push — sync-private re-verifies privacy live.
$pendingSync = (git config --local --get brain.pendingSync 2>$null)
$aheadCount = $null
if (git show-ref --verify --quiet refs/remotes/origin/main) {
  $aheadRaw = (& git rev-list --count origin/main..HEAD 2>$null | Out-String).Trim()
  if ($aheadRaw -match '^\d+$') { $aheadCount = [int]$aheadRaw }
}
if ($pendingSync -eq 'true' -or ($null -ne $aheadCount -and $aheadCount -gt 0)) {
  $countText = if ($null -ne $aheadCount) { "$aheadCount local commit(s) ahead of origin/main" } else { 'local checkpoint pending' }
  Warn "PENDING PRIVATE SYNC: $countText. Run: pwsh -NoProfile -File bin/sync-private.ps1"
} elseif ($null -ne $aheadCount) { Ok 'private sync: no local commits pending' }

# 4b) Brain-Model identity. A process-scoped value is authoritative; a shared clone's git config
# brain.model is only a potentially stale fallback.
$idEnv = $env:BRAIN_MODEL
$idCfg = (git config brain.model 2>$null)
if ($idEnv) { Ok "Brain-Model identity = $idEnv (from `$BRAIN_MODEL)" }
elseif ($idCfg) { Warn "Brain-Model fallback = $idCfg (git config brain.model): commits and locks will attribute to '$idCfg' regardless of which model is actually running. Fine for a human at the keyboard; each AI session should pass its own -Model or set `$BRAIN_MODEL." }
else { Warn "Brain-Model identity unset - set `$BRAIN_MODEL for this process before lock/commit." }

# 5) Doors in sync with the contract (advisory - skipped by -HardOnly; pre-commit still blocks
# committing a drifted door, and the full startup doctor re-checks it)
if (-not $HardOnly) {
$sync = Join-Path $root 'bin/sync-doors.ps1'
if (Test-Path $sync) {
  $syncOut = & pwsh -NoProfile -File $sync -Check 2>&1
  if ($LASTEXITCODE -eq 0) { Ok "front doors in sync with protocol/CONTRACT.md" }
  else {
    $missingDoors = @('CLAUDE.md','AGENTS.md','GEMINI.md','protocol/advisory.md') | Where-Object {
      -not (Test-Path (Join-Path $root $_))
    }
    if ($missingDoors.Count -gt 0) {
      Warn "front doors not generated yet ($($missingDoors -join ', ')) - first run: bin/setup-private.ps1; repair: bin/sync-doors.ps1"
    } elseif (-not (Test-Path (Join-Path $root 'STATE.md'))) {
      Warn "phase-zero front doors present (setup has not run) - run bin/setup-private.ps1 -Model '<your model id>' to generate the full contract doors"
    } else {
      Warn "front doors drifted from CONTRACT - run: pwsh -NoProfile -File bin/sync-doors.ps1"
    }
  }
  foreach ($line in $syncOut) { if ($line -match '^WARN:') { Warn $line } }
} else { Warn "bin/sync-doors.ps1 missing" }
}

# 5b) Canonical startup wiring. Every full participant uses one executable sequence. Missing the
# core scripts is a hard block; the per-tool SessionStart hook adapters (.claude/, .codex/) are
# OPTIONAL in the template (add them for your tools), so their absence only warns.
$startPs = Join-Path $root 'bin/start-session.ps1'
$sessionStop = Join-Path $root 'bin/session-stop.ps1'
$contractPath = Join-Path $root 'protocol/CONTRACT.md'
$startupProblems = @()
if (-not (Test-Path $startPs)) { $startupProblems += 'bin/start-session.ps1 missing' }
if (-not (Test-Path $sessionStop)) { $startupProblems += 'bin/session-stop.ps1 missing' }
if (Test-Path $contractPath) {
  $contractText = Get-Content -Raw $contractPath
  if ($contractText -notmatch 'bin/start-session\.ps1') { $startupProblems += 'CONTRACT does not route startup through bin/start-session.ps1' }
} else { $startupProblems += 'protocol/CONTRACT.md missing' }
if ($startupProblems.Count -eq 0) { Ok 'canonical startup wiring present (start-session + session-stop + contract)' }
else { Fail "canonical startup wiring broken: $($startupProblems -join '; ')" }

$claudeSettings = Join-Path $root '.claude/settings.json'
$codexHooks = Join-Path $root '.codex/hooks.json'
if ((Test-Path $claudeSettings) -or (Test-Path $codexHooks)) { Ok 'per-tool SessionStart hook adapter(s) present' }
else { Warn 'no .claude/ or .codex/ SessionStart adapter - fine for CLI-only use; add one to auto-run startup for your tool' }

if (-not $HardOnly) {  # ---- advisory region (recall wiring, node map, indexes), skipped by -HardOnly ----
# 5c) Active recall gate. Storage guardrails are not enough: recurring/stuck work must surface
# the prior traps at objective time. Requires the script + gates data + the reference doc.
$recallScript = Join-Path $root 'bin/brain-recall.ps1'
$recallData = Join-Path $root 'protocol/recall-gates.json'
$recallProblems = @()
if (-not (Test-Path $recallScript)) { $recallProblems += 'bin/brain-recall.ps1 missing' }
if (-not (Test-Path $recallData)) { $recallProblems += 'protocol/recall-gates.json missing (copy protocol/recall-gates.example.json, or run bin/bootstrap-clone.ps1)' }
if (Test-Path $contractPath) {
  if ((Get-Content -Raw $contractPath) -notmatch 'bin/brain-recall\.ps1') { $recallProblems += 'CONTRACT does not require bin/brain-recall.ps1' }
}
if ($recallProblems.Count -eq 0) { Ok 'active recall gate present (script + gates + contract wiring)' }
else { Warn "active recall gate incomplete: $($recallProblems -join '; ')" }

# 5d) Node map retrieval layer. This is the compact routing database agents use with STATE.md to
# choose a narrow reading slice before broad search.
$nodeScript = Join-Path $root 'bin/update-node-map.ps1'
$nodeRoots = Join-Path $root 'maps/node-roots.json'
$nodeJson = Join-Path $root 'maps/node-map.json'
$nodeMarkdown = Join-Path $root 'maps/NODE-MAP.md'
$nodeProblems = @()
if (-not (Test-Path $nodeScript)) { $nodeProblems += 'bin/update-node-map.ps1 missing' }
if (-not (Test-Path $nodeRoots)) { $nodeProblems += 'maps/node-roots.json missing' }
if (Test-Path $contractPath) {
  $contractText = Get-Content -Raw $contractPath
  if ($contractText -notmatch 'maps/NODE-MAP\.md' -and $contractText -notmatch 'maps/node-map\.json') {
    $nodeProblems += 'CONTRACT does not route startup through the node map'
  }
}
if ($nodeProblems.Count) {
  Warn "node map subsystem incomplete: $($nodeProblems -join '; ')"
} elseif (-not (Test-Path $nodeJson) -or -not (Test-Path $nodeMarkdown)) {
  Warn 'node map missing - run: pwsh -NoProfile -File bin/update-node-map.ps1'
} else {
  try {
    $nodeData = Get-Content -LiteralPath $nodeJson -Raw | ConvertFrom-Json
    $nodeIssues = @($nodeData.files | Where-Object { $_.issues.Count -gt 0 })
    $nodeTotal = @($nodeData.files).Count
    $nodeCheckOutput = @(& pwsh -NoProfile -File $nodeScript -Check 2>&1)
    if ($LASTEXITCODE -ne 0) {
      Warn 'node map differs from tracked Markdown or generated bridge output - run: pwsh -NoProfile -File bin/update-node-map.ps1'
    } else {
      Ok "node map exact parity ($nodeTotal tracked files; $($nodeIssues.Count) file(s) with active issues)"
    }
  } catch {
    Warn "node map unreadable - regenerate with: pwsh -NoProfile -File bin/update-node-map.ps1"
  }
}

# 6) Indexes match directories - every .md has a row, and every row names a real file.
function Check-Index([string]$dir, [string]$index) {
  $idxPath = Join-Path $root $index
  $dirPath = Join-Path $root $dir
  if (-not (Test-Path $dirPath)) { return }  # optional dir; skip quietly if you don't use it
  if (-not (Test-Path $idxPath)) { Warn "$index missing"; return }
  $listed = Get-Content -Raw $idxPath
  $missing = @()
  Get-ChildItem $dirPath -Filter *.md -File |
    Where-Object { $_.Name -ne 'INDEX.md' } |
    ForEach-Object { if ($listed -notmatch [regex]::Escape($_.Name)) { $missing += $_.Name } }
  if ($missing.Count -eq 0) { Ok "$index lists every file in $dir/" }
  else { Warn "$index missing rows for: $($missing -join ', ')" }
  $stale = @()
  foreach ($line in (Get-Content $idxPath)) {
    if ($line -notmatch '^\s*\|') { continue }
    $cells = ($line.Trim() -replace '^\||\|$', '') -split '\|'
    if ($cells.Count -lt 2) { continue }
    $name = $cells[1].Trim()
    if ($name -notmatch '^[\w.-]+\.md$' -or $name -eq 'INDEX.md') { continue }
    if (-not (Test-Path (Join-Path $root "$dir/$name")) -and
        -not (Test-Path (Join-Path $root "$dir/archive/$name"))) { $stale += $name; continue }
    # Resolve against TRACKED files too: an untracked-on-disk target means the pushed repo is
    # broken for fresh clones even though this clone looks healthy.
    if (@(& git ls-files -- "$dir/$name" "$dir/archive/$name").Count -eq 0) { $stale += "$name (untracked)" }
  }
  if ($stale.Count -eq 0) { Ok "$index rows all resolve to tracked files" }
  else { Warn "$index has row(s) not resolving to tracked files: $($stale -join ', ')" }
}
Check-Index 'specs' 'specs/INDEX.md'
Check-Index 'handoffs' 'handoffs/INDEX.md'
Check-Index 'reference' 'reference/INDEX.md'
}  # ---- end advisory region (recall wiring, node map, indexes) ----

# 7) Secret scan over tracked files (hazard -> non-compliance)
$patterns = @(
  'sk-ant-[A-Za-z0-9_-]{20,}', 'sk-(?:proj|svcacct)-[A-Za-z0-9_-]{20,}',
  'sk-[A-Za-z0-9]{20,}',
  'ghp_[A-Za-z0-9]{30,}', 'github_pat_[A-Za-z0-9_]{40,}',
  'AKIA[0-9A-Z]{16}', 'AIza[0-9A-Za-z_-]{30,}',
  'xox[baprs]-[A-Za-z0-9-]{10,}', '-----BEGIN [A-Z ]*PRIVATE KEY-----',
  '(?i)[\w-]*(api[_-]?key|secret|passwd|password|access[_-]?token)\b\s*[:=]\s*["''][^"'']{8,}["'']'
) -join '|'
$skip = @('.githooks/pre-commit', 'bin/brain-doctor.ps1')
$secretHits = @()
foreach ($f in (git ls-files)) {
  if ($skip -contains $f) { continue }
  $fp = Join-Path $root $f
  if (-not (Test-Path $fp)) { continue }
  $lineNo = 0
  foreach ($line in (Get-Content -LiteralPath $fp -ErrorAction SilentlyContinue)) {
    $lineNo++
    if ($line -match 'secret-audit:allow') { continue }
    if ($line -match $patterns) { $secretHits += "${f}:${lineNo}" }
  }
}
if ($secretHits.Count -eq 0) { Ok "no secret-shaped strings in tracked files" }
else { Fail "possible secret(s) in: $($secretHits -join ', ')" }

if (-not $HardOnly) {  # ---- advisory region (links/reference/inbox/coordination/lock), skipped by -HardOnly ----
# 8) Wiki-links resolve to reference/ files (warn). Kebab-case slugs only.
$ignore = @('links', 'link', 'wikilinks', 'wiki-link', 'wiki-links', 'slug', 'name', 'their-name')
$seen = @{}; $unresolved = New-Object System.Collections.Generic.List[string]
foreach ($f in (git ls-files '*.md')) {
  $fp = Join-Path $root $f
  if (-not (Test-Path $fp)) { continue }
  foreach ($m in [regex]::Matches((Get-Content -Raw -LiteralPath $fp), '\[\[([a-z0-9][a-z0-9-]*)\]\]')) {
    $slug = $m.Groups[1].Value
    if (($ignore -contains $slug) -or $seen.ContainsKey($slug)) { continue }
    $seen[$slug] = $true
    if (-not (Test-Path (Join-Path $root "reference/$slug.md"))) { $unresolved.Add($slug) }
  }
}
if ($unresolved.Count -eq 0) { Ok "all [[wiki-links]] resolve to reference/ files" }
else { Warn "unresolved [[links]] (no reference/<slug>.md): $($unresolved -join ', ')" }

# 9) reference/ index present (optional dir)
if (Test-Path (Join-Path $root 'reference')) {
  if (Test-Path (Join-Path $root 'reference/INDEX.md')) { Ok "reference/ + INDEX present" }
  else { Warn "reference/INDEX.md missing" }
}

# 10) inbox/ - notes waiting to be folded into STATE (optional dir)
$inbox = Join-Path $root 'inbox'
if (Test-Path $inbox) {
  $notes = @(Get-ChildItem $inbox -Recurse -File -ErrorAction SilentlyContinue | Where-Object {
    $_.Name -notin @('README.md', 'write-approval-audit.jsonl')
  })
  if ($notes.Count -eq 0) { Ok "inbox/ empty" }
  else { Warn "inbox/: $($notes.Count) note(s) waiting - fold into STATE and clear" }
}

# 11) coordination-files list sanity: the authoritative single-writer list exists and every
# entry names a real file (catches renames/typos that would silently unprotect).
$coordList = Join-Path $root 'protocol/coordination-files.txt'
if (Test-Path $coordList) {
  $entries = @(Get-Content $coordList | Where-Object { $_.Trim() -and $_ -notmatch '^\s*#' } | ForEach-Object { $_.Trim() })
  $missingCoord = @($entries | Where-Object { -not (Test-Path (Join-Path $root $_)) })
  # Files bin/bootstrap-clone.ps1 scaffolds are expected to be absent on a pristine (pre-bootstrap)
  # clone. If ONLY those are missing, point at bootstrap (consistent with the STATE.md / recall-gates
  # checks above); reserve the stern "fix the list" wording for a genuine list-vs-repo mismatch.
  $bootstrapScaffolded = @('.brain-root','STATE.md','DECISIONS.md','handoffs/INDEX.md','specs/INDEX.md',
    'reference/INDEX.md','inbox/README.md','protocol/recall-gates.json','protocol/advisory.md',
    'CLAUDE.md','AGENTS.md','GEMINI.md')
  $realMissing = @($missingCoord | Where-Object { $bootstrapScaffolded -notcontains $_ })
  if ($missingCoord.Count -eq 0) { Ok "coordination-files.txt: all $($entries.Count) protected paths exist" }
  elseif ($realMissing.Count -eq 0) { Warn "coordination-files.txt lists $($missingCoord.Count) not-yet-scaffolded file(s) - run bin/bootstrap-clone.ps1" }
  else { Warn "coordination-files.txt names missing file(s): $($realMissing -join ', ') - fix the list or restore the file" }
} else { Warn "protocol/coordination-files.txt missing - pre-commit lock gate falls back to a minimal built-in list" }

# 12) STATE.lock - structured coordination lock: holder + heartbeat staleness. Resolve the ONE
# shared lock (dirname(--git-common-dir)) so a worktree reports the same lock the canonical clone
# holds - a per-tree lock would let two sessions co-write coordination files.
$lockCommon = (git -C $root rev-parse --path-format=absolute --git-common-dir 2>$null)
if (-not $lockCommon) { $lockCommon = (git -C $root rev-parse --git-common-dir 2>$null) }
$lock = if ($lockCommon -and (Test-Path -LiteralPath (Split-Path -Parent $lockCommon))) {
  Join-Path (Split-Path -Parent $lockCommon) 'STATE.lock'
} else { Join-Path $root 'STATE.lock' }
if (Test-Path $lock) {
  $raw = (Get-Content -Raw $lock).Trim()
  $l = $null; try { $l = $raw | ConvertFrom-Json } catch {}
  if ($l) {
    $hb = if ($l.heartbeat_at) { $l.heartbeat_at } else { $l.acquired_at }
    $age = $null
    try { $age = [math]::Round(([DateTimeOffset]::UtcNow - [DateTimeOffset]::Parse($hb)).TotalMinutes) } catch {}
    $objective = if ($l.objective) { $l.objective } else { 'MISSING' }
    $who = "$($l.model) on $($l.host) (objective: $objective)"
    if ($null -ne $age -and $age -gt 120) {
      Warn "STATE.lock heartbeat ${age}m old - likely a dead session: $who. Release with bin/state-lock.ps1 -Release -Force"
    } else {
      Warn "STATE.lock held (heartbeat ${age}m ago): $who - coordination files are single-writer; do not edit them unless this is your lock"
    }
  } else {
    Warn "STATE.lock held (unstructured): $raw - single-writer coordination applies"
  }
} else { Ok "STATE.lock free" }

# 12b) Linked worktrees: AI-harness sessions run in .claude/worktrees/* and dead ones linger as
# full repo copies. Quiet when none exist; recent ones are likely live sessions.
$wtMain = Norm $top
foreach ($wtLine in @(git worktree list --porcelain 2>$null | Where-Object { $_ -like 'worktree *' })) {
  $wtPath = $wtLine.Substring(9)
  if ((Norm $wtPath) -eq $wtMain) { continue }
  $idleH = $null
  try { $idleH = [math]::Round(((Get-Date) - (Get-Item -LiteralPath $wtPath).LastWriteTime).TotalHours) } catch {}
  if ($null -ne $idleH -and $idleH -le 48) {
    Ok "linked worktree present (recent - likely a live session): $wtPath"
  } else {
    Warn "linked worktree likely from a dead session: $wtPath (idle ${idleH}h) - verify clean, then: git worktree remove '$wtPath'"
  }
}

# 12c) Rules-with-teeth: prose rules about coordination files drift unless a check holds them.
$stateText = if (Test-Path $sp) { Get-Content -Raw $sp } else { '' }
$requiredSections = @('## Right now', '## Active work', '## Next actions')
$missingSections = @($requiredSections | Where-Object { $stateText -notmatch "(?m)^$([regex]::Escape($_))" })
if ($stateText -and $missingSections.Count -eq 0) { Ok "STATE.md carries its contract sections (Right now / Active work / Next actions)" }
elseif ($stateText) { Warn "STATE.md missing section header(s): $($missingSections -join ', ') - the 30-second scan shape relies on them" }

$decPath = Join-Path $root 'DECISIONS.md'
if (Test-Path $decPath) {
  $decNums = @([regex]::Matches((Get-Content -Raw $decPath), '(?m)^## D(\d+)\b') | ForEach-Object { [int]$_.Groups[1].Value })
  if ($decNums.Count -ge 2) {
    $decMax = ($decNums | Measure-Object -Maximum).Maximum
    if ($decNums[0] -eq $decMax) { Ok "DECISIONS.md newest-on-top holds (top = D$($decNums[0]))" }
    else { Warn "DECISIONS.md newest-on-top violated: top entry is D$($decNums[0]) but newest is D$decMax - new entries go at the TOP" }
  }
}

$idxDates = @(Get-Content (Join-Path $root 'handoffs/INDEX.md') -ErrorAction SilentlyContinue | ForEach-Object { if ($_ -match '^\| (\d{4}-\d{2}-\d{2}) \|') { $Matches[1] } })
if ($idxDates.Count -ge 2) {
  $idxMax = ($idxDates | Measure-Object -Maximum).Maximum
  if ($idxDates[0] -eq $idxMax) { Ok "handoffs/INDEX.md newest-first holds (top row $($idxDates[0]))" }
  else { Warn "handoffs/INDEX.md newest-first violated: top row is $($idxDates[0]) but newest is $idxMax - new rows go at the TOP of the table" }
}
}  # ---- end advisory region (links/reference/inbox/coordination/lock) ----

Write-Host ""
if ($fail -gt 0) {
  Write-Host "doctor: $fail non-compliance event(s)."
  Write-Host '[ALERT]: MDR floor non-compliance detected. Correction is required before continuing.' -ForegroundColor Red
  exit 1
}
Write-Host "doctor: ok."
exit 0
