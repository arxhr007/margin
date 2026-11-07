# Margin installer for Windows.
#
#   irm https://raw.githubusercontent.com/arxhr007/margin/main/scripts/install.ps1 | iex
#
# Options are environment variables, set them before the command above:
#   $env:MARGIN_AUTOSTART = 1     also start Margin when you sign in
#   $env:MARGIN_UNINSTALL = 1     remove Margin (your settings in %APPDATA%\Margin are kept)
#   $env:MARGIN_VERSION   = 'v0.4.0'   install a specific release instead of the latest
#   $env:MARGIN_INSTALL_DIR = '...'    install somewhere else (default %LOCALAPPDATA%\Programs\Margin)
#
# What it does: downloads margin.exe from the GitHub release, checks its SHA-256, copies it to your user
# folder (no administrator rights needed), adds a Start Menu shortcut, and starts it. A file fetched this way
# carries no "downloaded from the internet" mark, so SmartScreen does not warn about it.

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'   # the progress bar makes Invoke-WebRequest very slow in Windows PowerShell 5.1
try { [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12 } catch {}

$repo = 'arxhr007/margin'
$dir = if ($env:MARGIN_INSTALL_DIR) { $env:MARGIN_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA 'Programs\Margin' }
$exe = Join-Path $dir 'margin.exe'
$shortcut = Join-Path ([Environment]::GetFolderPath('Programs')) 'Margin.lnk'
$runKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'

function Stop-Margin {
    Get-Process -Name margin -ErrorAction SilentlyContinue | Stop-Process -Force -ErrorAction SilentlyContinue
    Start-Sleep -Milliseconds 500
}

if ($env:MARGIN_UNINSTALL) {
    Write-Host 'Removing Margin...'
    Stop-Margin
    Remove-Item -LiteralPath $shortcut -Force -ErrorAction SilentlyContinue
    Remove-ItemProperty -Path $runKey -Name Margin -ErrorAction SilentlyContinue
    Remove-Item -LiteralPath $dir -Recurse -Force -ErrorAction SilentlyContinue
    Write-Host 'Done. Your settings in %APPDATA%\Margin were left alone; delete that folder to remove them too.'
    return
}

if ($env:MARGIN_BASE_URL) {
    $base = $env:MARGIN_BASE_URL            # for testing against a local server
} elseif ($env:MARGIN_VERSION) {
    $base = "https://github.com/$repo/releases/download/$($env:MARGIN_VERSION)"
} else {
    $base = "https://github.com/$repo/releases/latest/download"
}

$tmp = Join-Path ([IO.Path]::GetTempPath()) ("margin-install-" + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
    Write-Host 'Downloading margin.exe ...'
    $download = Join-Path $tmp 'margin.exe'
    try {
        Invoke-WebRequest -UseBasicParsing -Uri "$base/margin.exe" -OutFile $download
    } catch {
        throw "Download failed: $base/margin.exe  ($($_.Exception.Message))"
    }

    # checksum (the release publishes margin.exe.sha256 next to the exe)
    $sumFile = Join-Path $tmp 'margin.exe.sha256'
    $haveSum = $true
    try { Invoke-WebRequest -UseBasicParsing -Uri "$base/margin.exe.sha256" -OutFile $sumFile } catch { $haveSum = $false }
    if ($haveSum) {
        $expected = ((Get-Content -LiteralPath $sumFile -Raw).Trim() -split '\s+')[0].ToLowerInvariant()
        $actual = (Get-FileHash -LiteralPath $download -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($expected -ne $actual) { throw 'Checksum mismatch: the download is corrupt or has been tampered with.' }
        Write-Host 'Checksum OK.'
    } else {
        Write-Warning 'No checksum file in this release; skipping the checksum check.'
    }

    Stop-Margin
    New-Item -ItemType Directory -Force -Path $dir | Out-Null
    Copy-Item -LiteralPath $download -Destination $exe -Force
    Unblock-File -LiteralPath $exe -ErrorAction SilentlyContinue
    Write-Host "Installed $exe"

    $sh = New-Object -ComObject WScript.Shell
    $link = $sh.CreateShortcut($shortcut)
    $link.TargetPath = $exe
    $link.WorkingDirectory = $dir
    $link.Description = 'Margin: touchpad edge and corner controls'
    $link.Save()

    if ($env:MARGIN_AUTOSTART) {
        New-ItemProperty -Path $runKey -Name Margin -Value "`"$exe`"" -PropertyType String -Force | Out-Null
        Write-Host 'Margin will start when you sign in.'
    }

    Start-Process -FilePath $exe
    Write-Host ''
    Write-Host 'Margin is installed and running. Look for its icon in the notification area (click ^ next to the clock).'
    Write-Host 'Left-click the icon for settings. To uninstall:  $env:MARGIN_UNINSTALL = 1; irm https://raw.githubusercontent.com/arxhr007/margin/main/scripts/install.ps1 | iex'
} finally {
    Remove-Item -LiteralPath $tmp -Recurse -Force -ErrorAction SilentlyContinue
}
