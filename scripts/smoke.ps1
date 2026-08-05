$ErrorActionPreference = 'Stop'

$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$archive = Join-Path $root 'dist\lasso-zen-bre-1.0.0-beta.11-windows-x64.zip'
$tempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("lasso-zen-bre-" + [guid]::NewGuid().ToString('N'))
$process = $null

try {
  Expand-Archive -Path $archive -DestinationPath $tempDir -Force
  $listener = [System.Net.Sockets.TcpListener]::new([System.Net.IPAddress]::Loopback, 0)
  $listener.Start()
  $port = ([System.Net.IPEndPoint]$listener.LocalEndpoint).Port
  $listener.Stop()

  $stdout = Join-Path $tempDir 'stdout.log'
  $stderr = Join-Path $tempDir 'stderr.log'
  $binary = Join-Path $tempDir 'lasso-zen-bre.exe'
  $arguments = @(
    '--service-root', $tempDir,
    '--host', '127.0.0.1',
    '--port', "$port",
    '--decisions-dir', (Join-Path $tempDir 'decisions')
  )
  $process = Start-Process -FilePath $binary -ArgumentList $arguments -PassThru -RedirectStandardOutput $stdout -RedirectStandardError $stderr

  $ready = $null
  foreach ($attempt in 1..80) {
    try {
      $ready = Invoke-RestMethod -Uri "http://127.0.0.1:$port/health/ready" -TimeoutSec 2
      break
    } catch {
      Start-Sleep -Milliseconds 250
    }
  }
  if ($null -eq $ready -or $ready.status -ne 'ready' -or $ready.engineVersion -ne '1.0.0-beta.11') {
    throw 'Packaged readiness response is invalid'
  }

  $evaluation = Invoke-RestMethod -Method Post -Uri "http://127.0.0.1:$port/v1/decisions/example/evaluate" -ContentType 'application/json' -Body '{}'
  if ($evaluation.result.message -ne 'Hello from Service Lasso ZEN BRE') {
    throw 'Packaged decision evaluation is invalid'
  }

  Stop-Process -Id $process.Id -Force
  $process.WaitForExit()
  $process = $null

  $logs = (Get-Content $stdout -Raw -ErrorAction SilentlyContinue) + (Get-Content $stderr -Raw -ErrorAction SilentlyContinue)
  if ($logs -match 'DO_NOT_LEAK|BEGIN PRIVATE KEY|raw_secret[=:]') {
    throw 'Packaged service logs contain a forbidden sentinel'
  }
  Write-Host 'Packaged windows-x64 smoke test passed'
} finally {
  if ($process -and -not $process.HasExited) {
    $process.Kill()
  }
  if (Test-Path $tempDir) {
    Remove-Item -Recurse -Force $tempDir
  }
}
