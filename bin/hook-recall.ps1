#requires -Version 7.0
<#
.SYNOPSIS
  UserPromptSubmit adapter: bounded typed input, complete recall, visible degradation.
.DESCRIPTION
  Reads a JSON object with string user_input or prompt on stdin. Healthy CLEAR writes nothing;
  a match adds compact context. Errors continue the prompt with an explicit fallback request.
  Configuration and these adapter tests do not prove a particular app actually fires its hook.
#>
[CmdletBinding()]
param()
$ErrorActionPreference = 'Stop'
function Exit-Degraded([string]$Reason) {
  @{ hookSpecificOutput = @{
    hookEventName = 'UserPromptSubmit'
    additionalContext = "RECALL_HOOK_STATUS=DEGRADED reason=$Reason; automatic recall was not evaluated. Before acting run pwsh -NoProfile -File bin/prepare-work.ps1 -Objective '<operator objective>' (or pipe the complete objective with -FromStdin)."
  } } | ConvertTo-Json -Depth 4 -Compress
  exit 0
}

$child = $null
$childStarted = $false
try {
  # A 131072-character prompt can occupy 786432 JSON characters when escaped as \uXXXX.
  # Read only one character beyond this 1 MiB payload budget, never an unbounded ReadToEnd.
  # Hosts send UTF-8 JSON. Decode it explicitly; [Console]::In uses the OEM code page on Windows.
  $utf8 = [Text.UTF8Encoding]::new($false)
  $buffer = [char[]]::new(1048577)
  $length = [IO.StreamReader]::new([Console]::OpenStandardInput(), $utf8, $true).ReadBlock($buffer, 0, $buffer.Length)
  if ($length -gt 1048576) { Exit-Degraded 'payload-too-large' }
  if ($length -eq 0) { Exit-Degraded 'empty-input' }
  try { $payload = [string]::new($buffer, 0, $length) | ConvertFrom-Json -NoEnumerate -ErrorAction Stop }
  catch { Exit-Degraded 'invalid-json' }
  if ($null -eq $payload -or $payload.GetType() -ne [System.Management.Automation.PSCustomObject]) { Exit-Degraded 'payload-not-object' }
  $promptText = $payload.user_input
  if ($null -eq $promptText -or ($promptText -is [string] -and [string]::IsNullOrWhiteSpace($promptText))) { $promptText = $payload.prompt }
  if ($promptText -isnot [string]) { Exit-Degraded 'prompt-not-string' }
  if ([string]::IsNullOrWhiteSpace($promptText)) { Exit-Degraded 'missing-prompt' }
  if ($promptText.Length -gt 131072) { Exit-Degraded 'prompt-too-large' }

  $matcher = Join-Path $PSScriptRoot 'brain-recall.ps1'
  if (-not (Test-Path -LiteralPath $matcher -PathType Leaf)) { Exit-Degraded 'missing-matcher' }
  $start = [Diagnostics.ProcessStartInfo]::new()
  $start.FileName = Join-Path $PSHOME $(if ($IsWindows) { 'pwsh.exe' } else { 'pwsh' })
  $start.UseShellExecute = $false
  $start.CreateNoWindow = $true
  $start.RedirectStandardInput = $true
  $start.RedirectStandardOutput = $true
  $start.RedirectStandardError = $true
  $start.StandardInputEncoding = $utf8
  foreach ($arg in @('-NoProfile', '-NonInteractive', '-File', $matcher, '-FromStdin', '-Brief')) { $start.ArgumentList.Add($arg) }
  $child = [Diagnostics.Process]::new()
  $child.StartInfo = $start
  $null = $child.Start()
  $childStarted = $true
  $outputTask = $child.StandardOutput.ReadToEndAsync()
  $errorTask = $child.StandardError.ReadToEndAsync()
  # Write asynchronously as well: a broken matcher that never reads must not block the hook.
  $writeTask = $child.StandardInput.WriteAsync($promptText)
  $clock = [Diagnostics.Stopwatch]::StartNew()
  if (-not $writeTask.Wait(15000)) { $child.Kill($true); Exit-Degraded 'matcher-timeout' }
  $child.StandardInput.Close()
  $remaining = [Math]::Max(0, 15000 - [int]$clock.ElapsedMilliseconds)
  if (-not $child.WaitForExit($remaining)) { $child.Kill($true); Exit-Degraded 'matcher-timeout' }
  $banner = $outputTask.GetAwaiter().GetResult().Trim()
  $diagnostic = $errorTask.GetAwaiter().GetResult()
  if ($child.ExitCode -ne 0 -or -not [string]::IsNullOrWhiteSpace($diagnostic)) { Exit-Degraded 'matcher-failed' }
  if (-not $banner) { exit 0 }
  if ($banner.Length -gt 1400 -or -not $banner.StartsWith('RECALL_STATUS=MATCHED ')) { Exit-Degraded 'invalid-matcher-output' }
  @{ hookSpecificOutput = @{ hookEventName = 'UserPromptSubmit'; additionalContext = $banner } } | ConvertTo-Json -Depth 4 -Compress
} catch {
  # Do not copy exception/payload text into prompt context; it can contain private user words.
  if ($childStarted -and -not $child.HasExited) { try { $child.Kill($true) } catch { } }
  Exit-Degraded 'adapter-error'
} finally {
  if ($child) { $child.Dispose() }
}
