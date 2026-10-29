$ErrorActionPreference = 'Stop'
$script:MioProjectRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))

function Get-MioVersion {
    param([string]$Root)
    $manifest = Get-Content -LiteralPath (Join-Path $Root 'Cargo.toml') -Raw
    $version = [regex]::Match($manifest, '(?m)^version\s*=\s*"([^"]+)"').Groups[1].Value
    if ($version -notmatch '^\d+\.\d+\.\d+$') { throw 'Invalid application version.' }
    return $version
}

function Assert-MioExecutable {
    param([string]$Path, [ValidateSet('arm64','x64')][string]$Architecture)
    $reader = [IO.BinaryReader]::new([IO.File]::OpenRead($Path))
    try {
        if ($reader.ReadUInt16() -ne 0x5A4D) { throw 'Missing DOS header.' }
        $reader.BaseStream.Position = 0x3c
        $offset = $reader.ReadUInt32()
        if ($offset -gt $reader.BaseStream.Length - 26) { throw 'Invalid PE header offset.' }
        $reader.BaseStream.Position = $offset
        if ($reader.ReadUInt32() -ne 0x00004550) { throw 'Missing PE header.' }
        $expected = if ($Architecture -eq 'arm64') { 0xAA64 } else { 0x8664 }
        if ($reader.ReadUInt16() -ne $expected) { throw 'Executable architecture mismatch.' }
    } finally { $reader.Dispose() }
    $dumpbin = Get-Command dumpbin.exe -ErrorAction Stop
    $imports = (& $dumpbin.Source /dependents $Path) -join "`n"
    if ($LASTEXITCODE -ne 0) { throw 'Could not inspect executable imports.' }
    if ($imports -match '(?im)^\s*(?:vcruntime\w*|msvcp\w*|msvcr\w*|concrt\w*|ucrtbase)\.dll\s*$' -or
        $imports -match '(?im)^\s*api-ms-win-crt-.*\.dll\s*$') { throw 'Shared VC++/UCRT imports are not allowed.' }
    $headers = (& $dumpbin.Source /headers /loadconfig $Path) -join "`n"
    if ($LASTEXITCODE -ne 0) { throw 'Could not inspect PE security flags.' }
    foreach ($flag in @('High Entropy Virtual Addresses','Dynamic base','NX compatible','Control Flow Guard')) {
        if (-not $headers.Contains($flag)) { throw "Missing security flag: $flag" }
    }
    if ($headers -notmatch '0800\s+Dependent Load Flag') { throw 'Dependent DLL loading must be System32-only.' }
    Write-Host "Verified $Architecture executable: static CRT, ASLR, DEP, CFG, System32 DLL policy."
}

function Get-MioSdkTool {
    param([string]$Name, [string]$SdkBin)
    if ($SdkBin) { $path = Join-Path $SdkBin $Name }
    else {
        $local = Join-Path $script:MioProjectRoot '..\tools\windows-sdk-buildtools\10.0.26100.9169\sdk\bin\10.0.26100.0\arm64'
        if (Test-Path -LiteralPath (Join-Path $local $Name)) { $path = Join-Path $local $Name }
        else { $path = (Get-Command $Name -ErrorAction Stop).Source }
    }
    $path = [IO.Path]::GetFullPath($path)
    $signature = Get-AuthenticodeSignature -LiteralPath $path
    if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Subject -notmatch 'O=Microsoft Corporation(?:,|$)') {
        throw "SDK tool must have a valid Microsoft signature: $path"
    }
    return $path
}

function Copy-MioReleaseDocuments {
    param([string]$Root,[string]$Destination,[switch]$Store)
    foreach ($name in @('LICENSE','distribution/THIRD-PARTY-NOTICES.txt','distribution/PRIVACY.txt')) {
        Copy-MioCheckedFile -Path (Join-Path $Root $name) -Destination $Destination
    }
    $readme = if ($Store) { 'distribution/store/README.txt' } else { 'distribution/README.txt' }
    Copy-MioCheckedFile -Path (Join-Path $Root $readme) -Destination (Join-Path $Destination 'README.txt')
    $licenses = Join-Path $Root 'distribution/licenses'
    $items = @(Get-Item -LiteralPath $licenses) + @(Get-ChildItem -LiteralPath $licenses -Recurse -Force)
    if ($items | Where-Object { $_.Attributes -band [IO.FileAttributes]::ReparsePoint }) { throw 'Release licenses must not contain filesystem links.' }
    Copy-Item -LiteralPath $licenses -Destination $Destination -Recurse
    $security = Join-Path $Destination 'security'
    New-Item -ItemType Directory -Path $security | Out-Null
    foreach ($name in @('security-review-20261027.md','store-security-review-20261027.md','dependency-audit-20261027.json')) {
        Copy-MioCheckedFile -Path (Join-Path $Root "docs/$name") -Destination $security
    }
}

function Copy-MioCheckedFile {
    param([string]$Path,[string]$Destination)
    $item = Get-Item -LiteralPath $Path -Force
    if ($item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw "Expected a regular release file: $Path" }
    Copy-Item -LiteralPath $Path -Destination $Destination
}

function Write-MioHash {
    param([string]$Path)
    $hash = (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant()
    [IO.File]::WriteAllText("$Path.sha256", "$hash  $([IO.Path]::GetFileName($Path))`n", [Text.UTF8Encoding]::new($false))
}

function Invoke-MioSdkTool {
    param([string]$Path,[string[]]$Arguments,[string]$LogPath)
    & $Path @Arguments *> $LogPath
    if ($LASTEXITCODE -ne 0) {
        $message = (Get-Content -LiteralPath $LogPath -Raw).Replace([string][char]0,'')
        Write-Host (($message -split "`n" | Select-Object -Last 12) -join "`n")
        throw "SDK tool failed; inspect $LogPath"
    }
}
