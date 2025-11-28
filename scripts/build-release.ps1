# Build script for CP Editor on Windows
# Usage: .\scripts\build-release.ps1

$ErrorActionPreference = "Stop"

$ProjectName = "cp-editor"
$ProjectDir = Split-Path -Parent (Split-Path -Parent $PSScriptRoot)
$DistDir = Join-Path $ProjectDir "dist"

# Get version from Cargo.toml
$CargoToml = Get-Content (Join-Path $ProjectDir "Cargo.toml") -Raw
if ($CargoToml -match 'version\s*=\s*"([^"]+)"') {
    $Version = $Matches[1]
} else {
    $Version = "0.1.0"
}

Write-Host "Building CP Editor v$Version for Windows"

# Create dist directory
New-Item -ItemType Directory -Force -Path $DistDir | Out-Null

# Build release binary
Write-Host "Compiling..."
cargo build --release -p cp-editor
if ($LASTEXITCODE -ne 0) {
    Write-Error "Build failed"
    exit 1
}

# Create distribution folder
$WinDist = Join-Path $DistDir "cp-editor-$Version-windows-x86_64"
New-Item -ItemType Directory -Force -Path $WinDist | Out-Null

# Copy binary
$ExePath = Join-Path $ProjectDir "target\release\cp-editor.exe"
Copy-Item $ExePath -Destination $WinDist

# Create zip
$ZipPath = Join-Path $DistDir "cp-editor-$Version-windows-x86_64.zip"
if (Test-Path $ZipPath) {
    Remove-Item $ZipPath
}
Compress-Archive -Path $WinDist -DestinationPath $ZipPath

Write-Host ""
Write-Host "Build complete!"
Write-Host "Binary: $WinDist\cp-editor.exe"
Write-Host "Package: $ZipPath"
