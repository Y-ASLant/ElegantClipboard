$ErrorActionPreference = 'Stop'
if (-not $IsWindows) {
    throw 'GPUI Windows 压缩包只能在 Windows 上构建'
}

$root = (Resolve-Path -LiteralPath (Join-Path $PSScriptRoot '..')).Path
Push-Location $root
try {
    if ($env:CARGO_BUILD_TARGET) {
        throw '当前脚本只打包本机 Windows x64 MSVC 构建，请清除 CARGO_BUILD_TARGET'
    }
    $hostLine = & rustc -vV | Where-Object { $_ -like 'host: *' }
    if ($LASTEXITCODE -ne 0 -or $hostLine -ne 'host: x86_64-pc-windows-msvc') {
        throw '当前仅支持 Windows x64 MSVC 构建目标'
    }
    & cargo build -p elegant-clipboard-gpui --release --locked
    if ($LASTEXITCODE -ne 0) {
        throw "Cargo Release 构建失败：$LASTEXITCODE"
    }
    $metadata = (& cargo metadata --no-deps --format-version 1 --locked | ConvertFrom-Json)
    if ($LASTEXITCODE -ne 0) {
        throw "无法读取 Cargo 包信息：$LASTEXITCODE"
    }
    $package = @($metadata.packages | Where-Object name -eq 'elegant-clipboard-gpui')
    if ($package.Count -ne 1) {
        throw '无法确定 GPUI 应用版本'
    }
    $filename = "elegant-clipboard-gpui-v$($package[0].version)-windows-x64.zip"
    $packageRoot = Join-Path $metadata.target_directory 'packages'
    $executable = Join-Path $metadata.target_directory 'release\elegant-clipboard-gpui.exe'
    if (-not (Test-Path -LiteralPath $executable -PathType Leaf)) {
        throw 'Release 可执行文件不存在'
    }

    New-Item -ItemType Directory -Path $packageRoot -Force | Out-Null
    $stage = Join-Path $packageRoot ("stage-" + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $stage | Out-Null
    try {
        Copy-Item -LiteralPath $executable -Destination (Join-Path $stage 'elegant-clipboard-gpui.exe')
        Copy-Item -LiteralPath (Join-Path $root 'LICENSE') -Destination (Join-Path $stage 'LICENSE')
        Copy-Item -LiteralPath (Join-Path $root 'docs\WINDOWS_PACKAGE_README.txt') -Destination (Join-Path $stage 'README.txt')

        $archive = Join-Path $packageRoot $filename
        Compress-Archive -Path (Join-Path $stage '*') -DestinationPath $archive -CompressionLevel Optimal -Force
        $hash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
        Set-Content -LiteralPath "$archive.sha256" -Value "$hash *$filename" -Encoding utf8NoBOM
        Write-Output "Package: $archive"
        Write-Output "SHA256: $hash"
    }
    finally {
        $resolvedRoot = (Resolve-Path -LiteralPath $packageRoot).Path.TrimEnd('\')
        $resolvedStage = (Resolve-Path -LiteralPath $stage).Path.TrimEnd('\')
        $prefix = $resolvedRoot + [IO.Path]::DirectorySeparatorChar
        if (-not $resolvedStage.StartsWith($prefix, [StringComparison]::OrdinalIgnoreCase)) {
            throw "拒绝清理打包目录外的路径：$resolvedStage"
        }
        Remove-Item -LiteralPath $resolvedStage -Recurse -Force
    }
}
finally {
    Pop-Location
}
