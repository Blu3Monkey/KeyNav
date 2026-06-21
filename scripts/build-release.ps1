#Requires -Version 5.1
$ErrorActionPreference = "Stop"

$Root = Split-Path -Parent $PSScriptRoot
Set-Location $Root

$Dist = Join-Path $Root "dist"
$Assets = Join-Path $Root "installer\assets"
$Ico = Join-Path $Assets "keynav.ico"

New-Item -ItemType Directory -Force -Path $Dist, $Assets | Out-Null

if (-not (Test-Path $Ico)) {
    Write-Host "Generating placeholder keynav.ico..."
    Add-Type -AssemblyName System.Drawing
    $bmp = New-Object System.Drawing.Bitmap 32, 32
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.Clear([System.Drawing.Color]::FromArgb(255, 255, 102, 0))
    $icon = [System.Drawing.Icon]::FromHandle($bmp.GetHicon())
    $fs = [System.IO.File]::Create($Ico)
    $icon.Save($fs)
    $fs.Close()
    $g.Dispose()
    $bmp.Dispose()
    $icon.Dispose()
}

Write-Host "Building release..."
cargo build --release
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

Copy-Item (Join-Path $Root "target\release\keynav.exe") (Join-Path $Dist "keynav.exe") -Force

$Iscc = @(
    "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
    "${env:ProgramFiles}\Inno Setup 6\ISCC.exe"
) | Where-Object { Test-Path $_ } | Select-Object -First 1

if ($Iscc) {
    Write-Host "Building installer with Inno Setup..."
    & $Iscc (Join-Path $Root "installer\keynav.iss")
} else {
    Write-Warning 'Inno Setup not found - dist\keynav.exe ready; install ISCC to build setup.'
}

Write-Host "Done. Output: $Dist"
