# bump-version.ps1 — Unified version bump for ElegantClipboard
# Usage: .\scripts\bump-version.ps1 0.5.0

param(
    [Parameter(Mandatory=$true, Position=0)]
    [ValidatePattern('^\d+\.\d+\.\d+$')]
    [string]$Version
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot

Write-Host "Bumping version to $Version ..." -ForegroundColor Cyan

# 1. package.json
$pkgPath = Join-Path $root "package.json"
$pkg = Get-Content $pkgPath -Raw | ConvertFrom-Json
$oldVersion = $pkg.version
$pkg.version = $Version
$pkg | ConvertTo-Json -Depth 10 | Set-Content $pkgPath -Encoding utf8NoBOM
Write-Host "  package.json: $oldVersion -> $Version" -ForegroundColor Green

# 2. src-tauri/tauri.conf.json
$tauriPath = Join-Path $root "src-tauri\tauri.conf.json"
$tauri = Get-Content $tauriPath -Raw | ConvertFrom-Json
$tauri.version = $Version
$tauri | ConvertTo-Json -Depth 10 | Set-Content $tauriPath -Encoding utf8NoBOM
Write-Host "  tauri.conf.json: -> $Version" -ForegroundColor Green

# 3. src-tauri/Cargo.toml (regex replace to preserve formatting)
$cargoPath = Join-Path $root "src-tauri\Cargo.toml"
$cargo = Get-Content $cargoPath -Raw
$cargo = $cargo -replace '(?m)^(version\s*=\s*")[^"]*(")', "`${1}$Version`${2}"
Set-Content $cargoPath $cargo -Encoding utf8NoBOM -NoNewline
Write-Host "  Cargo.toml: -> $Version" -ForegroundColor Green

Write-Host ""
Write-Host "Done! Version set to $Version in all 3 files." -ForegroundColor Cyan
Write-Host "Next: git add -A && git commit -m 'chore: bump version to $Version'" -ForegroundColor DarkGray
