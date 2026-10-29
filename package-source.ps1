param([switch]$GitHub)
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.IO.Compression.FileSystem

$root = $PSScriptRoot
$manifest = Get-Content -LiteralPath (Join-Path $root 'Cargo.toml') -Raw
$version = [regex]::Match($manifest, '(?m)^version\s*=\s*"([^"]+)"').Groups[1].Value
if ($version -notmatch '^\d+\.\d+\.\d+$') { throw 'Invalid package version.' }
$name = if ($GitHub) { "MioNote-$version-github" } else { "MioNote-$version-source" }
$dist = Join-Path $root 'dist'
$githubFolder = Join-Path $dist $name
if ($GitHub -and (Test-Path -LiteralPath $githubFolder)) {
    throw "GitHub folder already exists; preserve or rename it before exporting again: $githubFolder"
}

# An explicit list keeps local notes, settings, build output and credentials out.
$files = @(
    '.cargo/config.toml', '.gitignore', '.gitattributes', 'build.ps1', 'build.rs', 'Cargo.lock', 'Cargo.toml',
    'CHANGELOG.md', 'CONTRIBUTING.md', 'LICENSE', 'package-source.ps1',
    'package.ps1', 'package-store.ps1', 'README.md', 'run.cmd', 'rust-toolchain.toml',
    'scripts/release-common.ps1', 'scripts/verify-packages.py', 'docs/store-release.md', 'docs/store-security-review-20261027.md',
    'docs/github-publish.md', 'docs/images/mio-note-1.0.0.png', 'distribution/store/runFullTrust.txt',
    'docs/wack-validation-20261027.md', 'docs/wack-results-20261027.json',
    'docs/wack-results-0.12.1-20261027.json', 'docs/store-runtime-validation-20261027.md', 'docs/store-runtime-results-20261027.json',
    'docs/wack-results-1.0.0-20261027.json', 'docs/release-validation-1.0.0-20261027.md',
    'distribution/PRIVACY.txt', 'distribution/privacy.html', 'distribution/store/README.txt',
    'distribution/store/identity.json', 'make-store-assets.ps1',
    'distribution/store/Assets/StoreLogo.png', 'distribution/store/Assets/Square44x44Logo.png', 'distribution/store/Assets/Square150x150Logo.png',
    'docs/inknote-format.md', 'docs/pages-and-pdf.md', 'docs/export-formats.md', 'docs/view-controls.md', 'docs/performance.md', 'docs/performance-20261026.csv',
    'distribution/README.txt',
    'docs/security-review-20261027.md', 'docs/dependency-audit-20261027.json',
    'scripts/check-dependencies.py', 'tests/fixtures/v2-mixed-paper.inknote', 'tests/fixtures/README.txt',
    'distribution/THIRD-PARTY-NOTICES.txt', 'make-icons.ps1',
    'assets/mio-note.ico', 'assets/branding/mio-note-v1.png',
    'assets/branding/mio-note-v1.ico', 'assets/branding/mio-note-v2.png', 'assets/branding/mio-note-v2.prompt.txt',
    'assets/branding/mio-note-256.png', 'assets/branding/mio-note-v1.prompt.txt'
)
foreach ($directory in @('src', 'distribution/licenses')) {
    $directoryPath = Join-Path $root $directory
    $items = @(Get-Item -LiteralPath $directoryPath)
    $items += @(Get-ChildItem -LiteralPath $directoryPath -Recurse -Force)
    if ($items | Where-Object { $_.Attributes -band [IO.FileAttributes]::ReparsePoint }) {
        throw "Links are not supported in source packages: $directory"
    }
    foreach ($item in $items | Where-Object { -not $_.PSIsContainer }) {
        if ($directory -eq 'src' -and $item.Extension -ne '.rs') { continue }
        $files += $item.FullName.Substring($root.Length + 1).Replace('\', '/')
    }
}
$files = @($files | Sort-Object -Unique)

$staging = Join-Path $root ('target/source-package/' + [Guid]::NewGuid().ToString('N'))
$folder = Join-Path $staging $name
New-Item -ItemType Directory -Path $folder -Force | Out-Null
$hashes = @()
foreach ($relative in $files) {
    $source = Join-Path $root $relative
    $item = Get-Item -LiteralPath $source -Force
    if ($item.PSIsContainer -or ($item.Attributes -band [IO.FileAttributes]::ReparsePoint)) {
        throw "Expected a regular source file: $relative"
    }
    $destination = Join-Path $folder $relative
    New-Item -ItemType Directory -Path (Split-Path -Parent $destination) -Force | Out-Null
    Copy-Item -LiteralPath $source -Destination $destination
    $hash = (Get-FileHash -LiteralPath $destination -Algorithm SHA256).Hash.ToLowerInvariant()
    $hashes += "$hash  $relative"
}
$utf8 = [Text.UTF8Encoding]::new($false)
if (-not $GitHub) {
    [IO.File]::WriteAllText((Join-Path $folder 'SOURCE-MANIFEST.sha256'), ($hashes -join "`n") + "`n", $utf8)
}

New-Item -ItemType Directory -Path $dist -Force | Out-Null
$zip = Join-Path $dist "$name.zip"
$temporaryZip = Join-Path $dist (".$name." + [Guid]::NewGuid().ToString('N') + '.tmp')
try {
    # ZipFile preserves .cargo and .gitignore as well as normal files.
    [IO.Compression.ZipFile]::CreateFromDirectory($folder, $temporaryZip, [IO.Compression.CompressionLevel]::Optimal, $true)
    Move-Item -LiteralPath $temporaryZip -Destination $zip -Force
} finally {
    if (Test-Path -LiteralPath $temporaryZip) { Remove-Item -LiteralPath $temporaryZip }
}
$zipHash = (Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash.ToLowerInvariant()
[IO.File]::WriteAllText("$zip.sha256", "$zipHash  $name.zip`n", $utf8)
Write-Host "Source ZIP: $zip"
Write-Host "SHA-256: $zipHash"
Write-Host "Source files: $($files.Count)"
if ($GitHub) {
    Copy-Item -LiteralPath $folder -Destination $dist -Recurse
    Write-Host "GitHub folder: $githubFolder"
}
