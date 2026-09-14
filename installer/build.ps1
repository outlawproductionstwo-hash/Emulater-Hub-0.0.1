param(
    [string]$Version = "0.0.2"
)

$ErrorActionPreference = "Stop"
$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$targetDir = Join-Path $root "target\release"
$outputDir = Join-Path $root "release"
$vcRedist = Join-Path $PSScriptRoot "vc_redist.x64.exe"
$installerScript = Join-Path $PSScriptRoot "EmulatorHub.iss"

if (-not (Test-Path (Join-Path $targetDir "emulator_hub_gui.exe"))) {
    throw "Release executable not found. Run 'cargo build --locked --release' first."
}

if (-not (Test-Path $vcRedist)) {
    Write-Host "Downloading the Microsoft Visual C++ runtime..."
    Invoke-WebRequest `
        -Uri "https://aka.ms/vs/17/release/vc_redist.x64.exe" `
        -OutFile $vcRedist
}

$isccPath = (Get-Command iscc.exe -ErrorAction SilentlyContinue).Source
if (-not $isccPath) {
    $knownPaths = @(
        (Join-Path ${env:ProgramFiles(x86)} "Inno Setup 6\ISCC.exe"),
        (Join-Path $env:ProgramFiles "Inno Setup 6\ISCC.exe"),
        (Join-Path $env:LOCALAPPDATA "Programs\Inno Setup 6\ISCC.exe")
    )
    $isccPath = $knownPaths | Where-Object { Test-Path $_ } | Select-Object -First 1
    if (-not $isccPath) {
        throw "Inno Setup 6 is required. Install it, then run this script again."
    }
}

New-Item -ItemType Directory -Force -Path $outputDir | Out-Null
& $isccPath `
    "/DMyAppVersion=$Version" `
    "/DSourceDir=$targetDir" `
    "/DOutputDir=$outputDir" `
    "/DVCRedistPath=$vcRedist" `
    $installerScript

if ($LASTEXITCODE -ne 0) {
    throw "Inno Setup failed with exit code $LASTEXITCODE"
}

$installer = Join-Path $outputDir "EmulatorHub-v$Version-Setup.exe"
$hash = (Get-FileHash $installer -Algorithm SHA256).Hash.ToLower()
"$hash  $(Split-Path $installer -Leaf)" | Set-Content "$installer.sha256"
Write-Host "Created $installer"
