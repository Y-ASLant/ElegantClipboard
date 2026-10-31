# Build elegant-clipboard .deb and .rpm from a native Linux release binary.
# Invoked by package-gpui-unix.ps1. Requires dpkg-dev and rpm.
param(
    [Parameter(Mandatory = $true)][string]$Executable,
    [Parameter(Mandatory = $true)][string]$Version,
    [Parameter(Mandatory = $true)][string]$HostTarget,
    [Parameter(Mandatory = $true)][string]$OutputDirectory,
    [Parameter(Mandatory = $true)][string]$SourceRoot
)

$ErrorActionPreference = 'Stop'
$PSNativeCommandUseErrorActionPreference = $false
if (-not $IsLinux) {
    throw 'Build Linux installers on Linux.'
}

function Write-UnixText {
    param([string]$Path, [string]$Content)
    $text = $Content.Replace("`r`n", "`n").Replace("`r", "`n")
    if (-not $text.EndsWith("`n")) { $text += "`n" }
    [IO.File]::WriteAllText($Path, $text, [Text.UTF8Encoding]::new($false))
}

function Invoke-Native {
    param([string]$File, [string[]]$ArgumentList, [string]$Failure)
    $output = & $File @ArgumentList 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "$Failure`n$(($output | Out-String).Trim())"
    }
    return @($output)
}

function Get-LinuxPackageIdentity {
    param([string]$CargoVersion, [string]$Target)
    if ($CargoVersion -notmatch '^(\d+\.\d+\.\d+)(?:-([0-9A-Za-z]+(?:\.[0-9A-Za-z]+)*))?$') {
        throw "Cannot map Cargo version to a package version: $CargoVersion"
    }
    $upstream = $Matches[1]
    if ($Matches[2]) { $upstream = "$upstream~$($Matches[2])" }
    $arch = switch ($Target) {
        'x86_64-unknown-linux-gnu' { [pscustomobject]@{ Deb = 'amd64'; Rpm = 'x86_64' } }
        'aarch64-unknown-linux-gnu' { [pscustomobject]@{ Deb = 'arm64'; Rpm = 'aarch64' } }
        default { throw "Unsupported Linux package target: $Target" }
    }
    [pscustomobject]@{
        Debian = "$upstream-1"
        RpmVersion = $upstream
        RpmRelease = '1'
        DebArch = $arch.Deb
        RpmArch = $arch.Rpm
    }
}

function Split-DebianDepends {
    param([string]$Depends)
    $parts = [System.Collections.Generic.List[string]]::new()
    $current = New-Object System.Text.StringBuilder
    $depth = 0
    foreach ($char in $Depends.ToCharArray()) {
        if ($char -eq '(') { $depth++ }
        elseif ($char -eq ')') { $depth-- }
        if ($char -eq ',' -and $depth -eq 0) {
            $atom = $current.ToString().Trim()
            if ($atom) { $parts.Add($atom) }
            [void]$current.Clear()
            continue
        }
        [void]$current.Append($char)
    }
    $atom = $current.ToString().Trim()
    if ($atom) { $parts.Add($atom) }
    return $parts
}

function Get-DebianPackageName {
    param([string]$Atom)
    $first = ($Atom -split '\|')[0].Trim()
    return (($first -split '\s+')[0] -replace ':.*$', '')
}

function Get-SharedLibraryDepends {
    param([string]$Binary)
    $tool = Get-Command dpkg-shlibdeps -ErrorAction SilentlyContinue
    if (-not $tool) { throw 'dpkg-shlibdeps is missing. Install dpkg-dev.' }
    $work = Join-Path ([IO.Path]::GetTempPath()) ('elegant-shlibdeps-' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path (Join-Path $work 'debian') | Out-Null
    try {
        Write-UnixText (Join-Path $work 'debian/control') "Source: elegant-clipboard`nPackage: elegant-clipboard`n"
        Push-Location $work
        try {
            $output = & dpkg-shlibdeps -O --ignore-missing-info -e $Binary 2>&1
            if ($LASTEXITCODE -ne 0) {
                throw "dpkg-shlibdeps failed.`n$(($output | Out-String).Trim())"
            }
        } finally {
            Pop-Location
        }
    } finally {
        Remove-Item -LiteralPath $work -Recurse -Force
    }
    $line = @($output | ForEach-Object { "$_" } | Where-Object { $_ -match '^shlibs:Depends=' } | Select-Object -Last 1)
    if ($line.Count -eq 0) { return @() }
    return @(Split-DebianDepends $line[0].Substring('shlibs:Depends='.Length))
}

function Get-ElfRequires {
    param([string]$Binary)
    $tool = '/usr/lib/rpm/rpmdeps'
    if (-not (Test-Path -LiteralPath $tool)) {
        $command = Get-Command rpmdeps -ErrorAction SilentlyContinue
        if (-not $command) { throw 'rpmdeps is missing. Install the rpm package.' }
        $tool = $command.Source
    }
    # Same entry point as /usr/lib/rpm/find-requires. File arguments are accepted;
    # stdin is only the fallback when no path is given.
    $output = & $tool --define '_use_internal_dependency_generator 1' --requires -- $Binary 2>&1
    if ($LASTEXITCODE -ne 0) {
        throw "rpmdeps failed.`n$(($output | Out-String).Trim())"
    }
    return @($output | ForEach-Object { "$_".Trim() } | Where-Object { $_ -and $_ -notmatch '^(warning|error):' })
}

function Add-UniqueAtoms {
    param([System.Collections.Generic.List[string]]$Atoms, [string[]]$Extra)
    $known = [System.Collections.Generic.HashSet[string]]::new([StringComparer]::Ordinal)
    foreach ($atom in $Atoms) { [void]$known.Add((Get-DebianPackageName $atom)) }
    foreach ($atom in $Extra) {
        $name = Get-DebianPackageName $atom
        if ($known.Add($name)) { $Atoms.Add($atom) }
    }
}

function Publish-PackageHash {
    param([string]$Path)
    $hash = (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
    $filename = [IO.Path]::GetFileName($Path)
    Write-UnixText "$Path.sha256" "$hash *$filename"
    Write-Output "Package: $Path"
    Write-Output "SHA256: $hash"
}

$identity = Get-LinuxPackageIdentity $Version $HostTarget
$executablePath = (Resolve-Path -LiteralPath $Executable).Path
if (-not (Test-Path -LiteralPath $executablePath -PathType Leaf)) {
    throw "Release executable is missing: $executablePath"
}
foreach ($relative in @('LICENSE', 'README.md', 'assets/elegant-clipboard.png')) {
    $source = Join-Path $SourceRoot $relative
    if (-not (Test-Path -LiteralPath $source -PathType Leaf)) {
        throw "Package resource is missing: $source"
    }
}
foreach ($tool in @('dpkg-deb', 'rpmbuild')) {
    if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) {
        throw "$tool is missing. Install dpkg-dev and rpm."
    }
}

$debDepends = [System.Collections.Generic.List[string]]::new()
foreach ($atom in @(Get-SharedLibraryDepends $executablePath)) { $debDepends.Add($atom) }
Add-UniqueAtoms $debDepends @(
    'libvulkan1',
    'libwayland-client0',
    'libwayland-cursor0',
    'libayatana-appindicator3-1 | libappindicator3-1',
    'xdg-utils',
    'dbus'
)
$debRecommends = 'pulseaudio-utils, mesa-vulkan-drivers'

$rpmRequires = [System.Collections.Generic.List[string]]::new()
foreach ($require in @(Get-ElfRequires $executablePath)) {
    if (-not $rpmRequires.Contains($require)) { $rpmRequires.Add($require) }
}
foreach ($require in @(
        'libvulkan.so.1()(64bit)',
        'libwayland-client.so.0()(64bit)',
        'libwayland-cursor.so.0()(64bit)',
        '(libayatana-appindicator3.so.1()(64bit) or libappindicator3.so.1()(64bit))',
        'xdg-utils',
        'dbus'
    )) {
    if (-not $rpmRequires.Contains($require)) { $rpmRequires.Add($require) }
}

$stage = Join-Path $OutputDirectory ('linux-stage-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $stage | Out-Null
try {
    $payload = Join-Path $stage 'payload'
    $binDir = Join-Path $payload 'usr/bin'
    $docDir = Join-Path $payload 'usr/share/doc/elegant-clipboard'
    $iconDir = Join-Path $payload 'usr/share/icons/hicolor/256x256/apps'
    New-Item -ItemType Directory -Path $binDir, $docDir, $iconDir, (Join-Path $payload 'usr/share/applications') | Out-Null
    Copy-Item -LiteralPath $executablePath -Destination (Join-Path $binDir 'elegant-clipboard-gpui')
    & chmod 755 (Join-Path $binDir 'elegant-clipboard-gpui')
    if ($LASTEXITCODE -ne 0) { throw 'Unable to mark the installed executable.' }
    & ln -s elegant-clipboard-gpui (Join-Path $binDir 'elegant-clipboard')
    if ($LASTEXITCODE -ne 0) { throw 'Unable to create the elegant-clipboard command.' }
    Copy-Item -LiteralPath (Join-Path $SourceRoot 'LICENSE') -Destination (Join-Path $docDir 'LICENSE')
    Copy-Item -LiteralPath (Join-Path $SourceRoot 'README.md') -Destination (Join-Path $docDir 'README.md')
    Copy-Item -LiteralPath (Join-Path $SourceRoot 'assets/elegant-clipboard.png') -Destination (Join-Path $iconDir 'elegant-clipboard.png')
    Write-UnixText (Join-Path $payload 'usr/share/applications/elegant-clipboard.desktop') @'
[Desktop Entry]
Type=Application
Version=1.0
Name=ElegantClipboard
GenericName=Clipboard Manager
GenericName[zh_CN]=剪贴板管理器
Comment=Native clipboard history for text, images, and files
Comment[zh_CN]=记录文本、图片和文件路径的原生剪贴板管理器
Exec=/usr/bin/elegant-clipboard
Icon=elegant-clipboard
Terminal=false
Categories=Utility;
Keywords=clipboard;history;paste;
'@

    $installedBytes = 0L
    Get-ChildItem -LiteralPath $payload -Recurse -File -Force | ForEach-Object { $installedBytes += $_.Length }
    $installedSize = [int][Math]::Max(1, [Math]::Ceiling($installedBytes / 1KB))
    $debRoot = Join-Path $stage 'deb'
    & cp -a $payload $debRoot
    if ($LASTEXITCODE -ne 0) { throw 'Unable to stage the deb payload.' }
    $debianDir = Join-Path $debRoot 'DEBIAN'
    New-Item -ItemType Directory -Path $debianDir | Out-Null
    $depends = ($debDepends -join ', ')
    Write-UnixText (Join-Path $debianDir 'control') @"
Package: elegant-clipboard
Version: $($identity.Debian)
Section: utils
Priority: optional
Architecture: $($identity.DebArch)
Maintainer: ASLant <aslant@users.noreply.github.com>
Homepage: https://github.com/Y-ASLant/ElegantClipboard
Installed-Size: $installedSize
Depends: $depends
Recommends: $debRecommends
Description: Native clipboard history manager
 ElegantClipboard records text, rich text, images, and file paths.
 History stays in the user data directory and is kept when the package is removed.
"@
    Write-UnixText (Join-Path $debianDir 'postinst') @'
#!/bin/sh
set -e
if [ "$1" != configure ]; then
    exit 0
fi
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database -q /usr/share/applications || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -q -t /usr/share/icons/hicolor || true
fi
'@
    Write-UnixText (Join-Path $debianDir 'postrm') @'
#!/bin/sh
set -e
if [ "$1" != remove ] && [ "$1" != purge ]; then
    exit 0
fi
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database -q /usr/share/applications || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -q -t /usr/share/icons/hicolor || true
fi
'@
    & chmod 755 (Join-Path $debianDir 'postinst') (Join-Path $debianDir 'postrm')
    if ($LASTEXITCODE -ne 0) { throw 'Unable to mark maintainer scripts executable.' }

    $debName = "ElegantClipboard-v$Version-$HostTarget.deb"
    $debPath = Join-Path $OutputDirectory $debName
    if (Test-Path -LiteralPath $debPath) { Remove-Item -LiteralPath $debPath -Force }
    Invoke-Native dpkg-deb @('--root-owner-group', '--build', $debRoot, $debPath) 'dpkg-deb failed.' | Out-Null

    $top = Join-Path $stage 'rpm'
    foreach ($dir in @('BUILD', 'BUILDROOT', 'RPMS', 'SOURCES', 'SPECS', 'SRPMS', 'tmp')) {
        New-Item -ItemType Directory -Path (Join-Path $top $dir) | Out-Null
    }
    & cp -a $payload (Join-Path $top 'SOURCES/payload')
    if ($LASTEXITCODE -ne 0) { throw 'Unable to stage the rpm payload.' }
    $requireLines = ($rpmRequires | ForEach-Object { "Requires: $_" }) -join "`n"
    $fileLines = @(
        '%attr(0755, root, root) /usr/bin/elegant-clipboard-gpui',
        '/usr/bin/elegant-clipboard',
        '/usr/share/applications/elegant-clipboard.desktop',
        '/usr/share/icons/hicolor/256x256/apps/elegant-clipboard.png',
        '/usr/share/doc/elegant-clipboard/LICENSE',
        '/usr/share/doc/elegant-clipboard/README.md'
    ) -join "`n"
    $spec = @"
Name: elegant-clipboard
Version: $($identity.RpmVersion)
Release: $($identity.RpmRelease)
Summary: Native clipboard history manager
License: MIT
URL: https://github.com/Y-ASLant/ElegantClipboard
BuildArch: $($identity.RpmArch)
AutoReqProv: no
$requireLines
Recommends: pulseaudio-utils
Recommends: mesa-vulkan-drivers

%description
ElegantClipboard records text, rich text, images, and file paths.
History stays in the user data directory and is kept when the package is removed.

%install
rm -rf %{buildroot}
mkdir -p %{buildroot}
cp -a %{_sourcedir}/payload/. %{buildroot}/

%post
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database -q /usr/share/applications || :
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -q -t /usr/share/icons/hicolor || :
fi

%postun
if [ "`$1" -eq 0 ]; then
    if command -v update-desktop-database >/dev/null 2>&1; then
        update-desktop-database -q /usr/share/applications || :
    fi
    if command -v gtk-update-icon-cache >/dev/null 2>&1; then
        gtk-update-icon-cache -q -t /usr/share/icons/hicolor || :
    fi
fi

%files
$fileLines
"@
    $specPath = Join-Path $top 'SPECS/elegant-clipboard.spec'
    Write-UnixText $specPath $spec
    Invoke-Native rpmbuild @(
        '-bb', '--nodeps', '--noclean',
        '--define', "_topdir $top",
        '--define', "_tmppath $(Join-Path $top 'tmp')",
        $specPath
    ) 'rpmbuild failed.' | Out-Null
    $built = @(Get-ChildItem -LiteralPath (Join-Path $top 'RPMS') -Recurse -File -Filter '*.rpm')
    if ($built.Count -ne 1) { throw "Expected one rpm, found $($built.Count)." }
    $rpmName = "ElegantClipboard-v$Version-$HostTarget.rpm"
    $rpmPath = Join-Path $OutputDirectory $rpmName
    if (Test-Path -LiteralPath $rpmPath) { Remove-Item -LiteralPath $rpmPath -Force }
    Move-Item -LiteralPath $built[0].FullName -Destination $rpmPath

    $requiredPaths = @(
        'usr/bin/elegant-clipboard-gpui',
        'usr/bin/elegant-clipboard',
        'usr/share/applications/elegant-clipboard.desktop',
        'usr/share/icons/hicolor/256x256/apps/elegant-clipboard.png',
        'usr/share/doc/elegant-clipboard/LICENSE',
        'usr/share/doc/elegant-clipboard/README.md'
    )
    $debListing = Invoke-Native dpkg-deb @('--contents', $debPath) 'Unable to list the deb.'
    $rpmListing = Invoke-Native rpm @('-qp', '-l', $rpmPath) 'Unable to list the rpm.'
    foreach ($path in $requiredPaths) {
        $pattern = '(^|[^\w.-])' + [regex]::Escape($path) + '($|[^\w.-])'
        $debHit = @($debListing | Where-Object { "$_" -match $pattern })
        $rpmHit = @($rpmListing | Where-Object { "$_" -match $pattern })
        if ($debHit.Count -eq 0 -or $rpmHit.Count -eq 0) {
            throw "Installer is missing $path"
        }
    }
    $debPackage = (Invoke-Native dpkg-deb @('-f', $debPath, 'Package') 'Unable to read the deb package name.') -join ''
    $debVersion = (Invoke-Native dpkg-deb @('-f', $debPath, 'Version') 'Unable to read the deb version.') -join ''
    $debArch = (Invoke-Native dpkg-deb @('-f', $debPath, 'Architecture') 'Unable to read the deb architecture.') -join ''
    $debDependsField = (Invoke-Native dpkg-deb @('-f', $debPath, 'Depends') 'Unable to read the deb dependencies.') -join ''
    foreach ($pair in @(
            @($debPackage.Trim(), 'elegant-clipboard'),
            @($debVersion.Trim(), $identity.Debian),
            @($debArch.Trim(), $identity.DebArch)
        )) {
        if ($pair[0] -ne $pair[1]) { throw "Unexpected deb metadata: $($pair[0]) != $($pair[1])" }
    }
    foreach ($token in @('libvulkan1', 'xdg-utils', 'dbus', 'libayatana-appindicator3-1')) {
        if ($debDependsField -notlike "*$token*") { throw "Deb dependencies are missing $token" }
    }
    $rpmIdentity = (Invoke-Native rpm @(
            '-qp', '--queryformat', '%{NAME}|%{VERSION}|%{RELEASE}|%{ARCH}', $rpmPath
        ) 'Unable to read the rpm identity.') -join ''
    $expectedRpm = "elegant-clipboard|$($identity.RpmVersion)|$($identity.RpmRelease)|$($identity.RpmArch)"
    if ($rpmIdentity.Trim() -ne $expectedRpm) {
        throw "Unexpected rpm identity: $rpmIdentity"
    }
    $rpmDependsField = (Invoke-Native rpm @('-qp', '--requires', $rpmPath) 'Unable to read the rpm dependencies.') -join "`n"
    foreach ($token in @('libvulkan.so.1', 'xdg-utils', 'dbus', 'libayatana-appindicator3.so.1')) {
        if ($rpmDependsField -notlike "*$token*") { throw "Rpm dependencies are missing $token" }
    }

    Publish-PackageHash $debPath
    Publish-PackageHash $rpmPath
} finally {
    Remove-Item -LiteralPath $stage -Recurse -Force
}
