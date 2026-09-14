param(
    [string]$Version = "0.0.4.2"
)

$ErrorActionPreference = "Stop"
$root = (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
$targetDir = Join-Path $root "target\release"
$outputDir = Join-Path $root "release"
$vcRedist = Join-Path $PSScriptRoot "vc_redist.x64.exe"
$installerScript = Join-Path $PSScriptRoot "EmulatorHub.iss"

function Archive-OldReleaseFiles {
    param(
        [string]$KeepVersion
    )

    if (-not (Test-Path $outputDir)) {
        return
    }

    $oldFiles = Get-ChildItem -LiteralPath $outputDir -File |
        Where-Object {
            $_.Name -like "EmulatorHub-*" -and
            $_.Name -notlike "*$KeepVersion*" -and
            $_.Extension -in @(".exe", ".sha256")
        }

    if (-not $oldFiles) {
        return
    }

    $archiveDir = Join-Path $outputDir "archive"
    New-Item -ItemType Directory -Force -Path $archiveDir | Out-Null
    $archiveName = "EmulatorHub-old-releases-{0}.zip" -f (Get-Date -Format "yyyyMMdd-HHmmss")
    $archivePath = Join-Path $archiveDir $archiveName
    Compress-Archive -LiteralPath $oldFiles.FullName -DestinationPath $archivePath -CompressionLevel Optimal

    if (Test-Path $archivePath) {
        $removedFiles = @()
        foreach ($oldFile in $oldFiles) {
            try {
                Remove-Item -LiteralPath $oldFile.FullName -Force -ErrorAction Stop
                $removedFiles += $oldFile
            }
            catch {
                Write-Warning "Could not remove $($oldFile.Name). Close Emulator Hub and remove it later."
            }
        }
        Write-Host "Archived $($removedFiles.Count) old release file(s) to $archivePath"
    }
}

Archive-OldReleaseFiles -KeepVersion $Version

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
$standalone = Join-Path $outputDir "EmulatorHub-v$Version-windows-x86_64.exe"
Copy-Item (Join-Path $targetDir "emulator_hub_gui.exe") $standalone -Force
$standaloneHash = (Get-FileHash $standalone -Algorithm SHA256).Hash.ToLower()
"$standaloneHash  $(Split-Path $standalone -Leaf)" |
    Set-Content "$standalone.sha256"

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
