# Build model-downloader, install it for the current user, and add a Desktop
# shortcut. Run from anywhere:
#
#   powershell -ExecutionPolicy Bypass -File scripts\install.ps1          # install
#   powershell -ExecutionPolicy Bypass -File scripts\install.ps1 -Clean   # install, then delete the build folder
#
# Re-run it after pulling changes to update the installed copy.

param(
    # Delete target\ after installing (frees ~1.5 GB; the next build starts from scratch).
    [switch]$Clean
)

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$installDir = Join-Path $env:LOCALAPPDATA 'Programs\ModelDownloader'
$exe = Join-Path $installDir 'model-downloader.exe'

Write-Host 'Building release...'
cargo build --release --manifest-path (Join-Path $repo 'Cargo.toml')
if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }

New-Item -ItemType Directory -Force -Path $installDir | Out-Null
Copy-Item -Force (Join-Path $repo 'target\release\model-downloader.exe') $exe
Write-Host "Installed: $exe"

# The icon is embedded in the .exe, so the shortcut just points at it.
$desktop = [Environment]::GetFolderPath('Desktop')
$link = Join-Path $desktop 'Model Downloader.lnk'
$shell = New-Object -ComObject WScript.Shell
$shortcut = $shell.CreateShortcut($link)
$shortcut.TargetPath = $exe
$shortcut.WorkingDirectory = $installDir
$shortcut.IconLocation = "$exe,0"
$shortcut.Description = 'Download AI reasoning models to an external drive'
$shortcut.Save()
Write-Host "Shortcut:  $link"

if ($Clean) {
    cargo clean --manifest-path (Join-Path $repo 'Cargo.toml')
    Write-Host 'Removed the build folder (target\).'
}
