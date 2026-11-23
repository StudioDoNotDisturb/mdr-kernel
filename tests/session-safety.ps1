#requires -Version 7.0
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$sandbox = Join-Path ([IO.Path]::GetTempPath()) "mdr-session-safety-$([guid]::NewGuid().ToString('N'))"
$clone = Join-Path $sandbox 'clone'
$failures = 0
$savedEnvironment = @{}
foreach ($name in 'BRAIN_MODEL','BRAIN_LOCK_TOKEN','BRAIN_OBJECTIVE','MDR_FORCE_LIVE_LOCK','GIT_DIR','GIT_WORK_TREE','GIT_INDEX_FILE') {
  $savedEnvironment[$name] = [Environment]::GetEnvironmentVariable($name, 'Process')
  Remove-Item -LiteralPath "Env:\$name" -ErrorAction SilentlyContinue
}
$originalLocation = Get-Location
function Assert-True([string]$Name, [bool]$Condition, [string]$Detail = '') {
  if ($Condition) { Write-Host "[PASS] $Name" }
  else { Write-Host "[FAIL] $Name $Detail"; $script:failures++ }
}
function Run-Lock([string[]]$LockArgs) {
  $output = @(& pwsh -NoProfile -File (Join-Path $clone 'bin/state-lock.ps1') @LockArgs 2>&1)
  [pscustomobject]@{ Exit = $LASTEXITCODE; Text = $output | Out-String }
}
function Run-Close([string[]]$CloseArgs) {
  $output = @(& pwsh -NoProfile -File (Join-Path $clone 'bin/close-session.ps1') @CloseArgs 2>&1)
  [pscustomobject]@{ Exit = $LASTEXITCODE; Text = $output | Out-String }
}
function Write-Utf8([string]$Path, [string]$Text) { [IO.File]::WriteAllText($Path, $Text, [Text.UTF8Encoding]::new($false)) }
try {
  New-Item -ItemType Directory -Path $clone | Out-Null
  # Copy the candidate snapshot, never the caller's Git history, remotes or live lock. This works
  # from both a clean export and the template directory without cloning the private source repo.
  Get-ChildItem -LiteralPath $root -Force | Where-Object { $_.Name -notin @('.git','.brain-root','STATE.lock') } |
    Copy-Item -Destination $clone -Recurse -Force
  # MDR_TEST_ENGINE names a built binary: the hooks then run the native engine, and every
  # assertion below must hold for it exactly as for the shell gates. Without it, test the shell.
  $engine = $env:MDR_TEST_ENGINE
  Remove-Item -Path (Join-Path $clone 'bin/mdr-kernel*') -Force -ErrorAction SilentlyContinue
  if ($engine) { Copy-Item -LiteralPath $engine -Destination (Join-Path $clone "bin/mdr-kernel$(if ($IsWindows) { '.exe' })") }
  & git -C $clone init -q -b main
  & git -C $clone config user.name 'session safety test'
  & git -C $clone config user.email 'session-safety@example.invalid'
  & git -C $clone config core.autocrlf false
  & git -C $clone -c core.hooksPath=.no-hooks add -A
  & git -C $clone -c core.hooksPath=.no-hooks commit -qm 'Isolated test snapshot'
  if ($LASTEXITCODE) { throw 'could not initialize disposable fixture' }
  & git -C $clone config brain.model 'test-model'
  $lockPath = Join-Path $clone 'STATE.lock'
  $acquired = Run-Lock @('-Acquire','-Model','test-model','-Objective','isolated lifecycle test')
  $token = if ($acquired.Text -match '(?m)^session token:\s*(\S+)') { $Matches[1] } else { '' }
  Assert-True 'acquire prints a fresh session proof' ($acquired.Exit -eq 0 -and [bool]$token) $acquired.Text
  if (-not $token) { throw 'fixture acquire failed' }
  $sealedBody = Get-Content -Raw -LiteralPath $lockPath
  $sealedHash = (Get-FileHash -LiteralPath $lockPath).Hash
  foreach ($action in '-Acquire','-Heartbeat','-Release','-CheckOwner') {
    $result = Run-Lock @($action,'-Token',$token)
    Assert-True "$action refuses config identity without changing lock" ($result.Exit -eq 1 -and (Get-FileHash -LiteralPath $lockPath).Hash -eq $sealedHash) $result.Text
    $result = Run-Lock @($action,'-Model','test-model','-Token','wrong-token')
    Assert-True "$action refuses another same-model session" ($result.Exit -eq 1 -and (Get-FileHash -LiteralPath $lockPath).Hash -eq $sealedHash) $result.Text
  }
  $result = Run-Lock @('-CheckOwner','-Model','test-model','-Token',$token)
  Assert-True 'CheckOwner verifies explicit identity and our acquired token' ($result.Exit -eq 0 -and $result.Text -match 'LOCK_OWNER=VERIFIED')
  $tokenless = $sealedBody | ConvertFrom-Json
  $tokenless.PSObject.Properties.Remove('token')
  Write-Utf8 $lockPath ($tokenless | ConvertTo-Json)
  $tokenlessHash = (Get-FileHash -LiteralPath $lockPath).Hash
  foreach ($action in '-Acquire','-Heartbeat','-Release','-CheckOwner') {
    $result = Run-Lock @($action,'-Model','test-model','-Token',$token)
    Assert-True "$action refuses tokenless legacy ownership" ($result.Exit -eq 1 -and (Get-FileHash -LiteralPath $lockPath).Hash -eq $tokenlessHash) $result.Text
  }
  $otherHost = $sealedBody | ConvertFrom-Json
  $otherHost.host = 'unrelated-fixture-host'
  Write-Utf8 $lockPath ($otherHost | ConvertTo-Json)
  $result = Run-Lock @('-CheckOwner','-Model','test-model','-Token',$token)
  Assert-True 'matching token does not override a different recorded host' ($result.Exit -eq 1)
  Write-Utf8 $lockPath $sealedBody
  $env:MDR_FORCE_LIVE_LOCK = '1'
  $result = Run-Lock @('-Release','-Force','-Model','test-model')
  Assert-True 'live recovery valve alone cannot prove abandonment' ($result.Exit -eq 1 -and (Test-Path -LiteralPath $lockPath))
  Remove-Item Env:\MDR_FORCE_LIVE_LOCK
  $result = Run-Lock @('-Release','-Force','-Model','test-model','-AbandonmentReason','Fixture claims abandonment of a live holder')
  Assert-True 'recovery reason alone cannot take a fresh-heartbeat lock without the live valve' ($result.Exit -eq 1 -and $result.Text -match 'presumed ALIVE' -and (Get-FileHash -LiteralPath $lockPath).Hash -eq $sealedHash) $result.Text
  $stale = $sealedBody | ConvertFrom-Json
  $stale.heartbeat_at = [DateTimeOffset]::UtcNow.AddHours(-3).ToString('o')
  Write-Utf8 $lockPath ($stale | ConvertTo-Json)
  $staleHash = (Get-FileHash -LiteralPath $lockPath).Hash
  $result = Run-Lock @('-Acquire','-Model','test-model','-Token','another-session-token')
  Assert-True 'old-but-live same-model holder is not taken over by acquire' ($result.Exit -eq 1 -and $result.Text -match 'abandonment is NOT PROVEN' -and (Get-FileHash -LiteralPath $lockPath).Hash -eq $staleHash) $result.Text
  $result = Run-Lock @('-Release','-Force','-Model','test-model')
  Assert-True 'stale heartbeat alone cannot authorize forced release' ($result.Exit -eq 1 -and $result.Text -match 'AbandonmentReason must record' -and (Get-FileHash -LiteralPath $lockPath).Hash -eq $staleHash) $result.Text
  $result = Run-Lock @('-Release','-Force','-Model','test-model','-AbandonmentReason','Test harness ended its isolated fixture operation')
  Assert-True 'explicit independent recovery evidence permits stale recovery' ($result.Exit -eq 0 -and -not (Test-Path -LiteralPath $lockPath))
  Write-Utf8 $lockPath ''
  $result = Run-Lock @('-CheckOwner','-Model','test-model','-Token',$token)
  Assert-True 'empty lock never proves ownership' ($result.Exit -eq 1)
  $result = Run-Lock @('-Release','-Force','-Model','test-model','-AbandonmentReason','Test harness created this empty fixture lock')
  Assert-True 'unreadable lock can be recovered only by explicit evidence' ($result.Exit -eq 0 -and -not (Test-Path -LiteralPath $lockPath))

  $shellCommand = Get-Command sh -ErrorAction SilentlyContinue
  $shell = if ($shellCommand) { $shellCommand.Source } elseif ($IsWindows) {
    Join-Path (Split-Path -Parent (Split-Path -Parent (Get-Command git).Source)) 'bin/sh.exe'
  } else { '/bin/sh' }
  if (-not (Test-Path -LiteralPath $shell)) { throw 'Git hook shell was not found' }
  Set-Location $clone
  # Run the actual shell gate with an empty PATH: missing utilities must fail before any
  # scan is mistaken for a clean result. Then remove one helper under an otherwise normal PATH.
  $missingTool = @(& $shell -c 'PATH=/mdr-nonexistent-fixture-path; export PATH; . .githooks/pre-commit' 2>&1)
  Assert-True 'pre-commit fails closed when a required utility is absent' ($LASTEXITCODE -eq 1 -and ($missingTool | Out-String) -match 'required utility')
  $toolBin = Join-Path $sandbox 'gate-tools'
  New-Item -ItemType Directory -Path $toolBin | Out-Null
  foreach ($tool in 'git','grep','sed','awk','head','tr','cut','dirname') {
    Write-Utf8 (Join-Path $toolBin $tool) "#!/bin/sh`nexit 99`n"
  }
  & $shell -c 'chmod +x "$1"/*' -- ($toolBin.Replace('\','/'))
  $missingPwsh = @(& $shell -c 'gate_path=$(cd "$1" && pwd); PATH="$gate_path"; export PATH; . .githooks/pre-commit' -- ($toolBin.Replace('\','/')) 2>&1)
  Assert-True 'pre-commit specifically refuses unavailable PowerShell' ($LASTEXITCODE -eq 1 -and ($missingPwsh | Out-String) -match "required utility 'pwsh'") ($missingPwsh | Out-String)
  $helper = Join-Path $clone 'bin/write-approval.ps1'
  $helperText = Get-Content -Raw -LiteralPath $helper
  Remove-Item -LiteralPath $helper
  $missingHelper = @(& $shell '.githooks/pre-commit' 2>&1)
  Assert-True 'pre-commit fails closed when a gate helper is absent' ($LASTEXITCODE -eq 1 -and ($missingHelper | Out-String) -match 'required gate')
  Write-Utf8 $helper $helperText
  & git restore -- bin/write-approval.ps1

  $messagePath = Join-Path $sandbox 'commit-message.txt'
  Write-Utf8 $messagePath "Fixture commit`n"
  $messageHash = (Get-FileHash -LiteralPath $messagePath).Hash
  $attribution = @(& $shell '.githooks/prepare-commit-msg' $messagePath 2>&1)
  Assert-True 'config-only attribution refuses without modifying the message' ($LASTEXITCODE -eq 1 -and (Get-FileHash -LiteralPath $messagePath).Hash -eq $messageHash)
  $env:BRAIN_MODEL = 'test-model'
  & $shell '.githooks/prepare-commit-msg' $messagePath
  & $shell '.githooks/prepare-commit-msg' $messagePath
  Assert-True 'explicit session identity adds exactly one trailer' ($LASTEXITCODE -eq 0 -and @((Get-Content $messagePath) | Where-Object { $_ -eq 'Brain-Model: test-model' }).Count -eq 1)
  Write-Utf8 $messagePath "Fixture commit`n`nBrain-Model: another-model`n"
  $attribution = @(& $shell '.githooks/prepare-commit-msg' $messagePath 2>&1)
  Assert-True 'explicit identity and trailer disagreement refuses' ($LASTEXITCODE -eq 1)
  Remove-Item Env:\BRAIN_MODEL
  & $shell '.githooks/prepare-commit-msg' $messagePath
  Assert-True 'explicit message trailer alone remains supported' ($LASTEXITCODE -eq 0)
  foreach ($badTrailer in "Brain-Model: `n", "Brain-Model: unknown`n", "Brain-Model: one`nBrain-Model: two`n") {
    Write-Utf8 $messagePath "Fixture commit`n`n$badTrailer"
    $attribution = @(& $shell '.githooks/prepare-commit-msg' $messagePath 2>&1)
    Assert-True 'blank, unknown or duplicate attribution refuses' ($LASTEXITCODE -eq 1)
  }
  Write-Utf8 $messagePath "Fixture commit`n"
  $env:BRAIN_MODEL = '  test-model  '
  & $shell '.githooks/prepare-commit-msg' $messagePath
  Assert-True 'BRAIN_MODEL whitespace is trimmed in the trailer' ($LASTEXITCODE -eq 0 -and @((Get-Content $messagePath) | Where-Object { $_ -ceq 'Brain-Model: test-model' }).Count -eq 1)
  Write-Utf8 $messagePath "Fixture commit`n"
  $messageHash = (Get-FileHash -LiteralPath $messagePath).Hash
  $env:BRAIN_MODEL = 'unknown'
  $attribution = @(& $shell '.githooks/prepare-commit-msg' $messagePath 2>&1)
  Assert-True 'BRAIN_MODEL=unknown is refused without modifying the message' ($LASTEXITCODE -eq 1 -and (Get-FileHash -LiteralPath $messagePath).Hash -eq $messageHash) ($attribution | Out-String)
  Remove-Item Env:\BRAIN_MODEL

  $bootstrapBefore = (& git status --porcelain --untracked-files=all | Out-String)
  $bootstrap = @(& pwsh -NoProfile -File (Join-Path $clone 'bin/bootstrap-clone.ps1') -CommitScaffold 2>&1)
  Assert-True 'bootstrap refuses config identity before creating scaffold or lock' ($LASTEXITCODE -eq 1 -and (& git status --porcelain --untracked-files=all | Out-String) -eq $bootstrapBefore -and -not (Test-Path -LiteralPath $lockPath)) ($bootstrap | Out-String)
  Write-Utf8 (Join-Path $clone 'bootstrap-foreign.txt') 'Another session staged this file.'
  & git add -- bootstrap-foreign.txt
  $bootstrapIndex = (& git write-tree).Trim()
  $bootstrapHead = (& git rev-parse HEAD).Trim()
  $bootstrap = @(& pwsh -NoProfile -File (Join-Path $clone 'bin/bootstrap-clone.ps1') -Model 'test-model' -CommitScaffold 2>&1)
  Assert-True 'bootstrap preserves and refuses a preexisting foreign index' ($LASTEXITCODE -eq 1 -and (& git write-tree).Trim() -eq $bootstrapIndex -and (& git rev-parse HEAD).Trim() -eq $bootstrapHead -and -not (Test-Path -LiteralPath $lockPath)) ($bootstrap | Out-String)
  & git restore --staged -- bootstrap-foreign.txt
  Remove-Item -LiteralPath (Join-Path $clone 'bootstrap-foreign.txt')

  Write-Utf8 (Join-Path $clone 'setup-foreign.txt') 'Another session owns this untracked file.'
  $setup = @(& pwsh -NoProfile -File (Join-Path $clone 'bin/setup-private.ps1') -LocalOnly -Model 'test-model' 2>&1)
  if ($LASTEXITCODE) { throw "local-only fixture setup failed:`n$($setup | Out-String)" }
  $scaffoldSet = @('STATE.md','DECISIONS.md','handoffs/INDEX.md','reference/INDEX.md','specs/INDEX.md','inbox/README.md',
    'protocol/recall-gates.json','CLAUDE.md','AGENTS.md','GEMINI.md','protocol/advisory.md','protocol/decision-authority.json',
    'protocol/rulings.json','maps/node-map.json','maps/NODE-MAP.md','maps/OBSIDIAN-NODE-LINKS.md')
  $scaffoldCommit = @(& git show --pretty=format: --name-only HEAD | Where-Object { $_ })
  Assert-True 'scaffold commit holds only scaffold outputs' ($scaffoldCommit.Count -gt 0 -and @($scaffoldCommit | Where-Object { $_ -notin $scaffoldSet }).Count -eq 0) ($scaffoldCommit -join ', ')
  Assert-True 'scaffold commit leaves a foreign untracked file untracked' ((Test-Path -LiteralPath (Join-Path $clone 'setup-foreign.txt')) -and -not (& git ls-files -- setup-foreign.txt))
  # Bootstrap reruns: a caller that already owns the lock keeps it; a leftover token that proves
  # nothing still lets bootstrap take and return a free lock; stray untracked files are no error.
  $acquired = Run-Lock @('-Acquire','-Model','test-model','-Objective','bootstrap ownership fixture')
  $ownToken = if ($acquired.Text -match '(?m)^session token:\s*(\S+)') { $Matches[1] } else { '' }
  $env:BRAIN_LOCK_TOKEN = $ownToken
  $bootstrap = @(& pwsh -NoProfile -File (Join-Path $clone 'bin/bootstrap-clone.ps1') -Model 'test-model' -CommitScaffold 2>&1)
  $bootstrapExit = $LASTEXITCODE
  $result = Run-Lock @('-CheckOwner','-Model','test-model','-Token',$ownToken)
  Assert-True 'bootstrap keeps a lock its caller already owned' ($bootstrapExit -eq 0 -and $result.Exit -eq 0) ($bootstrap | Out-String)
  $released = Run-Lock @('-Release','-Model','test-model','-Token',$ownToken)
  $env:BRAIN_LOCK_TOKEN = 'leftover-token-from-a-finished-session'
  $bootstrap = @(& pwsh -NoProfile -File (Join-Path $clone 'bin/bootstrap-clone.ps1') -Model 'test-model' -CommitScaffold 2>&1)
  Assert-True 'bootstrap with a leftover token acquires and releases only its own lock' ($LASTEXITCODE -eq 0 -and -not (Test-Path -LiteralPath $lockPath)) ($bootstrap | Out-String)
  Remove-Item Env:\BRAIN_LOCK_TOKEN
  Remove-Item -LiteralPath (Join-Path $clone 'setup-foreign.txt')
  Assert-True 'close fixture has no remote destination' (-not (& git remote))
  $fixtureHead = (& git rev-parse HEAD).Trim()
  $acquired = Run-Lock @('-Acquire','-Model','test-model','-Objective','coordination hook fixture')
  $hookToken = if ($acquired.Text -match '(?m)^session token:\s*(\S+)') { $Matches[1] } else { '' }
  $hookBody = Get-Content -Raw -LiteralPath $lockPath
  Add-Content -LiteralPath (Join-Path $clone 'STATE.md') -Value "`nCoordination hook fixture."
  & git add -- STATE.md
  $env:BRAIN_LOCK_TOKEN = $hookToken
  $hook = @(& $shell '.githooks/pre-commit' 2>&1)
  Assert-True 'coordination hook refuses config-only identity even with correct token' ($LASTEXITCODE -eq 1 -and ($hook | Out-String) -match 'explicit BRAIN_MODEL') ($hook | Out-String)
  $env:BRAIN_MODEL = 'test-model'
  Remove-Item Env:\BRAIN_LOCK_TOKEN
  $hook = @(& $shell '.githooks/pre-commit' 2>&1)
  Assert-True 'coordination hook refuses missing session proof' ($LASTEXITCODE -eq 1 -and ($hook | Out-String) -match 'BRAIN_LOCK_TOKEN') ($hook | Out-String)
  $tokenlessHook = $hookBody | ConvertFrom-Json
  $tokenlessHook.PSObject.Properties.Remove('token')
  Write-Utf8 $lockPath ($tokenlessHook | ConvertTo-Json)
  $env:BRAIN_LOCK_TOKEN = $hookToken
  $hook = @(& $shell '.githooks/pre-commit' 2>&1)
  Assert-True 'coordination hook refuses a tokenless lock record' ($LASTEXITCODE -eq 1 -and ($hook | Out-String) -match 'BRAIN_LOCK_TOKEN') ($hook | Out-String)
  Write-Utf8 $lockPath $hookBody
  $hook = @(& $shell '.githooks/pre-commit' 2>&1)
  Assert-True 'coordination hook accepts only verified session ownership' ($LASTEXITCODE -eq 0 -and ($hook | Out-String) -match 'LOCK_OWNER=VERIFIED') ($hook | Out-String)
  $env:BRAIN_MODEL = 'other-model'
  $hook = @(& $shell '.githooks/pre-commit' 2>&1)
  Assert-True 'coordination hook refuses the right token under a different model' ($LASTEXITCODE -eq 1 -and ($hook | Out-String) -match "held by 'test-model', not 'other-model'") ($hook | Out-String)
  $env:BRAIN_MODEL = 'test-model'
  $released = Run-Lock @('-Release','-Model','test-model','-Token',$hookToken)
  Remove-Item Env:\BRAIN_LOCK_TOKEN
  $commit = @(& git commit -qm 'Coordination commit without a lock' 2>&1)
  Assert-True 'real commit of a coordination file without the lock is refused' ($LASTEXITCODE -ne 0 -and ($commit | Out-String) -match 'STATE.lock is not held' -and (& git rev-parse HEAD).Trim() -eq $fixtureHead) ($commit | Out-String)
  & git restore --staged --worktree -- STATE.md
  Write-Utf8 (Join-Path $clone 'ordinary.txt') 'Ordinary non-coordination work.'
  & git add -- ordinary.txt
  $commit = @(& git commit -qm "Ordinary commit without a lock`n`nLeast-file: fixture file for the ordinary-commit test" 2>&1)
  Assert-True 'real ordinary commit needs no lock and gets one trailer' ($LASTEXITCODE -eq 0 -and @(& git log -1 --format=%B | Where-Object { $_ -ceq 'Brain-Model: test-model' }).Count -eq 1) ($commit | Out-String)
  Remove-Item Env:\BRAIN_MODEL
  & git rm -q -- ordinary.txt
  $commit = @(& git commit -qm 'Commit with no identity at all' 2>&1)
  Assert-True 'real commit with neither BRAIN_MODEL nor trailer is refused' ($LASTEXITCODE -ne 0 -and ($commit | Out-String) -match 'set BRAIN_MODEL or include') ($commit | Out-String)
  $commit = @(& git commit -qm "Remove ordinary fixture`n`nBrain-Model: test-model" 2>&1)
  Assert-True 'real commit with only an explicit message trailer is accepted' ($LASTEXITCODE -eq 0) ($commit | Out-String)
  if ($engine) {
    Write-Utf8 (Join-Path $clone 'untrailed.txt') 'A new file with no Least-file trailer.'
    & git add -- untrailed.txt
    $commit = @(& git commit -qm "Add a file without a trailer`n`nBrain-Model: test-model" 2>&1)
    Assert-True 'engine: a new file without a Least-file trailer is refused' ($LASTEXITCODE -ne 0 -and ($commit | Out-String) -match 'Least-file:') ($commit | Out-String)
    & git rm -q --cached -- untrailed.txt
    Remove-Item -LiteralPath (Join-Path $clone 'untrailed.txt')
    Add-Content -LiteralPath (Join-Path $clone 'native/src/main.rs') -Value '// drift'
    & git add -- native/src/main.rs
    $hook = @(& $shell '.githooks/pre-commit' 2>&1)
    Assert-True 'engine: a binary built from other sources refuses to guard the commit' ($LASTEXITCODE -eq 1 -and ($hook | Out-String) -match 'different native/ sources') ($hook | Out-String)
    $env:MDR_ENGINE = 'legacy'
    $hook = @(& $shell '.githooks/pre-commit' 2>&1)
    Assert-True 'engine: MDR_ENGINE=legacy runs the shell gates instead' ($LASTEXITCODE -eq 0) ($hook | Out-String)
    Remove-Item Env:\MDR_ENGINE
    & git restore --staged --worktree -- native/src/main.rs
  }
  $fixtureHead = (& git rev-parse HEAD).Trim()
  Write-Utf8 (Join-Path $clone 'foreign.txt') 'Another session owns this change.'
  & git add -- foreign.txt
  $indexBefore = (& git write-tree).Trim()
  $result = Run-Close @('-NoHandoff','-Model','test-model')
  Assert-True 'close refuses foreign staging without unstaging or committing it' ($result.Exit -eq 1 -and $result.Text -match 'outside this close set' -and (& git write-tree).Trim() -eq $indexBefore -and (& git rev-parse HEAD).Trim() -eq $fixtureHead -and -not (Test-Path -LiteralPath $lockPath) -and $result.Text -notmatch 'acquired STATE.lock') $result.Text
  & git restore --staged -- foreign.txt
  $result = Run-Close @('-NoHandoff','-Model','test-model')
  Assert-True 'foreign-only dirty tree creates no artificial map commit' ($result.Exit -eq 0 -and (& git rev-parse HEAD).Trim() -eq $fixtureHead -and (Test-Path -LiteralPath (Join-Path $clone 'foreign.txt'))) $result.Text
  Assert-True 'close leaves a foreign untracked file unstaged and untracked' (-not (& git ls-files -- foreign.txt) -and -not @(& git diff --cached --name-only).Count)
  Write-Utf8 (Join-Path $clone 'inbox/foreign-note.md') 'Another session owns this inbox note.'
  Write-Utf8 (Join-Path $clone 'owned one.txt') 'First owned change.'
  Write-Utf8 (Join-Path $clone 'owned two.txt') 'Second owned change.'
  $result = Run-Close @('-NoHandoff','-Model','test-model','-Also','"owned one.txt", "owned two.txt"','-LeastFile','owned fixture files for the Also test')
  $committed = @(& git show --pretty=format: --name-only HEAD)
  Assert-True 'quoted comma-separated Also paths both reach the checkpoint' ($result.Exit -eq 0 -and $committed -contains 'owned one.txt' -and $committed -contains 'owned two.txt') $result.Text
  Assert-True 'unowned inbox and unrelated work remain outside close' ($committed -notcontains 'inbox/foreign-note.md' -and $committed -notcontains 'foreign.txt' -and (Test-Path -LiteralPath (Join-Path $clone 'inbox/foreign-note.md')))
  Remove-Item -LiteralPath (Join-Path $clone 'foreign.txt'), (Join-Path $clone 'inbox/foreign-note.md')

  $date = (Get-Date).ToUniversalTime().ToString('yyyy-MM-dd')
  $handoff = Join-Path $clone "handoffs/$date-dry-run.md"
  Write-Utf8 $handoff "Title: Dry run fixture`nType: Handoff`nContext: Tests ownership before handoff repair.`n`n## SYSTEM NODE LINKS`n`nDirectory Path: handoffs/`nNext Sequential File: none`nRelated Workflows: none`nAI Execution Prompts: none`n"
  Add-Content -LiteralPath (Join-Path $clone 'handoffs/INDEX.md') -Value "`n$date-dry-run.md"
  $handoffHash = (Get-FileHash -LiteralPath $handoff).Hash
  $result = Run-Close @('-Topic','dry-run','-Model','test-model','-DryRun')
  Assert-True 'DryRun never repairs a handoff or leaves a lock' ($result.Exit -eq 0 -and (Get-FileHash -LiteralPath $handoff).Hash -eq $handoffHash -and -not (Test-Path -LiteralPath $lockPath)) $result.Text
  $acquired = Run-Lock @('-Acquire','-Model','test-model','-Objective','other isolated session')
  $otherToken = if ($acquired.Text -match '(?m)^session token:\s*(\S+)') { $Matches[1] } else { '' }
  $result = Run-Close @('-Topic','dry-run','-Model','test-model')
  Assert-True 'unproven owner cannot repair handoff before close refuses' ($result.Exit -eq 1 -and $result.Text -match 'LOCK_OWNER=UNPROVEN' -and (Get-FileHash -LiteralPath $handoff).Hash -eq $handoffHash) $result.Text
  $released = Run-Lock @('-Release','-Model','test-model','-Token',$otherToken)
  Assert-True 'test holder releases its own lock by acquired proof' ($released.Exit -eq 0)
  Write-Utf8 (Join-Path $clone 'staged-foreign.txt') 'Another session staged this.'
  & git add -- staged-foreign.txt
  $result = Run-Close @('-Topic','dry-run','-Model','test-model')
  Assert-True 'foreign staging refuses before handoff repair or lock acquire' ($result.Exit -eq 1 -and $result.Text -match 'outside this close set' -and (Get-FileHash -LiteralPath $handoff).Hash -eq $handoffHash -and -not (Test-Path -LiteralPath $lockPath)) $result.Text
  & git restore --staged -- staged-foreign.txt
  $headBefore = (& git rev-parse HEAD).Trim()
  $result = Run-Close @('-Topic','dry-run','-Model','test-model')
  $committed = @(& git show --pretty=format: --name-only HEAD | Where-Object { $_ })
  $outside = @($committed | Where-Object { $_ -notin @("handoffs/$date-dry-run.md", 'handoffs/INDEX.md') -and $_ -notlike 'maps/*' })
  Assert-True 'handoff close commits exactly the close set and releases the lock' ($result.Exit -eq 0 -and (& git rev-parse HEAD).Trim() -ne $headBefore -and $committed -contains "handoffs/$date-dry-run.md" -and $committed -contains 'handoffs/INDEX.md' -and -not $outside.Count -and -not (Test-Path -LiteralPath $lockPath)) ($result.Text + ($committed -join ', '))
  Assert-True 'handoff close leaves foreign untracked work in place' ((Test-Path -LiteralPath (Join-Path $clone 'staged-foreign.txt')) -and -not (& git ls-files -- staged-foreign.txt))
  Remove-Item -LiteralPath (Join-Path $clone 'staged-foreign.txt')
} finally {
  Set-Location $originalLocation
  foreach ($name in $savedEnvironment.Keys) {
    if ($null -eq $savedEnvironment[$name]) { Remove-Item -LiteralPath "Env:\$name" -ErrorAction SilentlyContinue }
    else { [Environment]::SetEnvironmentVariable($name, $savedEnvironment[$name], 'Process') }
  }
  $resolvedSandbox = [IO.Path]::GetFullPath($sandbox)
  $tempRoot = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd([IO.Path]::DirectorySeparatorChar) + [IO.Path]::DirectorySeparatorChar
  if (-not $resolvedSandbox.StartsWith($tempRoot, [StringComparison]::OrdinalIgnoreCase) -or (Split-Path -Leaf $resolvedSandbox) -notlike 'mdr-session-safety-*') { throw 'refusing cleanup outside the unique test sandbox' }
  if (Test-Path -LiteralPath $resolvedSandbox) { Remove-Item -LiteralPath $resolvedSandbox -Recurse -Force }
}
Write-Host "session-safety: $failures failure(s)."
exit ([int]($failures -gt 0))
