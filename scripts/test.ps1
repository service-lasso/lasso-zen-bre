$ErrorActionPreference = 'Stop'

$root = Split-Path -Parent $PSScriptRoot

$required = @(
  (Join-Path $root 'service.json'),
  (Join-Path $root 'verify\service-harness.json'),
  (Join-Path $root 'runtime\win32\echo-service.ps1')
)

foreach ($path in $required) {
  if (-not (Test-Path $path)) {
    throw "Missing required file: $path"
  }
}

$service = Get-Content (Join-Path $root 'service.json') -Raw | ConvertFrom-Json
if ($service.id -ne 'echo-service') {
  throw 'service.json id mismatch'
}

$manifestPaths = @((Join-Path $root 'service.json'))
$servicesRoot = Join-Path $root 'services'
if (Test-Path $servicesRoot) {
  $manifestPaths += Get-ChildItem -Path $servicesRoot -Recurse -Filter 'service.json' | ForEach-Object { $_.FullName }
}

foreach ($manifestPath in $manifestPaths) {
  $manifest = Get-Content $manifestPath -Raw | ConvertFrom-Json
  if ($manifest.PSObject.Properties.Name -contains 'healthcheck') {
    throw "Singular healthcheck is not allowed in $manifestPath; use healthchecks[]."
  }
  if ($manifest.execconfig -and $manifest.execconfig.PSObject.Properties.Name -contains 'healthcheck') {
    throw "execconfig.healthcheck is not allowed in $manifestPath; use top-level healthchecks[]."
  }
  if ($manifest.PSObject.Properties.Name -contains 'healthchecks') {
    if ($null -eq $manifest.healthchecks -or -not ($manifest.healthchecks -is [array])) {
      throw "healthchecks must be an array in $manifestPath."
    }
    foreach ($check in $manifest.healthchecks) {
      if (-not $check.id) {
        throw "Every healthchecks[] item needs a stable id in $manifestPath."
      }
    }
  }
}

$contract = Get-Content (Join-Path $root 'verify\service-harness.json') -Raw | ConvertFrom-Json
if ($contract.serviceId -ne 'echo-service') {
  throw 'service-harness.json serviceId mismatch'
}

$env:ECHO_MESSAGE = 'pipeline test message'
$output = & (Join-Path $root 'runtime\win32\echo-service.ps1') | Out-String
if ($output -notmatch 'pipeline test message') {
  throw 'Echo runtime output mismatch'
}

Write-Host 'Template tests passed (Windows)'
