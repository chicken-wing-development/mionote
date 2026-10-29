param(
    [string]$IdentityFile = 'distribution/store/identity.json',
    [ValidateSet('arm64','x64')][string[]]$Architectures = @('arm64','x64'),
    [string]$SdkBin
)
$ErrorActionPreference = 'Stop'
Set-Location -LiteralPath $PSScriptRoot
. (Join-Path $PSScriptRoot 'scripts/release-common.ps1')
if (-not [IO.Path]::IsPathRooted($IdentityFile)) { $IdentityFile = Join-Path $PSScriptRoot $IdentityFile }
$item = Get-Item -LiteralPath $IdentityFile -Force
if ($item.Length -gt 16384 -or $item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'Invalid identity file.' }
$identity = Get-Content -LiteralPath $IdentityFile -Raw -Encoding utf8 | ConvertFrom-Json
$keys = @('Name','Publisher','PublisherDisplayName','DisplayName','PackageVersion')
if (@($identity.PSObject.Properties.Name | Where-Object { $_ -notin $keys }).Count) { throw 'Unknown identity field.' }
foreach ($key in $keys) {
    $value = $identity.$key
    if ($value -isnot [string] -or [string]::IsNullOrWhiteSpace($value) -or $value.Length -gt 256 -or $value -match '[\x00-\x1f]') { throw "Invalid identity field: $key" }
}
if ($identity.Name -notmatch '^[A-Za-z0-9][A-Za-z0-9.-]{2,49}$') { throw 'Invalid package identity name.' }
if (-not $identity.Publisher.StartsWith('CN=')) { throw 'Publisher must match the Partner Center distinguished name.' }
$null = [Security.Cryptography.X509Certificates.X500DistinguishedName]::new($identity.Publisher)
if ($identity.PackageVersion -notmatch '^\d{1,5}\.\d{1,5}\.\d{1,5}\.0$') { throw 'Store version must contain four components and end in .0.' }
$parts = @($identity.PackageVersion.Split('.') | ForEach-Object { [int]$_ })
if ($parts[0] -lt 1 -or ($parts | Where-Object { $_ -gt 65535 })) { throw 'Invalid Store package version.' }
if (@($Architectures | Select-Object -Unique).Count -ne $Architectures.Count -or $Architectures.Count -eq 0) { throw 'Duplicate or missing architecture.' }
$makeappx = Get-MioSdkTool -Name 'makeappx.exe' -SdkBin $SdkBin
$makepri = Get-MioSdkTool -Name 'makepri.exe' -SdkBin $SdkBin
$version = Get-MioVersion $PSScriptRoot
$staging = Join-Path $PSScriptRoot ('target/store-package/' + [Guid]::NewGuid().ToString('N'))
$bundleInput = Join-Path $staging 'bundle-input'
$dist = Join-Path $PSScriptRoot 'dist'
New-Item -ItemType Directory -Path $bundleInput,$dist -Force | Out-Null
$escape = { param([string]$Value) [Security.SecurityElement]::Escape($Value) }
$name = & $escape $identity.Name
$publisher = & $escape $identity.Publisher
$publisherDisplay = & $escape $identity.PublisherDisplayName
$display = & $escape $identity.DisplayName
foreach ($architecture in $Architectures) {
    $target = Join-Path $PSScriptRoot "target/store-$architecture"
    & (Join-Path $PSScriptRoot 'build.ps1') -Store -Architecture $architecture -TargetDirectory $target
    $triple = if ($architecture -eq 'arm64') { 'aarch64-pc-windows-msvc' } else { 'x86_64-pc-windows-msvc' }
    $executable = Join-Path $target "$triple/release/mio-note.exe"
    Assert-MioExecutable -Path $executable -Architecture $architecture
    $folder = Join-Path $staging $architecture
    New-Item -ItemType Directory -Path $folder | Out-Null
    Copy-Item -LiteralPath $executable -Destination (Join-Path $folder 'MioNote.exe')
    Copy-MioReleaseDocuments -Root $PSScriptRoot -Destination $folder -Store
    $assets = Join-Path $folder 'Assets'
    New-Item -ItemType Directory -Path $assets | Out-Null
    foreach ($asset in @('StoreLogo.png','Square44x44Logo.png','Square150x150Logo.png')) {
        Copy-MioCheckedFile -Path (Join-Path $PSScriptRoot "distribution/store/Assets/$asset") -Destination $assets
    }
    $xml = @"
<?xml version="1.0" encoding="utf-8"?>
<Package xmlns="http://schemas.microsoft.com/appx/manifest/foundation/windows10"
 xmlns:uap="http://schemas.microsoft.com/appx/manifest/uap/windows10"
 xmlns:uap10="http://schemas.microsoft.com/appx/manifest/uap/windows10/10"
 xmlns:rescap="http://schemas.microsoft.com/appx/manifest/foundation/windows10/restrictedcapabilities"
 IgnorableNamespaces="uap uap10 rescap">
 <Identity Name="$name" Publisher="$publisher" Version="$($identity.PackageVersion)" ProcessorArchitecture="$architecture"/>
 <Properties><DisplayName>$display</DisplayName><PublisherDisplayName>$publisherDisplay</PublisherDisplayName>
  <Description>Windows Inkを使った軽量な手書きノート</Description><Logo>Assets\StoreLogo.png</Logo></Properties>
 <Dependencies><TargetDeviceFamily Name="Windows.Desktop" MinVersion="10.0.22000.0" MaxVersionTested="10.0.26100.0"/></Dependencies>
 <Resources><Resource Language="ja-JP"/></Resources>
 <Applications><Application Id="MioNote" Executable="MioNote.exe" uap10:RuntimeBehavior="packagedClassicApp" uap10:TrustLevel="mediumIL">
  <uap:VisualElements DisplayName="$display" Description="軽量な手書きノート" BackgroundColor="transparent"
   Square150x150Logo="Assets\Square150x150Logo.png" Square44x44Logo="Assets\Square44x44Logo.png"/>
 </Application></Applications>
 <Capabilities><rescap:Capability Name="runFullTrust"/></Capabilities>
</Package>
"@
    $manifest = Join-Path $folder 'AppxManifest.xml'
    [IO.File]::WriteAllText($manifest,$xml,[Text.UTF8Encoding]::new($false))
    $config = Join-Path $staging "pri-$architecture.xml"
    Invoke-MioSdkTool -Path $makepri -Arguments @('createconfig','/cf',$config,'/dq','ja-JP','/o') -LogPath (Join-Path $staging "makepri-config-$architecture.log")
    Invoke-MioSdkTool -Path $makepri -Arguments @('new','/pr',$folder,'/cf',$config,'/mn',$manifest,'/of',(Join-Path $folder 'resources.pri'),'/o') -LogPath (Join-Path $staging "makepri-$architecture.log")
    $msix = Join-Path $bundleInput "MioNote-$version-store-$architecture.msix"
    # Keep manifest/semantic validation enabled; never use /nv.
    Invoke-MioSdkTool -Path $makeappx -Arguments @('pack','/d',$folder,'/p',$msix,'/h','SHA256','/o') -LogPath (Join-Path $staging "makeappx-$architecture.log")
    Write-Host "Validated and packed: $architecture MSIX"
    Copy-Item -LiteralPath $msix -Destination $dist -Force
    Write-MioHash (Join-Path $dist ([IO.Path]::GetFileName($msix)))
}
$bundle = Join-Path $dist "MioNote-$version-store.msixbundle"
Invoke-MioSdkTool -Path $makeappx -Arguments @('bundle','/d',$bundleInput,'/p',$bundle,'/bv',$identity.PackageVersion,'/o') -LogPath (Join-Path $staging 'makeappx-bundle.log')
Write-MioHash $bundle
Write-Host "Store submission bundle (unsigned): $bundle"
Write-Host "Identity: $($identity.Name); publisher: $($identity.PublisherDisplayName); package version: $($identity.PackageVersion)"
