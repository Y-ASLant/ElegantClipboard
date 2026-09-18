param(
    [Parameter(Mandatory = $true)]
    [int]$ProcessId,
    [Parameter(Mandatory = $true)]
    [string]$OutputPath,
    [ValidateRange(1, 604800)]
    [int]$DurationSeconds = 28800,
    [ValidateRange(1, 3600)]
    [int]$IntervalSeconds = 60
)

$ErrorActionPreference = 'Stop'
if (-not $IsWindows) {
    throw 'GPUI 常驻采样仅支持 Windows'
}

$process = Get-Process -Id $ProcessId -ErrorAction Stop
if ([IO.Path]::GetFileName($process.Path) -ne 'elegant-clipboard-gpui.exe') {
    throw "进程 $ProcessId 不是 GPUI 应用"
}
$startTime = $process.StartTime
$output = [IO.Path]::GetFullPath($OutputPath)
if (Test-Path -LiteralPath $output) {
    throw "输出文件已存在：$output"
}
$directory = [IO.Path]::GetDirectoryName($output)
New-Item -ItemType Directory -Path $directory -Force | Out-Null

$deadline = (Get-Date).AddSeconds($DurationSeconds)
$samples = 0
do {
    $process = Get-Process -Id $ProcessId -ErrorAction SilentlyContinue
    if ($null -eq $process -or $process.StartTime -ne $startTime) {
        throw "GPUI 进程在采样期间退出；已记录 $samples 个样本：$output"
    }
    [pscustomobject]@{
        time_utc = (Get-Date).ToUniversalTime().ToString('o')
        process_id = $ProcessId
        private_bytes = $process.PrivateMemorySize64
        working_set_bytes = $process.WorkingSet64
        handles = $process.HandleCount
        threads = $process.Threads.Count
        cpu_seconds = $process.CPU
    } | Export-Csv -LiteralPath $output -NoTypeInformation -Append
    $samples++
    $remaining = ($deadline - (Get-Date)).TotalSeconds
    if ($remaining -gt 0) {
        Start-Sleep -Seconds ([Math]::Min($IntervalSeconds, $remaining))
    }
} while ((Get-Date) -lt $deadline)

Write-Output "已记录 $samples 个样本：$output"
