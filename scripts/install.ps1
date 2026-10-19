# Installs the desktop app; command-line lighting controls are tracked in #16.
[CmdletBinding()]
param(
    [string]$Version = 'latest',
    [switch]$DownloadOnly,
    [string]$Directory = (Get-Location).Path
)
$ErrorActionPreference = 'Stop'
if ($env:OS -ne 'Windows_NT') { throw 'Use install.sh on macOS or Linux.' }
$architecture = if ($env:PROCESSOR_ARCHITEW6432) { $env:PROCESSOR_ARCHITEW6432 } else { $env:PROCESSOR_ARCHITECTURE }
if ($architecture -ne 'AMD64') { throw "No desktop package for Windows/$architecture. Windows x64 is currently supported." }
[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
$scratch = Join-Path ([IO.Path]::GetTempPath()) ('dynamiclight-install-' + [guid]::NewGuid())
$null = New-Item -ItemType Directory -Path $scratch
$releases = 'https://github.com/mskutin/DynamicLight/releases'
$asset = 'dynamiclight-desktop-windows-x64-setup.exe'
try {
    if ($Version -eq 'latest') {
        $versionFile = Join-Path $scratch 'version.txt'
        Invoke-WebRequest -UseBasicParsing -Uri "$releases/latest/download/release-version.txt" -OutFile $versionFile
        $Version = (Get-Content -LiteralPath $versionFile -Raw).Trim()
    }
    $Version = $Version -creplace '^v', ''
    if ($Version -cnotmatch '^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(-[0-9A-Za-z]+([.-][0-9A-Za-z]+)*)?$') { throw 'Invalid release version.' }
    $base = "$releases/download/v$Version"
    $checksums = Join-Path $scratch 'SHA256SUMS'
    Invoke-WebRequest -UseBasicParsing -Uri "$base/SHA256SUMS" -OutFile $checksums
    $entries = @(Get-Content -LiteralPath $checksums | Where-Object { $_ -cmatch ('^[0-9a-f]{64}  ' + [regex]::Escape($asset) + '$') })
    if ($entries.Count -ne 1) { throw "Missing or ambiguous SHA-256 for $asset." }
    $expected = $entries[0].Substring(0, 64)
    $installer = Join-Path $scratch $asset
    Invoke-WebRequest -UseBasicParsing -Uri "$base/$asset" -OutFile $installer
    if ((Get-FileHash -LiteralPath $installer -Algorithm SHA256).Hash.ToLowerInvariant() -cne $expected) {
        throw 'SHA-256 mismatch; nothing installed.'
    }
    if ($DownloadOnly) {
        $null = New-Item -ItemType Directory -Force -Path $Directory
        Copy-Item -LiteralPath $installer -Destination (Join-Path $Directory $asset)
        Write-Host "Verified DynamicLight $Version downloaded to $Directory"
    } else {
        # Tauri NSIS: silent per-user install. Omit /R so the app is not launched.
        $process = Start-Process -FilePath $installer -ArgumentList '/S' -Wait -PassThru -WindowStyle Hidden
        if ($process.ExitCode -notin @(0, 3010)) { throw "Installer exited with code $($process.ExitCode)." }
        Write-Host "Installed DynamicLight $Version. Open it from the Start menu."
        if ($process.ExitCode -eq 3010) { Write-Host 'Windows needs a restart to finish installation.' }
    }
} finally {
    Remove-Item -LiteralPath $scratch -Recurse -Force
}
