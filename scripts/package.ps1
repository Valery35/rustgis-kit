# rustgis-kit - общая основа расширений RustGIS линейки Информ++.
# Copyright (C) 2026 ООО «Информ++». Лицензия Apache 2.0.
# SPDX-License-Identifier: Apache-2.0
#
# Пакет .rgplugin расширения-процесса RustGIS: plugin.json и программа
# по пути из run.windows манифеста. Подходит любому расширению на крейте.
#
#   powershell -File C:\Dev\rustgis-kit\scripts\package.ps1 `
#       -Manifest apps\my-rustgis\plugin.json `
#       -Program target\release\my-rustgis.exe `
#       -Dist dist
#
# В -Dist появляются:
#   <id>/                  plugin.json и программа - папка для
#                          «Add Plugin Folder…» и rustgis-cli
#   <id>-<версия>.rgplugin пакет для установки из файла
#
# Дополнительные файлы пакета: -Extra @{ 'docs/index.html' = 'путь\к\файлу' }.

param(
    [Parameter(Mandatory = $true)][string]$Manifest,
    [Parameter(Mandatory = $true)][string]$Program,
    [string]$Dist = 'dist',
    [hashtable]$Extra = @{}
)

$ErrorActionPreference = 'Stop'
if (-not (Test-Path $Manifest)) { throw "no manifest at ${Manifest}" }
if (-not (Test-Path $Program)) { throw "no program at ${Program} - cargo build --release" }
$manifestText = [IO.File]::ReadAllText((Resolve-Path $Manifest).Path, [Text.Encoding]::UTF8)
$m = $manifestText | ConvertFrom-Json
$id = $m.id
$version = $m.version
if (-not $id -or -not $version) { throw 'plugin.json has no id or version' }
# путь программы в пакете - первый элемент run.windows
$bin = @($m.run.windows)[0]
if (-not $bin) { throw 'plugin.json has no run.windows' }

$files = @(
    @{ path = 'plugin.json'; bytes = (New-Object Text.UTF8Encoding($false)).GetBytes($manifestText) },
    @{ path = $bin; bytes = [IO.File]::ReadAllBytes((Resolve-Path $Program).Path) }
)
foreach ($k in $Extra.Keys) {
    $files += @{ path = $k; bytes = [IO.File]::ReadAllBytes((Resolve-Path $Extra[$k]).Path) }
}

New-Item -ItemType Directory -Force $Dist | Out-Null
$folder = Join-Path $Dist $id
if (Test-Path $folder) { Remove-Item -Recurse -Force $folder -Confirm:$false }
foreach ($f in $files) {
    $target = $folder
    foreach ($part in $f.path.Split('/')) { $target = Join-Path $target $part }
    New-Item -ItemType Directory -Force (Split-Path -Parent $target) | Out-Null
    [IO.File]::WriteAllBytes($target, $f.bytes)
}

# Пакет - ZIP с файлами в корне, время записей одно: одинаковые файлы
# дают одинаковый пакет и SHA-256.
Add-Type -AssemblyName System.IO.Compression
$path = Join-Path $Dist "$id-$version.rgplugin"
if (Test-Path $path) { Remove-Item $path -Confirm:$false }
$stream = [IO.File]::Open($path, [IO.FileMode]::CreateNew)
try {
    $zip = New-Object IO.Compression.ZipArchive($stream, [IO.Compression.ZipArchiveMode]::Create)
    try {
        foreach ($f in $files) {
            $entry = $zip.CreateEntry($f.path, [IO.Compression.CompressionLevel]::Optimal)
            $entry.LastWriteTime = [DateTimeOffset]::new(2024, 1, 1, 0, 0, 0, [TimeSpan]::Zero)
            $out = $entry.Open()
            try { $out.Write($f.bytes, 0, $f.bytes.Length) } finally { $out.Dispose() }
        }
    } finally { $zip.Dispose() }
} finally { $stream.Dispose() }
$size = (Get-Item $path).Length
# RustGIS не берёт пакет больше 64 МиБ.
if ($size -gt 64MB) { throw ("{0} is {1:N0} bytes, over 64 MiB" -f (Split-Path -Leaf $path), $size) }
$sha = (Get-FileHash $path -Algorithm SHA256).Hash.ToLowerInvariant()
Write-Output ("{0}  {1:N0} bytes  sha256 {2}" -f $path, $size, $sha)
