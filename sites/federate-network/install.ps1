# Federate Network installer (Windows)
#
#   iex (irm https://federate.network/install.ps1)
#
# What it does, in order:
#   1. downloads the federate CLI binary (x86_64)
#   2. installs it to %LOCALAPPDATA%\Federate\bin and adds it to your PATH
#   3. runs `federate setup` elevated (UAC prompt):
#        - local verifying DNS resolver as a boot task (127.0.0.1)
#          answering every TLD in the signed root zone, present and future
#        - system DNS pointed at it (previous settings saved for uninstall)
#        - fed:// links registered to open in your browser
#        - live self-test (resolve + fetch home.fed)
#
# Undo everything (admin terminal): federate dns uninstall; federate handler uninstall

$ErrorActionPreference = "Stop"

$base = "https://federate.network/dl"
$url = "$base/federate-windows-x86_64.zip"
$dir = Join-Path $env:LOCALAPPDATA "Federate\bin"

if ([System.Environment]::Is64BitOperatingSystem -eq $false) {
    Write-Error "unsupported architecture: 32-bit Windows"
}

Write-Host "[..] downloading $url"
New-Item -ItemType Directory -Force -Path $dir | Out-Null
$zip = Join-Path $env:TEMP "federate.zip"
Invoke-WebRequest -Uri $url -OutFile $zip -UseBasicParsing
Expand-Archive -Path $zip -DestinationPath $dir -Force
Remove-Item $zip

# PATH (user scope), only once
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($userPath -notlike "*$dir*") {
    [Environment]::SetEnvironmentVariable("Path", "$userPath;$dir", "User")
    Write-Host "[ok] added $dir to your PATH (new terminals pick it up)"
}

$exe = Join-Path $dir "federate.exe"

Write-Host "[..] running machine setup (UAC prompt)"
$p = Start-Process -FilePath $exe -ArgumentList "setup" -Verb RunAs -Wait -PassThru
if ($p.ExitCode -ne 0) {
    Write-Error "federate setup failed (exit $($p.ExitCode)); run again from an Administrator terminal: federate setup"
}

Write-Host ""
Write-Host "[ok] Federate Network installed. Open http://home.fed"
