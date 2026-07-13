#requires -Version 7.0

function Get-RemoteIdentity {
  [CmdletBinding()]
  param([Parameter(Mandatory)][string]$RemoteUrl)

  $normalizedUrl = $RemoteUrl.Trim()
  if ($normalizedUrl -match '(?i)^(?:https?://github\.com/|ssh://git@github\.com/|git@github\.com:)(?<repo>[^/]+/[^/]+?)(?:\.git)?/?$') {
    # GitHub coordinates are host-defined case-insensitive; folding them is safe and canonical.
    return "github:$($Matches.repo.TrimEnd('/').ToLowerInvariant())"
  }
  # Generic remotes: normalize ONLY the scheme and host. Path components on Unix-hosted remotes
  # are case-sensitive, and lowercasing a full URL can conflate two distinct destinations into
  # one identity pin — a privacy-pin integrity hole, not a cosmetic issue. Anything that cannot
  # be parsed confidently (local paths, exotic URLs) is preserved verbatim: both the approval pin
  # and every later check run through this same function, so preservation always stays
  # self-consistent and a casing mismatch fails CLOSED (unapproved), never open.
  $urlForm = [regex]::Match($normalizedUrl, '^(?<scheme>[A-Za-z][A-Za-z0-9+.-]*://)(?<host>[^/]+)(?<rest>/.*)?$')
  if ($urlForm.Success) {
    return "remote:$($urlForm.Groups['scheme'].Value.ToLowerInvariant())$($urlForm.Groups['host'].Value.ToLowerInvariant())$($urlForm.Groups['rest'].Value.TrimEnd('/'))"
  }
  # scp-like user@host:path — exclude single-letter Windows drive prefixes (C:\ or C:/).
  $scpForm = [regex]::Match($normalizedUrl, '^(?<user>[^@/:\\]+@)?(?<host>[^:/\\]{2,}):(?<rest>.*)$')
  if ($scpForm.Success) {
    return "remote:$($scpForm.Groups['user'].Value.ToLowerInvariant())$($scpForm.Groups['host'].Value.ToLowerInvariant()):$($scpForm.Groups['rest'].Value.TrimEnd('/'))"
  }
  return "remote:$($normalizedUrl.TrimEnd('/'))"
}
