$ErrorActionPreference = 'Stop'

$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$dist = Join-Path $root 'dist'
$staging = Join-Path $dist 'lasso-zen-bre-win32'
$zipPath = Join-Path $dist 'lasso-zen-bre-1.0.0-beta.11-win32.zip'

cargo build --release --locked

New-Item -ItemType Directory -Force -Path $dist | Out-Null
if (Test-Path $staging) { Remove-Item -Recurse -Force $staging }
New-Item -ItemType Directory -Force -Path $staging | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $staging 'decisions') | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $staging 'config') | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $staging 'logs') | Out-Null
New-Item -ItemType Directory -Force -Path (Join-Path $staging '.state') | Out-Null

Copy-Item -Force (Join-Path $root 'target\release\lasso-zen-bre.exe') (Join-Path $staging 'lasso-zen-bre.exe')
Copy-Item -Force (Join-Path $root 'service.json') (Join-Path $staging 'service.json')
Copy-Item -Force (Join-Path $root 'LICENSE') (Join-Path $staging 'LICENSE')
Copy-Item -Force (Join-Path $root 'NOTICE') (Join-Path $staging 'NOTICE')

if (Test-Path $zipPath) { Remove-Item -Force $zipPath }
Compress-Archive -Path (Join-Path $staging '*') -DestinationPath $zipPath
Write-Host "Created $zipPath"
