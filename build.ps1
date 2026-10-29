param([switch]$Run, [string]$TargetDirectory = 'target',
    [ValidateSet('arm64','x64')][string]$Architecture = 'arm64', [switch]$Store)
$ErrorActionPreference = 'Stop'
Set-Location -LiteralPath $PSScriptRoot
$tools = Join-Path (Split-Path -Parent $PSScriptRoot) 'tools'
$localCargo = Join-Path $tools 'cargo\bin\cargo.exe'
if (Test-Path -LiteralPath $localCargo) {
    $env:CARGO_HOME = Join-Path $tools 'cargo'
    $env:RUSTUP_HOME = Join-Path $tools 'rustup'
    $env:PATH = "$(Join-Path $tools 'cargo\bin');$env:PATH"
}
$sdkRoot = Join-Path $tools $(if ($Architecture -eq 'x64') { 'msvc-x64' } else { 'msvc' })
if (Test-Path -LiteralPath $sdkRoot) {
    $compiler = Join-Path $sdkRoot 'VC\Tools\MSVC\14.44.35207'
    $kits = Join-Path $sdkRoot 'Windows Kits\10'
    $env:PATH = "$compiler\bin\Hostarm64\$Architecture;$kits\bin\10.0.26100.0\arm64;$env:PATH"
    $env:LIB = "$compiler\lib\$Architecture;$kits\Lib\10.0.26100.0\ucrt\$Architecture;$kits\Lib\10.0.26100.0\um\$Architecture"
}
$target = if ($Architecture -eq 'arm64') { 'aarch64-pc-windows-msvc' } else { 'x86_64-pc-windows-msvc' }
if (-not [IO.Path]::IsPathRooted($TargetDirectory)) {
    $TargetDirectory = Join-Path $PSScriptRoot $TargetDirectory
}
$TargetDirectory = [IO.Path]::GetFullPath($TargetDirectory)
$arguments = @()
# Use the x64 host for its build scripts too, avoiding mixed-architecture LIBs.
$x64Host = '1.98.1-x86_64-pc-windows-msvc'
if ($Architecture -eq 'x64' -and (Test-Path -LiteralPath (Join-Path $tools "rustup\toolchains\$x64Host"))) { $arguments += "+$x64Host" }
$arguments += @('build','--locked','--release','--target',$target,'--target-dir',$TargetDirectory)
if ($Store) { $arguments += @('--features','store') }
& cargo @arguments
if ($LASTEXITCODE -ne 0) { throw 'Rust build failed.' }
$output = Join-Path $TargetDirectory "$target\release\mio-note.exe"
Write-Host "Built: $output"
if ($Run) { Start-Process -FilePath $output -WorkingDirectory $PSScriptRoot -WindowStyle Hidden }
