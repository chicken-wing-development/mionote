param(
    [ValidateSet('arm64','x64')][string]$Architecture = 'arm64',
    [string]$SignedExecutable,
    [switch]$RequireSignature
)
$ErrorActionPreference = 'Stop'
Set-Location -LiteralPath $PSScriptRoot
. (Join-Path $PSScriptRoot 'scripts/release-common.ps1')
if ($SignedExecutable) { $executable = [IO.Path]::GetFullPath($SignedExecutable) }
else {
    $target = Join-Path $PSScriptRoot "target/portable-$Architecture"
    & (Join-Path $PSScriptRoot 'build.ps1') -Architecture $Architecture -TargetDirectory $target
    $triple = if ($Architecture -eq 'arm64') { 'aarch64-pc-windows-msvc' } else { 'x86_64-pc-windows-msvc' }
    $executable = Join-Path $target "$triple/release/mio-note.exe"
}
Assert-MioExecutable -Path $executable -Architecture $Architecture
$version = Get-MioVersion $PSScriptRoot
if ((Get-Item -LiteralPath $executable).VersionInfo.FileVersion -ne $version) { throw 'Executable version mismatch.' }
if (($RequireSignature -or $SignedExecutable) -and (Get-AuthenticodeSignature -LiteralPath $executable).Status -ne 'Valid') {
    throw 'A valid Authenticode signature is required for this release.'
}
$name = "MioNote-$version-portable-$Architecture"
# Always archive fresh staging, even if an earlier output contains user data.
$staging = Join-Path $PSScriptRoot ('target/portable-package/' + [Guid]::NewGuid().ToString('N'))
$folder = Join-Path $staging $name
New-Item -ItemType Directory -Path $folder -Force | Out-Null
Copy-Item -LiteralPath $executable -Destination (Join-Path $folder 'MioNote.exe')
Copy-MioReleaseDocuments -Root $PSScriptRoot -Destination $folder
[IO.File]::WriteAllText((Join-Path $folder 'mio-note.portable'), "MioNote portable v1`n", [Text.Encoding]::ASCII)
$hash = (Get-FileHash -LiteralPath (Join-Path $folder 'MioNote.exe') -Algorithm SHA256).Hash.ToLowerInvariant()
[IO.File]::WriteAllText((Join-Path $folder 'SHA256SUMS.txt'), "$hash  MioNote.exe`n", [Text.Encoding]::ASCII)
$dist = Join-Path $PSScriptRoot 'dist'
New-Item -ItemType Directory -Path $dist -Force | Out-Null
$zip = Join-Path $dist "$name.zip"
Compress-Archive -LiteralPath $folder -DestinationPath $zip -CompressionLevel Optimal -Force
Write-MioHash $zip
# Copy only package items; never remove or archive existing notes/settings.
$outputFolder = Join-Path $dist $name
New-Item -ItemType Directory -Path $outputFolder -Force | Out-Null
foreach ($item in Get-ChildItem -LiteralPath $folder -Force) {
    Copy-Item -LiteralPath $item.FullName -Destination $outputFolder -Recurse -Force
}
Write-Host "Portable EXE: $(Join-Path $outputFolder 'MioNote.exe')"
Write-Host "Portable ZIP: $zip"
