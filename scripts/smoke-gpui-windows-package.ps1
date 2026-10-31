param(
    [Parameter(Mandatory = $true)]
    [string]$Archive,
    [Parameter(Mandatory = $true)]
    [string]$Executable
)

$ErrorActionPreference = 'Stop'
if (-not $IsWindows) {
    throw 'Windows package smoke verification must run on Windows.'
}
$archivePath = (Resolve-Path -LiteralPath $Archive).Path
$builtExecutable = (Resolve-Path -LiteralPath $Executable).Path
$temporary = Join-Path ([IO.Path]::GetTempPath()) ('elegant-zip-smoke-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $temporary | Out-Null
try {
    $unpacked = Join-Path $temporary 'unpacked'
    Expand-Archive -LiteralPath $archivePath -DestinationPath $unpacked
    $applications = @($builtExecutable, (Join-Path $unpacked 'elegant-clipboard-gpui.exe'))
    for ($index = 0; $index -lt $applications.Count; $index++) {
        $application = (Resolve-Path -LiteralPath $applications[$index]).Path
        $data = Join-Path $temporary "data-$index"
        $stdout = Join-Path $temporary "stdout-$index.log"
        $stderr = Join-Path $temporary "stderr-$index.log"
        # Smoke mode disables capture and exits after rendering the native window.
        $process = Start-Process -FilePath $application -ArgumentList @('--smoke-test', '--data-dir', "`"$data`"") -PassThru -RedirectStandardOutput $stdout -RedirectStandardError $stderr
        try {
            if (-not $process.WaitForExit(60000)) {
                $process.Kill($true)
                $process.WaitForExit()
                throw "Native window smoke timed out: $application"
            }
            $process.WaitForExit()
            if ($process.ExitCode -ne 0) {
                throw "Native window smoke exited with code $($process.ExitCode): $application"
            }
        } finally {
            Get-Content -LiteralPath $stdout, $stderr
            $process.Dispose()
        }
    }
    Write-Output "Built and unpacked Windows application startup passed: $archivePath"
} finally {
    Remove-Item -LiteralPath $temporary -Recurse -Force
}
