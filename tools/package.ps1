# Builds the release binaries and assembles dist/VECTOR (portable folder + zip).
# Usage: powershell -ExecutionPolicy Bypass -File tools/package.ps1
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root
$env:PATH += ";$env:USERPROFILE\.cargo\bin"
cargo build --release
if ($LASTEXITCODE -ne 0) { throw "build failed" }
$out = Join-Path $root "dist\VECTOR"
if (Test-Path $out) { Remove-Item -Recurse -Force $out }
New-Item -ItemType Directory -Force $out | Out-Null
Copy-Item target\release\vector-game.exe $out
New-Item -ItemType Directory -Force "$out\tools" | Out-Null
foreach ($t in "data_check", "sim_bench", "balance_check") { Copy-Item "target\release\$t.exe" "$out\tools" }
Copy-Item -Recurse data "$out\data"
Copy-Item -Recurse scripts "$out\scripts"
New-Item -ItemType Directory -Force "$out\docs" | Out-Null
Copy-Item docs\*.md "$out\docs"
Copy-Item README.md $out
$zip = Join-Path $root "dist\VECTOR.zip"
if (Test-Path $zip) { Remove-Item $zip }
Compress-Archive -Path $out -DestinationPath $zip
$size = [math]::Round((Get-Item $zip).Length / 1MB, 2)
Write-Host "dist/VECTOR ready ($size MB zip)"
