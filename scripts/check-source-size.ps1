# Keep production code, examples and tests within the same module-size budget.
[CmdletBinding()]
param(
    [string]$SourceRoot
)

$ErrorActionPreference = 'Stop'
if (-not $SourceRoot) {
    $SourceRoot = Join-Path (Join-Path $PSScriptRoot '..') 'crates'
}
$maximumLines = 500
$files = @(Get-ChildItem -LiteralPath $SourceRoot -Recurse -File -Filter '*.rs')
if ($files.Count -eq 0) {
    throw "No Rust source files found in $SourceRoot"
}

$violations = @(
    foreach ($file in $files) {
        $lineCount = 0
        foreach ($line in [System.IO.File]::ReadLines($file.FullName)) {
            $lineCount++
        }
        if ($lineCount -gt $maximumLines) {
            '{0}: {1} lines (maximum {2})' -f $file.FullName, $lineCount, $maximumLines
        }
    }
)

if ($violations.Count -gt 0) {
    $violations | ForEach-Object { Write-Host $_ }
    throw 'Split oversized Rust files into cohesive responsibility-based modules.'
}

Write-Host "Source size check passed: $($files.Count) Rust files, maximum $maximumLines lines each."
