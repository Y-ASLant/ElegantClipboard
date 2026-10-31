# Run with PowerShell 7 on the platform being packaged.
$ErrorActionPreference = 'Stop'
if (-not ($IsMacOS -or $IsLinux)) {
    throw 'Build macOS and Linux packages on their respective operating systems.'
}
if ($env:CARGO_BUILD_TARGET) {
    throw 'Native packaging requires CARGO_BUILD_TARGET to be unset.'
}
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
Push-Location $root
try {
    $hostLine = & rustc -vV | Where-Object { $_ -like 'host: *' }
    if ($LASTEXITCODE -ne 0) { throw 'Unable to determine the Rust host.' }
    $hostTarget = $hostLine.Substring(6)
    if (($IsMacOS -and $hostTarget -notmatch '^(aarch64|x86_64)-apple-darwin$') -or
        ($IsLinux -and $hostTarget -notmatch '^(aarch64|x86_64)-unknown-linux-gnu$')) {
        throw "Unsupported native package target: $hostTarget"
    }
    & cargo build -p elegant-clipboard-gpui --release --locked
    if ($LASTEXITCODE -ne 0) { throw 'Release build failed.' }
    $metadataJson = & cargo metadata --no-deps --format-version 1 --locked
    if ($LASTEXITCODE -ne 0) { throw 'Cargo metadata failed.' }
    $metadata = $metadataJson | ConvertFrom-Json
    $package = @($metadata.packages | Where-Object name -eq 'elegant-clipboard-gpui')
    if ($package.Count -ne 1) { throw 'Unable to determine application version.' }
    $version = $package[0].version
    $executable = Join-Path $metadata.target_directory 'release/elegant-clipboard-gpui'
    if (-not (Test-Path -LiteralPath $executable -PathType Leaf)) {
        throw 'Release executable is missing.'
    }
    $packageRoot = Join-Path $metadata.target_directory 'packages'
    New-Item -ItemType Directory -Force -Path $packageRoot | Out-Null
    $stage = Join-Path $packageRoot ('stage-' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $stage | Out-Null
    try {
        $name = "ElegantClipboard-v$version-$hostTarget"
        if ($IsMacOS) {
            # Apple bundle version fields use the numeric release version, not SemVer prerelease labels.
            $bundleVersion = ($version -split '[-+]')[0]
            $app = Join-Path $stage 'ElegantClipboard.app'
            $contents = Join-Path $app 'Contents'
            New-Item -ItemType Directory -Path (Join-Path $contents 'MacOS') | Out-Null
            Copy-Item -LiteralPath $executable -Destination (Join-Path $contents 'MacOS/elegant-clipboard-gpui')
            $plist = @"
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>com.aslant.elegant-clipboard-gpui</string>
<key>CFBundleName</key><string>ElegantClipboard</string>
<key>CFBundleExecutable</key><string>elegant-clipboard-gpui</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>$bundleVersion</string>
<key>CFBundleVersion</key><string>$bundleVersion</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
"@
            Set-Content -LiteralPath (Join-Path $contents 'Info.plist') -Value $plist -Encoding utf8NoBOM
            & plutil -lint (Join-Path $contents 'Info.plist')
            if ($LASTEXITCODE -ne 0) { throw 'Invalid application property list.' }
            # Ad-hoc signing allows local execution; this is not Developer ID signing or notarization.
            & codesign --force --sign - $app
            if ($LASTEXITCODE -ne 0) { throw 'Ad-hoc application signing failed.' }
            Copy-Item -LiteralPath (Join-Path $root 'LICENSE') -Destination $stage
            Copy-Item -LiteralPath (Join-Path $root 'README.md') -Destination $stage
            & ln -s /Applications (Join-Path $stage 'Applications')
            if ($LASTEXITCODE -ne 0) { throw 'Unable to create the Applications shortcut.' }
            $archive = Join-Path $packageRoot "$name.dmg"
            & hdiutil create -ov -volname 'ElegantClipboard' -srcfolder $stage -fs HFS+ -format UDZO $archive
        } else {
            $bundle = Join-Path $stage $name
            New-Item -ItemType Directory -Path $bundle | Out-Null
            Copy-Item -LiteralPath $executable -Destination $bundle
            Copy-Item -LiteralPath (Join-Path $root 'LICENSE') -Destination $bundle
            Copy-Item -LiteralPath (Join-Path $root 'README.md') -Destination $bundle
            & chmod 755 (Join-Path $bundle 'elegant-clipboard-gpui')
            if ($LASTEXITCODE -ne 0) { throw 'Unable to mark executable.' }
            $archive = Join-Path $packageRoot "$name.tar.gz"
            & tar -czf $archive -C $stage $name
        }
        if ($LASTEXITCODE -ne 0) { throw 'Archive creation failed.' }
        $hash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
        Set-Content -LiteralPath "$archive.sha256" -Value "$hash *$([IO.Path]::GetFileName($archive))" -Encoding utf8NoBOM
        Write-Output "Package: $archive"
        Write-Output "SHA256: $hash"
        if ($IsLinux) {
            & (Join-Path $PSScriptRoot 'package-gpui-linux.ps1') -Executable $executable -Version $version -HostTarget $hostTarget -OutputDirectory $packageRoot -SourceRoot $root
        }
    } finally {
        Remove-Item -LiteralPath $stage -Recurse -Force
    }
} finally {
    Pop-Location
}
