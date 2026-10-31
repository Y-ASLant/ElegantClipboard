param(
    [Parameter(Mandatory = $true)]
    [string]$LogName,
    [Parameter(Mandatory = $true)]
    [string]$Program,
    [Parameter(Mandatory = $true)]
    [string[]]$CommandArgs
)

$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
$logDirectory = Join-Path $PSScriptRoot '../target/ci-logs'
New-Item -ItemType Directory -Path $logDirectory -Force | Out-Null
$log = Join-Path $logDirectory "$LogName.log"
& $Program @CommandArgs 2>&1 | Tee-Object -FilePath $log
$code = $LASTEXITCODE
Set-Content -LiteralPath (Join-Path $logDirectory "$LogName.exit-code") -Value $code
exit $code
