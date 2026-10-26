param(
    [Parameter(Mandatory = $true)]
    [string]$Image
)

$ErrorActionPreference = 'Stop'
if (-not $IsMacOS) {
    throw 'DMG smoke verification must run on macOS.'
}
$imagePath = (Resolve-Path -LiteralPath $Image).Path
& hdiutil verify $imagePath
if ($LASTEXITCODE -ne 0) { throw 'Disk image verification failed.' }

$temporary = Join-Path ([IO.Path]::GetTempPath()) ('elegant-dmg-smoke-' + [guid]::NewGuid().ToString('N'))
$mountPoint = Join-Path $temporary 'volume'
New-Item -ItemType Directory -Path $mountPoint -Force | Out-Null
$mounted = $false
try {
    & hdiutil attach -readonly -nobrowse -mountpoint $mountPoint $imagePath
    if ($LASTEXITCODE -ne 0) { throw 'Unable to mount the disk image.' }
    $mounted = $true
    $app = Join-Path $mountPoint 'ElegantClipboard.app'
    $shortcut = & readlink (Join-Path $mountPoint 'Applications')
    if ($LASTEXITCODE -ne 0 -or $shortcut -ne '/Applications') {
        throw 'The disk image must include a shortcut to /Applications.'
    }
    & codesign --verify --deep --strict $app
    if ($LASTEXITCODE -ne 0) { throw 'Packaged application signature verification failed.' }
    $executable = Join-Path $app 'Contents/MacOS/elegant-clipboard-gpui'
    & $executable --smoke-test --data-dir (Join-Path $temporary 'data')
    if ($LASTEXITCODE -ne 0) { throw 'Packaged application window smoke failed.' }
    Write-Output "DMG mount, Applications shortcut, signature and window smoke passed: $imagePath"
} finally {
    if ($mounted) {
        & hdiutil detach $mountPoint
        if ($LASTEXITCODE -ne 0) {
            # Do not recursively clean a directory that still contains a mounted volume.
            throw "Unable to detach disk image; temporary directory retained: $temporary"
        }
    }
    Remove-Item -LiteralPath $temporary -Recurse -Force
}
