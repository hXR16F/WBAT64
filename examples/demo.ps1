# A short tour of wbat64 from PowerShell.
# The UI is drawn on the console, so the pipeline only captures the result lines.
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$dialogs = Join-Path $here 'WBAT.TXT'

$out = & wbat64 box "@$($dialogs):signup" --emit ps1
$rc = $LASTEXITCODE
if ($rc -ge 100) {
    Write-Host 'cancelled'
    exit 1
}
Invoke-Expression ($out -join "`n")
"Hello $env:name from $env:city (over 21: $env:WCB1, newsletter: $env:WCB2)"
