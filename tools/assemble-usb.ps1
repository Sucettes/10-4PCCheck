# Assemble le dossier de la clé USB dans dist-usb/ (à copier tel quel à la racine de la clé,
# ou dans un dossier 10-4PCCheck/ de la clé) :
#   dist-usb/
#     windows/   10-4-pccheck.exe + tools/ (smartctl, adb, testdisk/photorec)
#     linux/     10-4-pccheck.AppImage + tools/ (si construits)
#     rapports/  rapports générés (JSON, HTML, PDF)
#     recup/     destination proposée par défaut pour PhotoRec
#     LISEZMOI.txt
# Usage : ./tools/assemble-usb.ps1 [-Build]   (-Build recompile l'exe avant)
param([switch]$Build)
$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
$usb = Join-Path $root "dist-usb"
$win = Join-Path $usb "windows"

if ($Build) {
    $env:PATH = "$env:USERPROFILE\.cargo\bin;$env:PATH"
    Push-Location (Join-Path $root "app")
    npx tauri build --no-bundle
    Pop-Location
}

$exe = Join-Path $root "target\release\pccheck.exe"
if (-not (Test-Path $exe)) { throw "Exe absent : lancer avec -Build" }

New-Item -ItemType Directory -Force (Join-Path $win "tools"), (Join-Path $usb "rapports"), (Join-Path $usb "recup") | Out-Null
try {
    Copy-Item $exe (Join-Path $win "10-4-pccheck.exe") -Force -ErrorAction Stop
} catch {
    # Application ouverte : l'exécutable est verrouillé. Les outils sont quand même copiés.
    Write-Warning "10-4-pccheck.exe en cours d'utilisation : non remplacé (ferme l'application et relance ce script)"
}

$tools = Join-Path $root "tools\windows"
foreach ($f in "smartctl.exe", "adb.exe", "AdbWinApi.dll", "AdbWinUsbApi.dll", "VERSIONS.txt") {
    $src = Join-Path $tools $f
    if (Test-Path $src) { Copy-Item $src (Join-Path $win "tools\$f") -Force } else { Write-Warning "$f absent (tools\windows)" }
}
$testdisk = Join-Path $tools "testdisk"
if (Test-Path $testdisk) {
    Copy-Item $testdisk (Join-Path $win "tools") -Recurse -Force
} else {
    Write-Warning "PhotoRec absent : lancer tools\fetch-tools-windows.ps1"
}
$sleuthkit = Join-Path $tools "sleuthkit"
if (Test-Path $sleuthkit) {
    Copy-Item $sleuthkit (Join-Path $win "tools") -Recurse -Force
} else {
    Write-Warning "The Sleuth Kit absent : lancer tools\fetch-tools-windows.ps1"
}

$appimage = Join-Path $root "dist-usb\linux\10-4-pccheck.AppImage"
if (-not (Test-Path $appimage)) { Write-Warning "AppImage Linux absente (voir tools/build-appimage.sh ou la CI)" }

Copy-Item (Join-Path $root "docs\LISEZMOI-CLE.txt") (Join-Path $usb "LISEZMOI.txt") -Force
Write-Host "Clé prête dans $usb"
