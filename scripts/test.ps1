$ErrorActionPreference = 'Stop'

$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$required = @(
  (Join-Path $root 'Cargo.toml'),
  (Join-Path $root 'service.json'),
  (Join-Path $root 'verify\service-harness.json'),
  (Join-Path $root 'src\main.rs'),
  (Join-Path $root 'NOTICE')
)

foreach ($path in $required) {
  if (-not (Test-Path $path)) {
    throw "Missing required file: $path"
  }
}

$service = Get-Content (Join-Path $root 'service.json') -Raw | ConvertFrom-Json
if ($service.id -ne 'zen-bre') {
  throw 'service.json id mismatch'
}
if ($service.enabled -ne $false) {
  throw 'zen-bre must stay disabled by default'
}
if ($service.meta.upstream.crate -ne 'zen-engine' -or $service.meta.upstream.version -ne '1.0.0-beta.11') {
  throw 'upstream zen-engine pin mismatch'
}
if (-not ($service.execconfig.globalenv.PSObject.Properties.Name -contains 'ZEN_BRE_URL')) {
  throw 'ZEN_BRE_URL global export missing'
}

if ($service.PSObject.Properties.Name -contains 'healthcheck') {
  throw 'Singular healthcheck is not allowed; use healthchecks[].'
}
if ($service.execconfig -and $service.execconfig.PSObject.Properties.Name -contains 'healthcheck') {
  throw 'execconfig.healthcheck is not allowed; use top-level healthchecks[].'
}
if ($null -eq $service.healthchecks -or -not ($service.healthchecks -is [array])) {
  throw 'healthchecks must be an array.'
}

$contract = Get-Content (Join-Path $root 'verify\service-harness.json') -Raw | ConvertFrom-Json
if ($contract.serviceId -ne 'zen-bre') {
  throw 'service-harness.json serviceId mismatch'
}

$cargo = Get-Content (Join-Path $root 'Cargo.toml') -Raw
if ($cargo -notmatch 'zen-engine = \{ version = "=1\.0\.0-beta\.11", features = \["arbitrary_precision"\] \}') {
  throw 'Cargo.toml must pin zen-engine 1.0.0-beta.11 with arbitrary_precision enabled'
}

cargo test --locked

Write-Host 'lasso-zen-bre tests passed (Windows)'
