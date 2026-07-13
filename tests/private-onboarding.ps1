#requires -Version 7.0
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$failures = 0

function Expect-Exit([string]$Name, [int]$Expected, [scriptblock]$Action) {
  $output = @(& $Action 2>&1)
  $actual = $LASTEXITCODE
  if ($actual -eq $Expected) {
    if ($Expected -eq 0) { $output | Out-Host }
    else { $output | ForEach-Object { ([string]$_ -replace '^\[FAIL\]', '[expected refusal]') | Write-Host } }
    Write-Host "[PASS] $Name"
  } else {
    $output | Out-Host
    Write-Host "[FAIL] $Name (expected $Expected, got $actual)"
    $script:failures++
  }
}

$sandbox = Join-Path ([IO.Path]::GetTempPath()) "mdr-private-onboarding-$([guid]::NewGuid().ToString('N'))"
$bareA = Join-Path $sandbox 'private-a.git'
$bareB = Join-Path $sandbox 'private-b.git'
$clone = Join-Path $sandbox 'clone'
$secondClone = Join-Path $sandbox 'second-clone'
$peer = Join-Path $sandbox 'peer'
$localClone = Join-Path $sandbox 'local-only'
try {
  New-Item -ItemType Directory -Path $sandbox | Out-Null
  & git clone -q --bare $root $bareA
  if ($LASTEXITCODE) { throw 'could not create private-a test remote' }
  & git clone -q --bare $root $bareB
  if ($LASTEXITCODE) { throw 'could not create private-b test remote' }
  & git clone -q $bareA $clone
  if ($LASTEXITCODE) { throw 'could not create test clone' }
  & git -C $clone config core.hooksPath .githooks

  # Fresh-agent cold start, phase zero: an agent opened in a never-set-up clone must find static
  # front doors that route it to setup-private (not raw bootstrap), and the canonical startup must
  # block with the same routing rather than assume memory exists.
  $phaseZeroOk = $true
  foreach ($door in 'CLAUDE.md', 'AGENTS.md', 'GEMINI.md') {
    $doorPath = Join-Path $clone $door
    if (-not (Test-Path -LiteralPath $doorPath)) { $phaseZeroOk = $false; continue }
    $doorText = Get-Content -Raw -LiteralPath $doorPath
    if ($doorText -notmatch 'setup-private\.ps1' -or $doorText -match 'BEGIN BRAIN-CONTRACT' -or
        $doorText -notmatch 'phase-zero') { $phaseZeroOk = $false }
  }
  if ($phaseZeroOk) { Write-Host '[PASS] fresh clone ships phase-zero doors routing to setup-private' }
  else { Write-Host '[FAIL] phase-zero doors missing, generated too early, or not routing to setup-private'; $failures++ }

  $coldStartOut = @(& pwsh -NoProfile -File (Join-Path $clone 'bin/start-session.ps1') -Model 'test-model' 2>&1)
  $coldStartExit = $LASTEXITCODE
  $coldStartText = ($coldStartOut | Out-String)
  if ($coldStartExit -eq 1 -and $coldStartText -match 'STARTUP_STATUS=BLOCKED' -and
      $coldStartText -match 'setup-private\.ps1' -and $coldStartText -notmatch 'NOTE=STATE\.md is missing - run bin/bootstrap-clone') {
    Write-Host '[PASS] pre-setup startup blocks and routes to setup-private'
  } else { $coldStartOut | Out-Host; Write-Host "[FAIL] pre-setup startup routing (exit $coldStartExit)"; $failures++ }

  $missingModelOut = @(& pwsh -NoProfile -File (Join-Path $clone 'bin/setup-private.ps1') 2>&1)
  $missingModelExit = $LASTEXITCODE
  if ($missingModelExit -eq 1 -and ($missingModelOut | Out-String) -match 'missing -Model.+model id.+setup-private\.ps1 -Model') {
    Write-Host '[PASS] missing model id fails with an actionable command'
  } else { Write-Host '[FAIL] missing model id guidance'; $failures++ }

  & git -C $clone remote set-url origin 'https://github.com/StudioDoNotDisturb/mdr-kernel.git'
  Expect-Exit 'direct upstream bootstrap fails before memory is created' 1 {
    & pwsh -NoProfile -File (Join-Path $clone 'bin/bootstrap-clone.ps1') -Model 'test-model' -CommitScaffold
  }
  if (-not (Test-Path (Join-Path $clone 'STATE.md'))) { Write-Host '[PASS] refused bootstrap created no STATE.md' }
  else { Write-Host '[FAIL] refused bootstrap created STATE.md'; $failures++ }

  & git -C $clone remote set-url origin $bareA
  $beforeSetup = (& git --git-dir=$bareA rev-parse main).Trim()
  $firstSetupOut = @(& pwsh -NoProfile -File (Join-Path $clone 'bin/setup-private.ps1') -Model 'test-model' -ConfirmPrivateRemote 2>&1)
  $firstSetupExit = $LASTEXITCODE
  $firstSetupOut | Out-Host
  if ($firstSetupExit -eq 0) { Write-Host '[PASS] one-command private-remote setup verifies, bootstraps, and pushes' }
  else { Write-Host "[FAIL] one-command private-remote setup (exit $firstSetupExit)"; $failures++ }
  $firstSetupText = ($firstSetupOut | Out-String)
  if ($firstSetupText -match 'REFINER: TEST-MODEL' -and $firstSetupText -match 'PLEASE ENJOY EACH SESSION EQUALLY') {
    Write-Host '[PASS] first setup prints the induction badge and readiness tagline'
  } else { Write-Host '[FAIL] first setup missing the induction badge or readiness tagline'; $failures++ }
  $afterSetup = (& git --git-dir=$bareA rev-parse main).Trim()
  if ($beforeSetup -ne $afterSetup) { Write-Host '[PASS] setup pushed the initial private checkpoint' }
  else { Write-Host '[FAIL] setup did not push the initial private checkpoint'; $failures++ }
  $fullDoorsOk = $true
  foreach ($door in 'CLAUDE.md', 'AGENTS.md', 'GEMINI.md') {
    if ((Get-Content -Raw -LiteralPath (Join-Path $clone $door)) -notmatch 'BEGIN BRAIN-CONTRACT') { $fullDoorsOk = $false }
  }
  if ($fullDoorsOk) { Write-Host '[PASS] setup regenerated the phase-zero doors into full contract doors' }
  else { Write-Host '[FAIL] a door still lacks the contract block after setup'; $failures++ }

  # Approval and hooks live in .git/config, so every clone must run setup once. Prove both a
  # second-machine-style clone and an idempotent rerun before continuing the close battery.
  & git clone -q $bareA $secondClone
  if ($LASTEXITCODE) { throw 'could not create second-clone test copy' }
  & git -C $secondClone config user.name 'second clone'; & git -C $secondClone config user.email 'second@example.invalid'
  $secondSetupOut = @(& pwsh -NoProfile -File (Join-Path $secondClone 'bin/setup-private.ps1') -Model 'test-model' -ConfirmPrivateRemote 2>&1)
  $secondSetupExit = $LASTEXITCODE
  $secondSetupOut | Out-Host
  if ($secondSetupExit -eq 0) { Write-Host '[PASS] a second clone runs setup once to recreate local approval and hooks' }
  else { Write-Host "[FAIL] second-clone setup (exit $secondSetupExit)"; $failures++ }
  if (($secondSetupOut | Out-String) -notmatch 'REFINER:') { Write-Host '[PASS] second-clone setup skips the induction badge (scaffold came from history)' }
  else { Write-Host '[FAIL] second-clone setup reprinted the induction badge'; $failures++ }
  $secondApproved = (& git -C $secondClone config --local --get brain.approvedRemote)
  $secondHooks = (& git -C $secondClone config --local --get core.hooksPath)
  if ($secondApproved -and $secondHooks -eq '.githooks') { Write-Host '[PASS] second clone has local destination approval and hooks' }
  else { Write-Host '[FAIL] second clone did not recreate local safety state'; $failures++ }
  Expect-Exit 'setup rerun in the same clone is idempotent' 0 {
    & pwsh -NoProfile -File (Join-Path $secondClone 'bin/setup-private.ps1') -Model 'test-model' -ConfirmPrivateRemote
  }

  & git -C $clone remote add evil $bareB
  & git -C $clone config remote.pushDefault evil
  $evilBefore = (& git --git-dir=$bareB rev-parse main).Trim()

  $statePath = Join-Path $clone 'STATE.md'
  $state = [IO.File]::ReadAllText($statePath).Replace(
    'Fresh mdr-kernel clone. Replace this with your real current state.',
    'Private-onboarding close-set battery marker.'
  )
  [IO.File]::WriteAllText($statePath, $state, (New-Object Text.UTF8Encoding($false)))
  $stray = Join-Path $clone 'stray-download.bin'
  [IO.File]::WriteAllText($stray, 'must-not-be-committed', (New-Object Text.UTF8Encoding($false)))
  $parallelPath = Join-Path $clone 'CONTRIBUTING.md'
  Add-Content -LiteralPath $parallelPath -Value "`nParallel-session battery marker."
  $oldModel = $env:BRAIN_MODEL
  $env:BRAIN_MODEL = 'test-model'
  try {
    Expect-Exit 'close-session pushes private main without sweeping a stray file' 0 {
      & pwsh -NoProfile -File (Join-Path $clone 'bin/close-session.ps1') -NoHandoff
    }
  } finally {
    if ($null -eq $oldModel) { Remove-Item Env:\BRAIN_MODEL -ErrorAction SilentlyContinue }
    else { $env:BRAIN_MODEL = $oldModel }
  }
  & git -C $clone ls-files --error-unmatch stray-download.bin 2>$null
  if ($LASTEXITCODE -ne 0 -and (Test-Path $stray)) { Write-Host '[PASS] stray file stayed local and untracked' }
  else { Write-Host '[FAIL] stray file was tracked or removed'; $failures++ }
  if ((Get-Content -Raw $parallelPath) -match 'Parallel-session battery marker' -and
      -not ((& git --git-dir=$bareA show main:CONTRIBUTING.md | Out-String) -match 'Parallel-session battery marker')) {
    Write-Host '[PASS] shared close sync preserved an unrelated tracked edit without pushing it'
  } else { Write-Host '[FAIL] close sync did not preserve the unrelated tracked edit'; $failures++ }
  & git -C $clone restore CONTRIBUTING.md
  # The stray file close-session teaches adopters to leave behind must not block the next startup:
  # only uncommitted TRACKED changes make the pull unsafe (same gate sync-private uses).
  $strayStartOut = @(& pwsh -NoProfile -File (Join-Path $clone 'bin/start-session.ps1') -Model 'test-model' 2>&1)
  if ($LASTEXITCODE -eq 0 -and (($strayStartOut | Out-String) -match 'STARTUP_STATUS=READY')) {
    Write-Host '[PASS] startup stays READY with an untracked stray file present'
  } else { Write-Host '[FAIL] untracked stray file blocked startup'; $failures++ }
  $remoteState = (& git --git-dir=$bareA show main:STATE.md | Out-String)
  if ($remoteState -match 'close-set battery marker') { Write-Host '[PASS] close-session pushed the owned STATE update' }
  else { Write-Host '[FAIL] close-session did not push the owned STATE update'; $failures++ }
  if ((& git --git-dir=$bareB rev-parse main).Trim() -eq $evilBefore) { Write-Host '[PASS] explicit origin/main ignores divergent remote.pushDefault' }
  else { Write-Host '[FAIL] close-session followed remote.pushDefault'; $failures++ }

  # Explicit offline close: local commit only, persistent startup/doctor warning, then one-command
  # recovery with a fresh live privacy check. Point origin at a GitHub-shaped destination and
  # poison network proxies so this proves the no-push close uses only prior local approval.
  Add-Content -LiteralPath $statePath -Value "`nOffline checkpoint battery marker."
  $beforeOffline = (& git --git-dir=$bareA rev-parse main).Trim()
  $approvedBeforeOffline = (& git -C $clone config --local --get brain.approvedRemote)
  & git -C $clone config --local --unset-all brain.approvedRemote
  $missingPinOut = @(& pwsh -NoProfile -File (Join-Path $clone 'bin/close-session.ps1') -NoHandoff -OfflineCheckpoint -DryRun 2>&1)
  $missingPinExit = $LASTEXITCODE
  if ($missingPinExit -eq 1 -and ($missingPinOut | Out-String) -match 'requires a destination approved earlier') {
    Write-Host '[PASS] offline checkpoint refuses a clone without prior destination approval'
  } else { Write-Host '[FAIL] offline checkpoint accepted missing destination approval'; $failures++ }
  & git -C $clone config --local brain.approvedRemote $approvedBeforeOffline
  & git -C $clone remote set-url origin 'https://github.com/example/private-memory.git'
  $swappedOfflineOut = @(& pwsh -NoProfile -File (Join-Path $clone 'bin/close-session.ps1') -NoHandoff -OfflineCheckpoint -DryRun 2>&1)
  $swappedOfflineExit = $LASTEXITCODE
  if ($swappedOfflineExit -eq 1 -and ($swappedOfflineOut | Out-String) -match 'origin changed since approval' -and
      ($swappedOfflineOut | Out-String) -notmatch 'REMOTE_PRIVACY_STATUS=') {
    Write-Host '[PASS] offline checkpoint refuses an origin that no longer matches its approval pin'
  } else { Write-Host '[FAIL] offline checkpoint accepted a swapped origin'; $failures++ }
  & git -C $clone config --local brain.approvedRemote 'github:example/private-memory'
  $oldHttpProxy = $env:HTTP_PROXY; $oldHttpsProxy = $env:HTTPS_PROXY; $oldAllProxy = $env:ALL_PROXY
  $env:HTTP_PROXY = 'http://127.0.0.1:1'; $env:HTTPS_PROXY = 'http://127.0.0.1:1'; $env:ALL_PROXY = 'http://127.0.0.1:1'
  $env:BRAIN_MODEL = 'test-model'
  try {
    $offlineOut = @(& pwsh -NoProfile -File (Join-Path $clone 'bin/close-session.ps1') -NoHandoff -OfflineCheckpoint 2>&1)
    $offlineExit = $LASTEXITCODE
    $offlineOut | Out-Host
  } finally {
    if ($null -eq $oldModel) { Remove-Item Env:\BRAIN_MODEL -ErrorAction SilentlyContinue }
    else { $env:BRAIN_MODEL = $oldModel }
    if ($null -eq $oldHttpProxy) { Remove-Item Env:\HTTP_PROXY -ErrorAction SilentlyContinue } else { $env:HTTP_PROXY = $oldHttpProxy }
    if ($null -eq $oldHttpsProxy) { Remove-Item Env:\HTTPS_PROXY -ErrorAction SilentlyContinue } else { $env:HTTPS_PROXY = $oldHttpsProxy }
    if ($null -eq $oldAllProxy) { Remove-Item Env:\ALL_PROXY -ErrorAction SilentlyContinue } else { $env:ALL_PROXY = $oldAllProxy }
    & git -C $clone remote set-url origin $bareA
    & git -C $clone config --local brain.approvedRemote $approvedBeforeOffline
  }
  $offlineText = ($offlineOut | Out-String)
  if ($offlineExit -eq 0 -and $offlineText -match 'existing private destination approval found' -and
      $offlineText -notmatch 'REMOTE_PRIVACY_STATUS=') { Write-Host '[PASS] offline GitHub checkpoint uses no live privacy probe' }
  else { Write-Host '[FAIL] offline GitHub checkpoint attempted live verification or did not close'; $failures++ }
  $afterOffline = (& git --git-dir=$bareA rev-parse main).Trim()
  if ($beforeOffline -eq $afterOffline -and (& git -C $clone config --local --get brain.pendingSync) -eq 'true') {
    Write-Host '[PASS] offline checkpoint left remote unchanged and marked pending sync'
  } else { Write-Host '[FAIL] offline checkpoint push/marker invariant'; $failures++ }
  $doctorOut = (& pwsh -NoProfile -File (Join-Path $clone 'bin/brain-doctor.ps1') | Out-String)
  if ($doctorOut -match 'PENDING PRIVATE SYNC.+bin/sync-private\.ps1') { Write-Host '[PASS] doctor names exact pending-sync recovery command' }
  else { Write-Host '[FAIL] doctor did not surface exact pending-sync recovery'; $failures++ }
  Expect-Exit 'sync-private live-verifies and recovers offline checkpoint' 0 {
    & pwsh -NoProfile -File (Join-Path $clone 'bin/sync-private.ps1')
  }
  $synced = (& git --git-dir=$bareA show main:STATE.md | Out-String)
  if ($synced -match 'Offline checkpoint battery marker' -and -not (& git -C $clone config --local --get brain.pendingSync 2>$null)) {
    Write-Host '[PASS] recovery pushed checkpoint and cleared pending marker'
  } else { Write-Host '[FAIL] recovery did not push/clear marker'; $failures++ }

  # A concurrent, non-conflicting private checkpoint should trigger exactly one automatic rebase
  # and then land without expert intervention.
  & git clone -q $bareA $peer
  & git -C $peer config user.name 'peer'; & git -C $peer config user.email 'peer@example.invalid'
  New-Item -ItemType Directory -Path (Join-Path $peer 'reference') -Force | Out-Null
  Set-Content -LiteralPath (Join-Path $peer 'reference/peer-battery.md') -Value 'peer checkpoint'
  & git -C $peer -c core.hooksPath=/dev/null add reference/peer-battery.md
  & git -C $peer -c core.hooksPath=/dev/null commit -q -m 'peer checkpoint'
  & git -C $peer -c core.hooksPath=/dev/null push -q origin main
  Add-Content -LiteralPath $statePath -Value "`nAutomatic retry battery marker."
  $env:BRAIN_MODEL = 'test-model'
  try {
    Expect-Exit 'close-session delegates shared sync and rebases a non-conflicting checkpoint' 0 {
      & pwsh -NoProfile -File (Join-Path $clone 'bin/close-session.ps1') -NoHandoff
    }
  } finally {
    if ($null -eq $oldModel) { Remove-Item Env:\BRAIN_MODEL -ErrorAction SilentlyContinue }
    else { $env:BRAIN_MODEL = $oldModel }
  }
  if ((& git --git-dir=$bareA show main:STATE.md | Out-String) -match 'Automatic retry battery marker') {
    Write-Host '[PASS] single retry delivered local checkpoint'
  } else { Write-Host '[FAIL] retry did not deliver local checkpoint'; $failures++ }

  # Git reports a successful rebase even when applying --autostash leaves conflicts. Prove the
  # shared sync detects that dirty-tree restoration failure before push and keeps recovery loud.
  & git -C $peer pull -q --rebase
  $peerContributing = Join-Path $peer 'CONTRIBUTING.md'
  (Get-Content -Raw $peerContributing).Replace('# Contributing to mdr-kernel', '# Remote contributing to mdr-kernel') |
    Set-Content -NoNewline -LiteralPath $peerContributing
  & git -C $peer -c core.hooksPath=/dev/null add CONTRIBUTING.md
  & git -C $peer -c core.hooksPath=/dev/null commit -q -m 'peer contributing change'
  & git -C $peer -c core.hooksPath=/dev/null push -q origin main
  (Get-Content -Raw $parallelPath).Replace('# Contributing to mdr-kernel', '# Local contributing to mdr-kernel') |
    Set-Content -NoNewline -LiteralPath $parallelPath
  Add-Content -LiteralPath $statePath -Value "`nAutostash safety battery marker."
  $remoteBeforeAutostash = (& git --git-dir=$bareA rev-parse main).Trim()
  $env:BRAIN_MODEL = 'test-model'
  try {
    Expect-Exit 'autostash restoration conflict fails before push' 1 {
      & pwsh -NoProfile -File (Join-Path $clone 'bin/close-session.ps1') -NoHandoff
    }
  } finally {
    if ($null -eq $oldModel) { Remove-Item Env:\BRAIN_MODEL -ErrorAction SilentlyContinue }
    else { $env:BRAIN_MODEL = $oldModel }
  }
  $unmerged = @(& git -C $clone diff --name-only --diff-filter=U)
  if ((& git --git-dir=$bareA rev-parse main).Trim() -eq $remoteBeforeAutostash -and $unmerged -contains 'CONTRIBUTING.md' -and
      (Test-Path -LiteralPath (Join-Path (Split-Path -Parent ((& git -C $clone rev-parse --path-format=absolute --git-common-dir).Trim())) 'STATE.lock'))) {
    Write-Host '[PASS] autostash conflict kept the remote unchanged and retained the lock'
  } else { Write-Host '[FAIL] autostash conflict crossed the push/lock boundary'; $failures++ }
  $env:BRAIN_MODEL = 'test-model'
  $env:MDR_FORCE_LIVE_LOCK = '1'   # harness cleanup: the holder is a finished test step, not a live partner
  & pwsh -NoProfile -File (Join-Path $clone 'bin/state-lock.ps1') -Release -Force | Out-Null
  Remove-Item Env:\MDR_FORCE_LIVE_LOCK -ErrorAction SilentlyContinue
  if ($null -eq $oldModel) { Remove-Item Env:\BRAIN_MODEL -ErrorAction SilentlyContinue } else { $env:BRAIN_MODEL = $oldModel }
  & git -C $clone reset --hard -q origin/main
  & git -C $clone stash clear
  & git -C $clone config --local --unset-all brain.pendingSync 2>$null

  # Drive round 3, F1: under pwsh -File the documented comma form of -Also must carry BOTH paths.
  Set-Content -LiteralPath (Join-Path $clone 'notes-a.md') -Value 'battery also-a'
  Set-Content -LiteralPath (Join-Path $clone 'notes-b.md') -Value 'battery also-b'
  $env:BRAIN_MODEL = 'test-model'
  try {
    Expect-Exit 'close accepts multiple -Also paths in comma form' 0 {
      & pwsh -NoProfile -File (Join-Path $clone 'bin/close-session.ps1') -NoHandoff -Also 'notes-a.md,notes-b.md'
    }
  } finally {
    if ($null -eq $oldModel) { Remove-Item Env:\BRAIN_MODEL -ErrorAction SilentlyContinue }
    else { $env:BRAIN_MODEL = $oldModel }
  }
  if (@(& git -C $clone ls-files notes-a.md notes-b.md).Count -eq 2) {
    Write-Host '[PASS] comma-form -Also committed both named paths'
  } else { Write-Host '[FAIL] comma-form -Also dropped a path'; $failures++ }

  # Drive round 5 incident: mixing a pre-staged deletion (git rm) with the same path in -Also
  # aborted the single git add and split the close into two commits. One close, one commit.
  & git -C $clone rm -q notes-b.md
  Add-Content -LiteralPath $statePath -Value "`nSplit-close battery marker."
  $env:BRAIN_MODEL = 'test-model'
  try {
    Expect-Exit 'close accepts a pre-staged deletion named in -Also without splitting' 0 {
      & pwsh -NoProfile -File (Join-Path $clone 'bin/close-session.ps1') -NoHandoff -Also 'notes-b.md'
    }
  } finally {
    if ($null -eq $oldModel) { Remove-Item Env:\BRAIN_MODEL -ErrorAction SilentlyContinue }
    else { $env:BRAIN_MODEL = $oldModel }
  }
  $splitCloseShow = (& git -C $clone show --name-status --format= HEAD | Out-String)
  if ($splitCloseShow -match 'D\s+notes-b\.md' -and $splitCloseShow -match 'M\s+STATE\.md' -and -not (git -C $clone status --porcelain --untracked-files=no)) {
    Write-Host '[PASS] deletion and owned edits landed in ONE close commit with a clean tree'
  } else { Write-Host '[FAIL] pre-staged deletion split or dirtied the close'; $failures++ }

  # Round-4 micro-finding: an -Also spec that matches nothing pending must warn loudly instead of
  # letting the session believe its file rode the close.
  Set-Content -LiteralPath (Join-Path $clone 'notes-c.md') -Value 'battery also-c'
  $env:BRAIN_MODEL = 'test-model'
  try {
    $ghostOut = @(& pwsh -NoProfile -File (Join-Path $clone 'bin/close-session.ps1') -NoHandoff -Also 'notes-c.md,ghost-note.md' 2>&1)
    $ghostExit = $LASTEXITCODE
  } finally {
    if ($null -eq $oldModel) { Remove-Item Env:\BRAIN_MODEL -ErrorAction SilentlyContinue }
    else { $env:BRAIN_MODEL = $oldModel }
  }
  $ghostText = ($ghostOut | Out-String)
  if ($ghostExit -eq 0 -and $ghostText -match "\[warn\] -Also 'ghost-note\.md' matched no pending change" -and
      $ghostText -notmatch "-Also 'notes-c\.md' matched no pending" -and
      @(& git -C $clone ls-files notes-c.md).Count -eq 1) {
    Write-Host '[PASS] close warns on a no-match -Also path and still commits the matched one'
  } else { $ghostOut | Out-Host; Write-Host "[FAIL] no-match -Also warning (exit $ghostExit)"; $failures++ }

  # Round-4 micro-finding: write-approval argument errors surface as one-line [FAIL] verdicts,
  # never raw PowerShell exception dumps.
  $waFailOut = @(& pwsh -NoProfile -File (Join-Path $clone 'bin/write-approval.ps1') -Propose 2>&1)
  $waFailExit = $LASTEXITCODE
  $waFailText = ($waFailOut | Out-String)
  if ($waFailExit -eq 1 -and $waFailText -match '\[FAIL\] -Propose requires' -and
      $waFailText -notmatch 'Exception|At line|CategoryInfo') {
    Write-Host '[PASS] write-approval renders argument errors as [FAIL], not raw exceptions'
  } else { $waFailOut | Out-Host; Write-Host "[FAIL] write-approval error style (exit $waFailExit)"; $failures++ }

  # Drive round 3, F2/F4: the template's own prescribed flow for new durable knowledge —
  # write-approval propose/approve, INDEX row, close with -Also; the audit JSONL rides itself.
  $draft = Join-Path $sandbox 'battery-note-draft.md'
  Set-Content -LiteralPath $draft -Value 'Battery reference note body.'
  $env:BRAIN_MODEL = 'test-model'
  try {
    $proposeOut = @(& pwsh -NoProfile -File (Join-Path $clone 'bin/write-approval.ps1') -Propose -Target 'reference/battery-note.md' -Source $draft -Reason 'battery flow' 2>&1)
    $proposeOut | Out-Host
    $proposalId = (($proposeOut | Where-Object { $_ -like 'PROPOSAL_ID=*' } | Select-Object -First 1) -replace '^PROPOSAL_ID=', '')
    & pwsh -NoProfile -File (Join-Path $clone 'bin/write-approval.ps1') -Approve -ProposalId $proposalId -Reason 'battery approve' | Out-Host
    Add-Content -LiteralPath (Join-Path $clone 'reference/INDEX.md') -Value '| battery-note.md | battery write-approval flow note |'
    Expect-Exit 'write-approval flow closes with file, INDEX row, and auto-ridden audit' 0 {
      & pwsh -NoProfile -File (Join-Path $clone 'bin/close-session.ps1') -NoHandoff -Also 'reference/battery-note.md'
    }
  } finally {
    if ($null -eq $oldModel) { Remove-Item Env:\BRAIN_MODEL -ErrorAction SilentlyContinue }
    else { $env:BRAIN_MODEL = $oldModel }
  }
  $remoteHasNote = ((& git --git-dir=$bareA show main:reference/battery-note.md 2>$null | Out-String) -match 'Battery reference note')
  $remoteHasAudit = ((& git --git-dir=$bareA show main:inbox/write-approval-audit.jsonl 2>$null | Out-String) -match 'battery-note')
  if ($remoteHasNote -and $remoteHasAudit) { Write-Host '[PASS] approved note and its audit entry both reached the private remote' }
  else { Write-Host '[FAIL] write-approval close left the note or audit behind'; $failures++ }

  # The durable write-approval audit log is not a foldable inbox note: with only README + the
  # audit in inbox/, a close must not emit the unfolded-note warning (doctor and close agree).
  $env:BRAIN_MODEL = 'test-model'
  try {
    $auditWarnOut = @(& pwsh -NoProfile -File (Join-Path $clone 'bin/close-session.ps1') -NoHandoff -DryRun 2>&1)
  } finally {
    if ($null -eq $oldModel) { Remove-Item Env:\BRAIN_MODEL -ErrorAction SilentlyContinue }
    else { $env:BRAIN_MODEL = $oldModel }
  }
  if (($auditWarnOut | Out-String) -notmatch 'unfolded note') {
    Write-Host '[PASS] close does not count the write-approval audit log as an unfolded inbox note'
  } else { $auditWarnOut | Out-Host; Write-Host '[FAIL] close still warns on the exempt audit log'; $failures++ }

  # Drive round 3, F3: a staged INDEX row must never reference a file the close leaves behind.
  Set-Content -LiteralPath (Join-Path $clone 'reference/rogue-note.md') -Value 'rogue direct write'
  Add-Content -LiteralPath (Join-Path $clone 'reference/INDEX.md') -Value '| rogue-note.md | rogue row for battery |'
  $env:BRAIN_MODEL = 'test-model'
  try {
    $rogueOut = @(& pwsh -NoProfile -File (Join-Path $clone 'bin/close-session.ps1') -NoHandoff 2>&1)
    $rogueExit = $LASTEXITCODE
  } finally {
    if ($null -eq $oldModel) { Remove-Item Env:\BRAIN_MODEL -ErrorAction SilentlyContinue }
    else { $env:BRAIN_MODEL = $oldModel }
  }
  if ($rogueExit -eq 1 -and (($rogueOut | Out-String) -match 'referencing left-behind')) {
    Write-Host '[PASS] close refuses an INDEX row pointing at a left-behind file'
  } else { $rogueOut | Out-Host; Write-Host "[FAIL] INDEX/leftover guard (exit $rogueExit)"; $failures++ }
  & git -C $clone reset --hard -q
  Remove-Item -LiteralPath (Join-Path $clone 'reference/rogue-note.md') -ErrorAction SilentlyContinue

  # Drive round 3, F5: a session claiming one identity over a different clone fallback gets told.
  $divergeOut = @(& pwsh -NoProfile -File (Join-Path $clone 'bin/start-session.ps1') -Model 'someone-else' 2>&1)
  if ($LASTEXITCODE -eq 0 -and (($divergeOut | Out-String) -match "fallback identity is 'test-model'")) {
    Write-Host '[PASS] startup names the identity divergence and the exact seal command'
  } else { Write-Host '[FAIL] identity divergence was silent'; $failures++ }

  # Conflicting concurrent edits fail closed: abort the rebase, keep the lock, and leave a named
  # recovery path instead of looping or guessing at conflict resolution.
  & git -C $peer pull -q --rebase
  $peerStatePath = Join-Path $peer 'STATE.md'
  (Get-Content -Raw $peerStatePath).Replace('Automatic retry battery marker.', 'Remote conflict battery marker.') |
    Set-Content -NoNewline -LiteralPath $peerStatePath
  & git -C $peer -c core.hooksPath=/dev/null add STATE.md
  & git -C $peer -c core.hooksPath=/dev/null commit -q -m 'remote conflicting checkpoint'
  & git -C $peer -c core.hooksPath=/dev/null push -q origin main
  (Get-Content -Raw $statePath).Replace('Automatic retry battery marker.', 'Local conflict battery marker.') |
    Set-Content -NoNewline -LiteralPath $statePath
  $env:BRAIN_MODEL = 'test-model'
  try {
    Expect-Exit 'conflicting retry fails closed after one attempt' 1 {
      & pwsh -NoProfile -File (Join-Path $clone 'bin/close-session.ps1') -NoHandoff
    }
  } finally {
    if ($null -eq $oldModel) { Remove-Item Env:\BRAIN_MODEL -ErrorAction SilentlyContinue }
    else { $env:BRAIN_MODEL = $oldModel }
  }
  & git -C $clone rev-parse --verify -q REBASE_HEAD 2>$null
  $rebaseActive = ($LASTEXITCODE -eq 0)
  $commonDir = (& git -C $clone rev-parse --path-format=absolute --git-common-dir).Trim()
  $testLock = Join-Path (Split-Path -Parent $commonDir) 'STATE.lock'
  if (-not $rebaseActive -and (Test-Path -LiteralPath $testLock) -and (& git -C $clone config --local --get brain.pendingSync) -eq 'true') {
    Write-Host '[PASS] conflict was aborted, lock retained, and pending sync stayed loud'
  } else { Write-Host '[FAIL] conflict safe-failure invariant'; $failures++ }
  $env:BRAIN_MODEL = 'test-model'
  $env:MDR_FORCE_LIVE_LOCK = '1'   # harness cleanup: the holder is a finished test step, not a live partner
  & pwsh -NoProfile -File (Join-Path $clone 'bin/state-lock.ps1') -Release -Force | Out-Null
  Remove-Item Env:\MDR_FORCE_LIVE_LOCK -ErrorAction SilentlyContinue
  if ($null -eq $oldModel) { Remove-Item Env:\BRAIN_MODEL -ErrorAction SilentlyContinue } else { $env:BRAIN_MODEL = $oldModel }
  & git -C $clone reset --hard -q origin/main
  & git -C $clone config --local --unset-all brain.pendingSync 2>$null

  & git -C $clone remote set-url origin $bareB
  Expect-Exit 'approved destination pin refuses a remote swap' 1 {
    & pwsh -NoProfile -File (Join-Path $clone 'bin/remote-privacy.ps1') -RequireApproved -ConfirmPrivateRemote
  }

  $before = (& git --git-dir=$bareB rev-parse main).Trim()
  Expect-Exit 'pre-push propagates the remote-swap refusal' 1 {
    & git -C $clone push origin main
  }
  $after = (& git --git-dir=$bareB rev-parse main).Trim()
  if ($before -eq $after) { Write-Host '[PASS] blocked push left swapped remote unchanged' }
  else { Write-Host '[FAIL] blocked push changed swapped remote'; $failures++ }

  # Local-only is a stable mode, not an offline-checkpoint error path.
  & git clone -q $bareA $localClone
  & git -C $localClone config user.name 'local'; & git -C $localClone config user.email 'local@example.invalid'
  Expect-Exit 'local-only setup succeeds' 0 {
    & pwsh -NoProfile -File (Join-Path $localClone 'bin/setup-private.ps1') -Model 'test-model' -LocalOnly
  }
  $env:BRAIN_MODEL = 'test-model'
  try {
    Expect-Exit 'local-only mode refuses redundant OfflineCheckpoint' 1 {
      & pwsh -NoProfile -File (Join-Path $localClone 'bin/close-session.ps1') -NoHandoff -OfflineCheckpoint
    }
  } finally {
    if ($null -eq $oldModel) { Remove-Item Env:\BRAIN_MODEL -ErrorAction SilentlyContinue }
    else { $env:BRAIN_MODEL = $oldModel }
  }

  # ---- CP2 regressions: lifecycle invariants (system-audit findings 4-6, 8) ----
  & git -C $clone remote set-url origin $bareA   # restore the approved origin after the swap tests
  $commonDir2 = (& git -C $clone rev-parse --path-format=absolute --git-common-dir).Trim()
  $cloneLock = Join-Path (Split-Path -Parent $commonDir2) 'STATE.lock'

  # Finding 4: a claim of new lock ownership needs an explicit identity; the clone's git config
  # brain.model ('test-model' here) is refused as an acquire/close identity source.
  $savedBM = $env:BRAIN_MODEL
  Remove-Item Env:\BRAIN_MODEL -ErrorAction SilentlyContinue
  try {
    $cfgAcqOut = @(& pwsh -NoProfile -File (Join-Path $clone 'bin/state-lock.ps1') -Acquire -Objective 'cp2 battery' 2>&1)
    $cfgAcqExit = $LASTEXITCODE
    $cfgAcqText = ($cfgAcqOut | Out-String)
    if ($cfgAcqExit -eq 1 -and $cfgAcqText -match 'refusing -Acquire' -and
        $cfgAcqText -match "identity resolved from 'config'" -and -not (Test-Path $cloneLock)) {
      Write-Host '[PASS] acquire refuses a config-fallback identity and creates no lock'
    } else { $cfgAcqOut | Out-Host; Write-Host "[FAIL] config-fallback acquire refusal (exit $cfgAcqExit)"; $failures++ }

    Add-Content -LiteralPath $statePath -Value "`nCP2 close identity battery marker."
    $cfgCloseOut = @(& pwsh -NoProfile -File (Join-Path $clone 'bin/close-session.ps1') -NoHandoff 2>&1)
    $cfgCloseExit = $LASTEXITCODE
    if ($cfgCloseExit -eq 1 -and (($cfgCloseOut | Out-String) -match "identity must be explicit \(resolved from 'config'")) {
      Write-Host '[PASS] close refuses a config-only committing identity'
    } else { $cfgCloseOut | Out-Host; Write-Host "[FAIL] close config-identity refusal (exit $cfgCloseExit)"; $failures++ }

    # ...and the -Model parameter alone carries identity through lock, hook, and trailer.
    Expect-Exit 'close with explicit -Model completes end to end' 0 {
      & pwsh -NoProfile -File (Join-Path $clone 'bin/close-session.ps1') -NoHandoff -Model 'test-model'
    }
    $trailer = (& git -C $clone log -1 --format=%B | Out-String)
    if ($trailer -match 'Brain-Model: test-model') { Write-Host '[PASS] -Model close stamped the Brain-Model trailer' }
    else { Write-Host '[FAIL] -Model close missing the Brain-Model trailer'; $failures++ }
  } finally {
    if ($null -ne $savedBM) { $env:BRAIN_MODEL = $savedBM }
  }

  # Finding 5: a second same-model process without the session token must not adopt, close over,
  # or release a live partner's lock.
  $acqOut = @(& pwsh -NoProfile -File (Join-Path $clone 'bin/state-lock.ps1') -Acquire -Model 'test-model' -Objective 'cp2 token battery' 2>&1)
  $acqExit = $LASTEXITCODE
  $lockJson = $null
  if (Test-Path $cloneLock) { try { $lockJson = (Get-Content -Raw $cloneLock | ConvertFrom-Json) } catch {} }
  if ($acqExit -eq 0 -and $lockJson -and $lockJson.model -eq 'test-model' -and $lockJson.token) {
    Write-Host '[PASS] explicit -Model acquire records identity and a session token'
  } else { $acqOut | Out-Host; Write-Host "[FAIL] explicit -Model acquire (exit $acqExit)"; $failures++ }
  Add-Content -LiteralPath $statePath -Value "`nCP2 token battery marker."
  $env:BRAIN_MODEL = 'test-model'
  Remove-Item Env:\BRAIN_LOCK_TOKEN -ErrorAction SilentlyContinue
  try {
    $adoptOut = @(& pwsh -NoProfile -File (Join-Path $clone 'bin/close-session.ps1') -NoHandoff 2>&1)
    $adoptExit = $LASTEXITCODE
  } finally {
    if ($null -eq $oldModel) { Remove-Item Env:\BRAIN_MODEL -ErrorAction SilentlyContinue }
    else { $env:BRAIN_MODEL = $oldModel }
  }
  $lockAfter = $null
  if (Test-Path $cloneLock) { try { $lockAfter = (Get-Content -Raw $cloneLock | ConvertFrom-Json) } catch {} }
  if ($adoptExit -eq 1 -and (($adoptOut | Out-String) -match "another 'test-model' session \(token mismatch\)") -and
      $lockAfter -and $lockAfter.token -eq $lockJson.token) {
    Write-Host '[PASS] close refuses to adopt a live same-model lock token'
  } else { $adoptOut | Out-Host; Write-Host "[FAIL] same-model token adoption refusal (exit $adoptExit)"; $failures++ }

  # The rightful holder (correct token) still closes over its own lock.
  $env:BRAIN_MODEL = 'test-model'; $env:BRAIN_LOCK_TOKEN = [string]$lockJson.token
  try {
    Expect-Exit 'holder with the correct token closes over its own lock' 0 {
      & pwsh -NoProfile -File (Join-Path $clone 'bin/close-session.ps1') -NoHandoff
    }
  } finally {
    if ($null -eq $oldModel) { Remove-Item Env:\BRAIN_MODEL -ErrorAction SilentlyContinue }
    else { $env:BRAIN_MODEL = $oldModel }
    Remove-Item Env:\BRAIN_LOCK_TOKEN -ErrorAction SilentlyContinue
  }

  # Same trap, shell-safe recovery: -Token as a PARAMETER must work with no env var at all —
  # drive evidence showed an agent fumbling cross-shell $env: quoting and escalating to -Force
  # because the safe path demanded shell gymnastics the unsafe path didn't.
  $acq2Out = @(& pwsh -NoProfile -File (Join-Path $clone 'bin/state-lock.ps1') -Acquire -Model 'test-model' -Objective 'cp2 token param battery' 2>&1)
  $lockJson2 = $null
  if (Test-Path $cloneLock) { try { $lockJson2 = (Get-Content -Raw $cloneLock | ConvertFrom-Json) } catch {} }
  if ($lockJson2 -and $lockJson2.token) { Write-Host '[PASS] second acquire for the token-param leg holds a sealed lock' }
  else { $acq2Out | Out-Host; Write-Host '[FAIL] token-param leg could not acquire a sealed lock'; $failures++ }
  Add-Content -LiteralPath $statePath -Value "`nCP2 token-param battery marker."
  $env:BRAIN_MODEL = 'test-model'
  Remove-Item Env:\BRAIN_LOCK_TOKEN -ErrorAction SilentlyContinue
  try {
    Expect-Exit 'holder recovers with the -Token parameter alone (no env var)' 0 {
      & pwsh -NoProfile -File (Join-Path $clone 'bin/close-session.ps1') -NoHandoff -Token ([string]$lockJson2.token)
    }
  } finally {
    if ($null -eq $oldModel) { Remove-Item Env:\BRAIN_MODEL -ErrorAction SilentlyContinue }
    else { $env:BRAIN_MODEL = $oldModel }
  }

  # ---- Drive-2 lock-safety battery (2026-07-12 concurrent-session drive): a refused acquire is
  # the only guidance an un-briefed agent has, so it must steer to -Token/-Wait and never hand a
  # live-lock takeover recipe to the party it just refused; -Release -Force must refuse a
  # fresh-heartbeat lock outright (the drive destroyed a 21-second-old live partner lock). ----

  # Live same-model refusal: safe moves offered, no takeover recipe.
  $dl1Out = @(& pwsh -NoProfile -File (Join-Path $clone 'bin/state-lock.ps1') -Acquire -Model 'test-model' -Objective 'drive2 refusal-text battery' 2>&1)
  $dl1Json = $null
  if (Test-Path $cloneLock) { try { $dl1Json = (Get-Content -Raw $cloneLock | ConvertFrom-Json) } catch {} }
  if ($dl1Json -and $dl1Json.token) { Write-Host '[PASS] drive2 leg holds a sealed lock' }
  else { $dl1Out | Out-Host; Write-Host '[FAIL] drive2 leg could not acquire a sealed lock'; $failures++ }
  Remove-Item Env:\BRAIN_LOCK_TOKEN -ErrorAction SilentlyContinue
  $refuseOut = @(& pwsh -NoProfile -File (Join-Path $clone 'bin/state-lock.ps1') -Acquire -Model 'test-model' 2>&1)
  $refuseExit = $LASTEXITCODE
  $refuseText = ($refuseOut | Out-String)
  if ($refuseExit -eq 1 -and $refuseText -match '-Acquire -Wait' -and $refuseText -match '-Token <token>' -and
      $refuseText -match 'NOT PROVEN' -and $refuseText -notmatch 'take it over') {
    Write-Host '[PASS] live same-model refusal steers to -Token/-Wait with no takeover recipe'
  } else { $refuseOut | Out-Host; Write-Host "[FAIL] live same-model refusal guidance (exit $refuseExit)"; $failures++ }

  # Force refuses the live lock and leaves it in place; the confirmed-dead valve still works.
  $forceLiveOut = @(& pwsh -NoProfile -File (Join-Path $clone 'bin/state-lock.ps1') -Release -Force -Model 'test-model' 2>&1)
  $forceLiveExit = $LASTEXITCODE
  if ($forceLiveExit -eq 1 -and (($forceLiveOut | Out-String) -match 'refusing -Release -Force') -and (Test-Path $cloneLock)) {
    Write-Host '[PASS] -Release -Force refuses a fresh-heartbeat lock and leaves it in place'
  } else { $forceLiveOut | Out-Host; Write-Host "[FAIL] live-lock Force refusal (exit $forceLiveExit)"; $failures++ }
  $env:MDR_FORCE_LIVE_LOCK = '1'
  try {
    Expect-Exit 'MDR_FORCE_LIVE_LOCK=1 valve force-releases a fresh lock' 0 {
      & pwsh -NoProfile -File (Join-Path $clone 'bin/state-lock.ps1') -Release -Force -Model 'test-model'
    }
  } finally { Remove-Item Env:\MDR_FORCE_LIVE_LOCK -ErrorAction SilentlyContinue }

  # A stale heartbeat (>120m) is dead-session recovery: the refusal switches to the takeover
  # recipe and Force works with no valve.
  & pwsh -NoProfile -File (Join-Path $clone 'bin/state-lock.ps1') -Acquire -Model 'test-model' -Objective 'drive2 stale battery' | Out-Null
  $staleRaw = Get-Content -Raw -LiteralPath $cloneLock
  $staleIso = [DateTimeOffset]::UtcNow.AddHours(-3).ToUniversalTime().ToString('o')
  $staleRaw = $staleRaw -replace '"heartbeat_at":\s*"[^"]+"', ('"heartbeat_at": "' + $staleIso + '"')
  Set-Content -LiteralPath $cloneLock -Value $staleRaw -NoNewline
  Remove-Item Env:\BRAIN_LOCK_TOKEN -ErrorAction SilentlyContinue
  $staleRefuse = @(& pwsh -NoProfile -File (Join-Path $clone 'bin/state-lock.ps1') -Acquire -Model 'test-model' 2>&1)
  $staleText = ($staleRefuse | Out-String)
  if ($staleText -match 'stale past 120m' -and $staleText -match '-Release -Force, then -Acquire') {
    Write-Host '[PASS] stale same-model refusal surfaces the takeover recipe'
  } else { $staleRefuse | Out-Host; Write-Host '[FAIL] stale refusal guidance'; $failures++ }
  Expect-Exit 'stale lock force-releases without the valve' 0 {
    & pwsh -NoProfile -File (Join-Path $clone 'bin/state-lock.ps1') -Release -Force -Model 'test-model'
  }

  # -Wait is the queue primitive: it times out against a live holder and acquires after release.
  & pwsh -NoProfile -File (Join-Path $clone 'bin/state-lock.ps1') -Acquire -Model 'test-model' -Objective 'drive2 wait battery' | Out-Null
  $waitJson = (Get-Content -Raw -LiteralPath $cloneLock | ConvertFrom-Json)
  Remove-Item Env:\BRAIN_LOCK_TOKEN -ErrorAction SilentlyContinue
  $waitOut = @(& pwsh -NoProfile -File (Join-Path $clone 'bin/state-lock.ps1') -Acquire -Wait -TimeoutSec 6 -Model 'test-model' 2>&1)
  $waitExit = $LASTEXITCODE
  $waitText = ($waitOut | Out-String)
  if ($waitExit -eq 1 -and $waitText -match 'polling every 5s' -and $waitText -match 'gave up after 6s') {
    Write-Host '[PASS] -Wait polls, then gives up on budget against a live holder'
  } else { $waitOut | Out-Host; Write-Host "[FAIL] -Wait timeout behavior (exit $waitExit)"; $failures++ }
  $releaseJob = Start-Job -ScriptBlock {
    param($slPath, $tok)
    Start-Sleep -Seconds 7
    & pwsh -NoProfile -File $slPath -Release -Token $tok -Model 'test-model'
  } -ArgumentList (Join-Path $clone 'bin/state-lock.ps1'), ([string]$waitJson.token)
  $waitedOut = @(& pwsh -NoProfile -File (Join-Path $clone 'bin/state-lock.ps1') -Acquire -Wait -TimeoutSec 45 -Model 'test-model' -Objective 'drive2 wait winner' 2>&1)
  $waitedExit = $LASTEXITCODE
  Wait-Job $releaseJob | Out-Null; Remove-Job $releaseJob -Force
  $wonJson = $null
  if (Test-Path $cloneLock) { try { $wonJson = (Get-Content -Raw $cloneLock | ConvertFrom-Json) } catch {} }
  if ($waitedExit -eq 0 -and $wonJson -and $wonJson.token -ne $waitJson.token) {
    Write-Host '[PASS] -Wait acquires with a fresh token once the holder releases'
  } else { $waitedOut | Out-Host; Write-Host "[FAIL] -Wait acquire-after-release (exit $waitedExit)"; $failures++ }
  & pwsh -NoProfile -File (Join-Path $clone 'bin/state-lock.ps1') -Release -Token ([string]$wonJson.token) -Model 'test-model' | Out-Null

  # ---- Drive-2 close date tolerance: an honest local-date/UTC±1 handoff closes without renames,
  # and the miss message names the accepted window + -SessionDate (never a rename). ----
  $dtDate = (Get-Date).ToUniversalTime().AddDays(-1).ToString('yyyy-MM-dd')
  $dtHandoff = Join-Path $clone "handoffs/$dtDate-datetol.md"
  $dtBody = @(
    "Title: Handoff - $dtDate - datetol"
    'Type: Handoff / Session Closeout'
    'Context: Drive-2 date-tolerance battery handoff.'
    ''
    "# Handoff - $dtDate - datetol"
    ''
    'Battery body.'
    ''
    '---'
    ''
    '## SYSTEM NODE LINKS'
    ''
    'Directory Path: handoffs/'
    'Next Sequential File: INDEX.md'
    'Related Workflows: STATE.md'
    'AI Execution Prompts: ../protocol/CONTRACT.md'
  ) -join "`n"
  Set-Content -LiteralPath $dtHandoff -Value $dtBody -NoNewline
  $dtIndexPath = Join-Path $clone 'handoffs/INDEX.md'
  $dtIndex = Get-Content -LiteralPath $dtIndexPath
  $dtIndexOut = foreach ($line in $dtIndex) {
    $line
    if ($line -match '^\|---\|') { "| $dtDate | $dtDate-datetol.md | Drive-2 date-tolerance battery | Closed |" }
  }
  Set-Content -LiteralPath $dtIndexPath -Value ($dtIndexOut -join "`n")
  $dtOut = @(& pwsh -NoProfile -File (Join-Path $clone 'bin/close-session.ps1') -Topic datetol -Model 'test-model' 2>&1)
  $dtExit = $LASTEXITCODE
  $dtText = ($dtOut | Out-String)
  if ($dtExit -eq 0 -and $dtText -match "handoff date $dtDate accepted") {
    Write-Host '[PASS] close accepts a UTC-1-dated handoff and says which date it used'
  } else { $dtOut | Out-Host; Write-Host "[FAIL] close date tolerance (exit $dtExit)"; $failures++ }

  $missOut = @(& pwsh -NoProfile -File (Join-Path $clone 'bin/close-session.ps1') -Topic no-such-topic -Model 'test-model' 2>&1)
  $missExit = $LASTEXITCODE
  $missText = ($missOut | Out-String)
  if ($missExit -eq 1 -and $missText -match 'accepted dates for -Topic' -and $missText -match '-SessionDate <yyyy-MM-dd>') {
    Write-Host '[PASS] missing-handoff failure names the accepted window and -SessionDate'
  } else { $missOut | Out-Host; Write-Host "[FAIL] missing-handoff date guidance (exit $missExit)"; $failures++ }
  $missLock = $null
  if (Test-Path $cloneLock) { try { $missLock = (Get-Content -Raw $cloneLock | ConvertFrom-Json) } catch {} }
  if ($missLock) { & pwsh -NoProfile -File (Join-Path $clone 'bin/state-lock.ps1') -Release -Token ([string]$missLock.token) -Model 'test-model' | Out-Null }

  # Finding 8a: a partially staged contract/door set is in-sync on disk but drifted in the commit
  # snapshot; the staged-blob check must catch what the working-tree check cannot.
  $cp2Contract = Join-Path $clone 'protocol/CONTRACT.md'
  $contractMut = (Get-Content -Raw $cp2Contract) -replace 'How to start/end a session:', 'CP2 staged-drift marker. How to start/end a session:'
  [IO.File]::WriteAllText($cp2Contract, $contractMut, (New-Object Text.UTF8Encoding($false)))
  & pwsh -NoProfile -File (Join-Path $clone 'bin/sync-doors.ps1') | Out-Null
  & git -C $clone add protocol/CONTRACT.md
  & pwsh -NoProfile -File (Join-Path $clone 'bin/sync-doors.ps1') -Check | Out-Null
  $treeExit = $LASTEXITCODE
  & pwsh -NoProfile -File (Join-Path $clone 'bin/sync-doors.ps1') -Check -Staged | Out-Null
  $stagedExit = $LASTEXITCODE
  if ($treeExit -eq 0 -and $stagedExit -eq 1) { Write-Host '[PASS] staged-blob door check catches drift the working-tree check misses' }
  else { Write-Host "[FAIL] staged door-drift detection (tree $treeExit, staged $stagedExit)"; $failures++ }
  $env:BRAIN_MODEL = 'test-model'
  try {
    $driftCommit = @(& git -C $clone commit -m 'cp2 drift attempt' 2>&1)
    $driftExit = $LASTEXITCODE
  } finally {
    if ($null -eq $oldModel) { Remove-Item Env:\BRAIN_MODEL -ErrorAction SilentlyContinue }
    else { $env:BRAIN_MODEL = $oldModel }
  }
  if ($driftExit -ne 0 -and (($driftCommit | Out-String) -match 'STAGED contract/door set')) {
    Write-Host '[PASS] pre-commit blocks the partially staged contract/door commit'
  } else { $driftCommit | Out-Host; Write-Host "[FAIL] staged-drift pre-commit gate (exit $driftExit)"; $failures++ }
  & git -C $clone reset -q HEAD -- . 2>$null
  & git -C $clone reset --hard -q

  # Finding 8b: the enforcement helpers are lock-protected coordination files.
  $coordListText = Get-Content -Raw (Join-Path $clone 'protocol/coordination-files.txt')
  $helpersMissing = @('.githooks/pre-push', 'bin/remote-identity.ps1', 'bin/secret-audit.ps1' |
    Where-Object { $coordListText -notmatch [regex]::Escape($_) })
  if (-not $helpersMissing.Count) { Write-Host '[PASS] pre-push/remote-identity/secret-audit are on the coordination allowlist' }
  else { Write-Host "[FAIL] coordination allowlist missing: $($helpersMissing -join ', ')"; $failures++ }
  Add-Content -LiteralPath (Join-Path $clone 'bin/secret-audit.ps1') -Value '# cp2 battery touch'
  & git -C $clone add bin/secret-audit.ps1
  $env:BRAIN_MODEL = 'test-model'
  try {
    $helperCommit = @(& git -C $clone commit -m 'cp2 helper attempt' 2>&1)
    $helperExit = $LASTEXITCODE
  } finally {
    if ($null -eq $oldModel) { Remove-Item Env:\BRAIN_MODEL -ErrorAction SilentlyContinue }
    else { $env:BRAIN_MODEL = $oldModel }
  }
  if ($helperExit -ne 0 -and (($helperCommit | Out-String) -match 'STATE\.lock is not held')) {
    Write-Host '[PASS] committing an enforcement helper without the lock is blocked'
  } else { $helperCommit | Out-Host; Write-Host "[FAIL] enforcement-helper lock gate (exit $helperExit)"; $failures++ }
  & git -C $clone reset --hard -q

  # CP2 identity-pin integrity: generic remote identities preserve path case and fold only
  # scheme/host — two Unix-hosted paths differing by case must never share a pin.
  . (Join-Path $clone 'bin/remote-identity.ps1')
  $idUpper = Get-RemoteIdentity 'ssh://git@host.example/srv/Repo.git'
  $idLower = Get-RemoteIdentity 'ssh://git@host.example/srv/repo.git'
  $idHostFold = Get-RemoteIdentity 'SSH://GIT@HOST.EXAMPLE/srv/Repo.git'
  $idScpUpper = Get-RemoteIdentity 'git@Host.example:Srv/Repo.git'
  $idScpHostFold = Get-RemoteIdentity 'git@host.example:Srv/Repo.git'
  $idScpLower = Get-RemoteIdentity 'git@host.example:srv/repo.git'
  if ($idUpper -cne $idLower -and $idUpper -ceq $idHostFold -and
      $idScpUpper -ceq $idScpHostFold -and $idScpUpper -cne $idScpLower) {
    Write-Host '[PASS] generic remote pins preserve path case and fold only scheme/host'
  } else { Write-Host "[FAIL] remote-identity case handling ($idUpper / $idLower / $idHostFold / $idScpUpper / $idScpHostFold / $idScpLower)"; $failures++ }
  # ...and the approval comparison itself must be case-sensitive, or preserved case re-conflates.
  $pinActual = (& git -C $clone config --local --get brain.approvedRemote)
  & git -C $clone config --local brain.approvedRemote $pinActual.ToUpperInvariant()
  Expect-Exit 'approval pin comparison is case-sensitive' 1 {
    & pwsh -NoProfile -File (Join-Path $clone 'bin/remote-privacy.ps1') -RequireApproved -ConfirmPrivateRemote
  }
  & git -C $clone config --local brain.approvedRemote $pinActual

  # Parked sandbox trap: -Also names a file inside a fully-untracked directory, which plain
  # porcelain collapses into a single 'dir/' entry.
  New-Item -ItemType Directory -Path (Join-Path $clone 'cp2-newdir') | Out-Null
  Set-Content -LiteralPath (Join-Path $clone 'cp2-newdir/deep-note.md') -Value 'cp2 deep also'
  $env:BRAIN_MODEL = 'test-model'
  try {
    $deepOut = @(& pwsh -NoProfile -File (Join-Path $clone 'bin/close-session.ps1') -NoHandoff -Also 'cp2-newdir/deep-note.md' 2>&1)
    $deepExit = $LASTEXITCODE
  } finally {
    if ($null -eq $oldModel) { Remove-Item Env:\BRAIN_MODEL -ErrorAction SilentlyContinue }
    else { $env:BRAIN_MODEL = $oldModel }
  }
  if ($deepExit -eq 0 -and @(& git -C $clone ls-files cp2-newdir/deep-note.md).Count -eq 1 -and
      (($deepOut | Out-String) -notmatch 'matched no pending change')) {
    Write-Host '[PASS] -Also stages a file inside a fully-untracked directory'
  } else { $deepOut | Out-Host; Write-Host "[FAIL] untracked-dir -Also (exit $deepExit)"; $failures++ }

  # Branch invariant holds for local-only closes too.
  & git -C $localClone checkout -q -b cp2-side
  $env:BRAIN_MODEL = 'test-model'
  try {
    Expect-Exit 'local-only close refuses a non-main branch' 1 {
      & pwsh -NoProfile -File (Join-Path $localClone 'bin/close-session.ps1') -NoHandoff
    }
  } finally {
    if ($null -eq $oldModel) { Remove-Item Env:\BRAIN_MODEL -ErrorAction SilentlyContinue }
    else { $env:BRAIN_MODEL = $oldModel }
  }
  & git -C $localClone checkout -q main

  # Finding 6: a private-remote memory that loses origin must degrade LOUDLY — privacy names
  # MISSING_ORIGIN, doctor hard-fails, startup blocks, close refuses with durable pending-sync.
  & git -C $clone remote remove origin
  $moPrivacy = @(& pwsh -NoProfile -File (Join-Path $clone 'bin/remote-privacy.ps1') 2>&1)
  $moPrivacyExit = $LASTEXITCODE
  if ($moPrivacyExit -eq 1 -and (($moPrivacy | Out-String) -match 'REMOTE_PRIVACY_STATUS=MISSING_ORIGIN')) {
    Write-Host '[PASS] privacy reports MISSING_ORIGIN instead of LOCAL_ONLY when the pin is set'
  } else { $moPrivacy | Out-Host; Write-Host "[FAIL] MISSING_ORIGIN privacy status (exit $moPrivacyExit)"; $failures++ }
  $moDoctor = @(& pwsh -NoProfile -File (Join-Path $clone 'bin/brain-doctor.ps1') -HardOnly 2>&1)
  $moDoctorExit = $LASTEXITCODE
  if ($moDoctorExit -eq 1 -and (($moDoctor | Out-String) -match 'origin is missing')) {
    Write-Host '[PASS] doctor hard-fails a private-remote memory with no origin'
  } else { $moDoctor | Out-Host; Write-Host "[FAIL] doctor MISSING_ORIGIN hard gate (exit $moDoctorExit)"; $failures++ }
  $moStart = @(& pwsh -NoProfile -File (Join-Path $clone 'bin/start-session.ps1') -Model 'test-model' 2>&1)
  if ($LASTEXITCODE -eq 1 -and (($moStart | Out-String) -match 'STARTUP_STATUS=BLOCKED')) {
    Write-Host '[PASS] startup reports BLOCKED while the private remote is missing'
  } else { Write-Host '[FAIL] startup stayed READY despite the missing private remote'; $failures++ }
  Add-Content -LiteralPath $statePath -Value "`nCP2 missing-origin battery marker."
  $env:BRAIN_MODEL = 'test-model'
  try {
    $moClose = @(& pwsh -NoProfile -File (Join-Path $clone 'bin/close-session.ps1') -NoHandoff 2>&1)
    $moCloseExit = $LASTEXITCODE
  } finally {
    if ($null -eq $oldModel) { Remove-Item Env:\BRAIN_MODEL -ErrorAction SilentlyContinue }
    else { $env:BRAIN_MODEL = $oldModel }
  }
  if ($moCloseExit -eq 1 -and (($moClose | Out-String) -match 'origin is missing') -and
      (& git -C $clone config --local --get brain.pendingSync) -eq 'true') {
    Write-Host '[PASS] close refuses the degraded memory and records durable pending-sync'
  } else { $moClose | Out-Host; Write-Host "[FAIL] missing-origin close refusal (exit $moCloseExit)"; $failures++ }
  & git -C $clone remote add origin $bareA
  & git -C $clone config --local --unset-all brain.pendingSync 2>$null
  & git -C $clone checkout -q -- STATE.md
  Expect-Exit 'doctor recovers after the origin is restored' 0 {
    & pwsh -NoProfile -File (Join-Path $clone 'bin/brain-doctor.ps1') -HardOnly
  }
} finally {
  if (Test-Path -LiteralPath $sandbox) { Remove-Item -LiteralPath $sandbox -Recurse -Force }
}

if ($failures) { Write-Host "private-onboarding: $failures failure(s)"; exit 1 }
Write-Host 'private-onboarding: PASS'
