# Télécharge les outils tiers Windows depuis leurs sources officielles et les place dans tools/windows/ :
#   - adb (Android SDK Platform-Tools, Google) : adb.exe + AdbWinApi.dll + AdbWinUsbApi.dll
#   - TestDisk / PhotoRec (CGSecurity) : testdisk_win.exe, photorec_win.exe et leurs DLL, dans tools/windows/testdisk/
#   - The Sleuth Kit (Brian Carrier) : tsk_recover.exe, mmls.exe, fls.exe et leurs DLL, dans tools/windows/sleuthkit/
# smartctl.exe n'est pas téléchargé ici (voir la CI : paquet smartmontools).
#
# Un outil déjà présent est sauté (option -Force pour tout retélécharger). Chaque archive est gardée
# dans tools/windows/archives/ et sa somme SHA-256 est inscrite dans tools/windows/VERSIONS.txt.
# Usage (PowerShell) : ./tools/fetch-tools-windows.ps1 [-Force]
param([switch]$Force)
$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
$dest = Join-Path $root "tools\windows"
$archives = Join-Path $dest "archives"
New-Item -ItemType Directory -Force $archives | Out-Null

$sources = @(
    @{ Name = "platform-tools"; Url = "https://dl.google.com/android/repository/platform-tools-latest-windows.zip"; Check = "adb.exe" },
    @{ Name = "testdisk"; Url = "https://www.cgsecurity.org/testdisk-7.2.win64.zip"; Check = "testdisk\testdisk_win.exe" },
    @{ Name = "sleuthkit"; Url = "https://github.com/sleuthkit/sleuthkit/releases/download/sleuthkit-4.15.0/sleuthkit-4.15.0-win32.zip"; Check = "sleuthkit\tsk_recover.exe" }
)

$versionsFile = Join-Path $dest "VERSIONS.txt"
$versions = if ((Test-Path $versionsFile) -and -not $Force) { @(Get-Content $versionsFile) } else { @("# Outils tiers (sources officielles)", "") }

foreach ($s in $sources) {
    if (-not $Force -and (Test-Path (Join-Path $dest $s.Check))) {
        Write-Host "$($s.Name) déjà présent : sauté"
        continue
    }
    $zip = Join-Path $archives "$($s.Name).zip"
    Write-Host "Téléchargement de $($s.Url)"
    Invoke-WebRequest -Uri $s.Url -OutFile $zip -UseBasicParsing
    $hash = (Get-FileHash $zip -Algorithm SHA256).Hash.ToLower()
    $size = [math]::Round((Get-Item $zip).Length / 1MB, 1)
    Write-Host "  $size Mo, SHA-256 $hash"
    $versions += "$($s.Name) : $($s.Url) (téléchargé le $(Get-Date -Format 'yyyy-MM-dd'))"
    $versions += "  sha256 $hash"

    $tmp = Join-Path $archives "$($s.Name)-extrait"
    if (Test-Path $tmp) { Remove-Item -Recurse -Force $tmp }
    Expand-Archive -Path $zip -DestinationPath $tmp

    switch ($s.Name) {
        "platform-tools" {
            foreach ($f in "adb.exe", "AdbWinApi.dll", "AdbWinUsbApi.dll") {
                Copy-Item (Join-Path $tmp "platform-tools\$f") (Join-Path $dest $f) -Force
            }
            $v = (& (Join-Path $dest "adb.exe") version | Select-Object -First 2) -join " / "
            $versions += "  $v"
        }
        "testdisk" {
            # photorec_win.exe et testdisk_win.exe ont besoin des DLL livrées à côté : on garde tout.
            $inner = Get-ChildItem $tmp -Directory | Select-Object -First 1
            $target = Join-Path $dest "testdisk"
            if (Test-Path $target) { Remove-Item -Recurse -Force $target }
            Copy-Item $inner.FullName $target -Recurse
            if (-not (Test-Path (Join-Path $target "photorec_win.exe"))) { throw "photorec_win.exe absent de l'archive" }
            $versions += "  $($inner.Name)"
        }
        "sleuthkit" {
            # Les exécutables sont dans bin/ avec leurs DLL (libewf, zlib...).
            $bin = Get-ChildItem $tmp -Recurse -Filter "tsk_recover.exe" | Select-Object -First 1
            if (-not $bin) { throw "tsk_recover.exe absent de l'archive" }
            $target = Join-Path $dest "sleuthkit"
            if (Test-Path $target) { Remove-Item -Recurse -Force $target }
            Copy-Item $bin.DirectoryName $target -Recurse
            $versions += "  $((& (Join-Path $target 'tsk_recover.exe') -V 2>&1 | Select-Object -First 1))"
        }
    }
    Remove-Item -Recurse -Force $tmp
    $versions += ""
}

$versions | Set-Content $versionsFile -Encoding utf8
Write-Host "Terminé. Détails dans tools\windows\VERSIONS.txt"
