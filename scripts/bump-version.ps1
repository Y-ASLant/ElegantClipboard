param(
    [Parameter(Mandatory = $true, Position = 0)]
    [ValidatePattern('^\d+\.\d+\.\d+$')]
    [string]$Version
)

$ErrorActionPreference = 'Stop'
$root = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
$cargoPath = Join-Path $root 'Cargo.toml'
$cargo = Get-Content -LiteralPath $cargoPath -Raw
$pattern = '(?m)(^\[workspace\.package\]\r?\nversion\s*=\s*")[^"]+("\s*$)'
$match = [regex]::Match($cargo, $pattern)
if (-not $match.Success) {
    throw '无法定位 Cargo 工作区版本'
}

$updated = [regex]::Replace($cargo, $pattern, "`${1}$Version`${2}", 1)
if ($updated -ne $cargo) {
    Set-Content -LiteralPath $cargoPath -Value $updated -Encoding utf8NoBOM -NoNewline
}
Push-Location $root
try {
    & cargo check --workspace
    if ($LASTEXITCODE -ne 0) {
        throw "Cargo 锁文件更新或检查失败：$LASTEXITCODE"
    }
}
finally {
    Pop-Location
}

Write-Output "Cargo 工作区版本已更新为 $Version"
