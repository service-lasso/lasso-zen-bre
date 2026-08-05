$ErrorActionPreference = 'Stop'

$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$engineVersion = '1.0.0-beta.11'
$targetTriple = if ($env:TARGET_TRIPLE) { $env:TARGET_TRIPLE } else { 'x86_64-pc-windows-msvc' }
$assetPlatform = if ($env:ASSET_PLATFORM) { $env:ASSET_PLATFORM } else { 'windows-x64' }
if ($targetTriple -ne 'x86_64-pc-windows-msvc' -or $assetPlatform -ne 'windows-x64') {
  throw "Unsupported Windows package target: $targetTriple / $assetPlatform"
}

$dist = Join-Path $root 'dist'
$staging = Join-Path $dist "staging-$assetPlatform"
$zipPath = Join-Path $dist "lasso-zen-bre-$engineVersion-$assetPlatform.zip"
$binary = Join-Path $root "target\$targetTriple\release\lasso-zen-bre.exe"

cargo build --release --locked --target $targetTriple

New-Item -ItemType Directory -Force -Path $dist | Out-Null
if (Test-Path $staging) { Remove-Item -Recurse -Force $staging }
New-Item -ItemType Directory -Force -Path $staging | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $staging 'decisions') | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $staging 'config') | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $staging 'logs') | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $staging '.state') | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $staging 'THIRD_PARTY_LICENSES') | Out-Null

Copy-Item -Force $binary (Join-Path $staging 'lasso-zen-bre.exe')
Copy-Item -Force (Join-Path $root 'service.json') (Join-Path $staging 'service.json')
Copy-Item -Force (Join-Path $root 'LICENSE') (Join-Path $staging 'LICENSE')
Copy-Item -Force (Join-Path $root 'NOTICE') (Join-Path $staging 'NOTICE')
Copy-Item -Force (Join-Path $root 'README.md') (Join-Path $staging 'README.md')
Copy-Item -Force (Join-Path $root 'THIRD_PARTY_LICENSES\zen-engine-MIT.txt') (Join-Path $staging 'THIRD_PARTY_LICENSES\zen-engine-MIT.txt')
Copy-Item -Force (Join-Path $root 'examples\decisions\example.json') (Join-Path $staging 'decisions\example.json')

if (Test-Path $zipPath) { Remove-Item -Force $zipPath }
Compress-Archive -Path (Join-Path $staging '*') -DestinationPath $zipPath

$entries = [System.IO.Compression.ZipFile]::OpenRead($zipPath)
try {
  $names = @($entries.Entries | ForEach-Object { $_.FullName.Replace('\', '/') })
  foreach ($required in @('lasso-zen-bre.exe', 'service.json', 'decisions/example.json')) {
    if ($names -notcontains $required) {
      throw "Archive is missing $required"
    }
  }
} finally {
  $entries.Dispose()
}

Write-Host "Created $zipPath"
