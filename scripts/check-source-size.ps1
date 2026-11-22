# Per-file budget: 640 hard max, 560 warn. Split by cohesive responsibility; see docs/README.md.
[CmdletBinding()]
param(
    [string]$SourceRoot,
    [int]$WarnAt = 560
)

$ErrorActionPreference = 'Stop'
if (-not $SourceRoot) {
    $SourceRoot = Join-Path (Join-Path $PSScriptRoot '..') 'crates'
}
$maximumLines = 640
$files = @(Get-ChildItem -LiteralPath $SourceRoot -Recurse -File -Filter '*.rs')
if ($files.Count -eq 0) {
    throw "No Rust source files found in $SourceRoot"
}

$violations = @()
$warnings = @()
foreach ($file in $files) {
    $lineCount = 0
    foreach ($line in [System.IO.File]::ReadLines($file.FullName)) {
        $lineCount++
    }
    if ($lineCount -gt $maximumLines) {
        $violations += '{0}: {1} lines (maximum {2})' -f $file.FullName, $lineCount, $maximumLines
    } elseif ($lineCount -ge $WarnAt) {
        $warnings += '{0}: {1} lines (warns at {2})' -f $file.FullName, $lineCount, $WarnAt
    }
}

$warnings | ForEach-Object { Write-Warning $_ }

if ($violations.Count -gt 0) {
    $violations | ForEach-Object { Write-Host $_ }
    throw 'Split oversized Rust files by cohesive responsibility (not line-count slices). See docs/README.md.'
}

Write-Host "Source size check passed: $($files.Count) Rust files, maximum $maximumLines lines each."
