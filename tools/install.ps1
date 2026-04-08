# Almide installer for Windows — downloads a prebuilt binary from GitHub Releases.
#
# Usage:
#   irm https://raw.githubusercontent.com/almide/almide/main/tools/install.ps1 | iex
#   .\install.ps1 v0.12.3                                    # specific version
#   $env:ALMIDE_INSTALL = "$HOME\bin"; .\install.ps1          # custom install dir

$ErrorActionPreference = 'Stop'

$Repo = "almide/almide"
$InstallDir = if ($env:ALMIDE_INSTALL) { $env:ALMIDE_INSTALL } else { "$env:USERPROFILE\.local\bin" }
$Version = if ($args.Count -gt 0) { $args[0] } else { "latest" }

$Archive = "almide-windows-x86_64.zip"

if ($Version -eq "latest") {
    $Base = "https://github.com/$Repo/releases/latest/download"
} else {
    $Base = "https://github.com/$Repo/releases/download/$Version"
}

$Url = "$Base/$Archive"
$ChecksumUrl = "$Base/almide-checksums.sha256"

# --- Download ---

$Tmp = Join-Path ([System.IO.Path]::GetTempPath()) "almide-install-$([System.Guid]::NewGuid())"
New-Item -ItemType Directory -Path $Tmp -Force | Out-Null

try {
    Write-Host "Downloading $Archive..."
    try {
        Invoke-WebRequest -Uri $Url -OutFile "$Tmp\$Archive" -UseBasicParsing
    } catch {
        Write-Host "error: download failed" -ForegroundColor Red
        Write-Host "       check that the version exists: https://github.com/$Repo/releases"
        exit 1
    }

    Invoke-WebRequest -Uri $ChecksumUrl -OutFile "$Tmp\checksums.sha256" -UseBasicParsing

    # --- Verify checksum ---

    Write-Host "Verifying checksum..."
    $Checksums = Get-Content "$Tmp\checksums.sha256"
    $Line = $Checksums | Where-Object { $_ -match [regex]::Escape($Archive) }
    $Expected = ($Line -split '\s+')[0].ToLower()
    $Actual = (Get-FileHash "$Tmp\$Archive" -Algorithm SHA256).Hash.ToLower()

    if ($Expected -ne $Actual) {
        Write-Host "error: checksum mismatch" -ForegroundColor Red
        Write-Host "       expected: $Expected"
        Write-Host "       got:      $Actual"
        exit 1
    }

    # --- Install ---

    Expand-Archive -Path "$Tmp\$Archive" -DestinationPath "$Tmp\extracted" -Force
    New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
    Copy-Item "$Tmp\extracted\almide-windows-x86_64\almide.exe" "$InstallDir\almide.exe" -Force

    Write-Host ""
    Write-Host "Installed almide to $InstallDir\almide.exe"
    & "$InstallDir\almide.exe" --version

    # --- PATH check ---

    $UserPath = [Environment]::GetEnvironmentVariable('PATH', 'User')
    if ($UserPath -notlike "*$InstallDir*") {
        Write-Host ""
        Write-Host "To add almide to your PATH, run:"
        Write-Host ""
        Write-Host "  [Environment]::SetEnvironmentVariable('PATH', '$InstallDir;' + [Environment]::GetEnvironmentVariable('PATH', 'User'), 'User')"
        Write-Host ""
        Write-Host "Then restart your terminal."
    }
} finally {
    Remove-Item -Recurse -Force $Tmp -ErrorAction SilentlyContinue
}
