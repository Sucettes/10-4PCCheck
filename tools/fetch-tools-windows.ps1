# Télécharge les outils tiers Windows depuis leurs sources officielles et les place dans tools/windows/ :
#   - adb (Android SDK Platform-Tools, Google) : adb.exe + AdbWinApi.dll + AdbWinUsbApi.dll
#   - PhotoRec (TestDisk, CGSecurity) : photorec_win.exe et ses DLL, dans tools/windows/testdisk/
# smartctl.exe n'est pas téléchargé ici (voir la CI : paquet smartmontools).
#
# Chaque archive est gardée dans tools/windows/archives/ et sa somme SHA-256 est inscrite dans
# tools/windows/VERSIONS.txt, pour pouvoir vérifier plus tard que rien n'a changé.
# Usage (PowerShell) : ./tools/fetch-tools-windows.ps1
$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
$dest = Join-Path $root "tools\windows"
$archives = Join-Path $dest "archives"
New-Item -ItemType Directory -Force $archives | Out-Null

$sources = @(
    @{ Name = "platform-tools"; Url = "https://dl.google.com/android/repository/platform-tools-latest-windows.zip" },
    @{ Name = "testdisk"; Url = "https://www.cgsecurity.org/testdisk-7.2.win64.zip" }
)

$versions = @("# Outils tiers téléchargés le $(Get-Date -Format 'yyyy-MM-dd HH:mm')", "")
foreach ($s in $sources) {
    $zip = Join-Path $archives "$($s.Name).zip"
    Write-Host "Téléchargement de $($s.Url)"
    Invoke-WebRequest -Uri $s.Url -OutFile $zip -UseBasicParsing
    $hash = (Get-FileHash $zip -Algorithm SHA256).Hash.ToLower()
    $size = [math]::Round((Get-Item $zip).Length / 1MB, 1)
    Write-Host "  $size Mo, SHA-256 $hash"
    $versions += "$($s.Name) : $($s.Url)"
    $versions += "  sha256 $hash"

    $tmp = Join-Path $archives "$($s.Name)-extrait"
    if (Test-Path $tmp) { Remove-Item -Recurse -Force $tmp }
    Expand-Archive -Path $zip -DestinationPath $tmp

    if ($s.Name -eq "platform-tools") {
        foreach ($f in "adb.exe", "AdbWinApi.dll", "AdbWinUsbApi.dll") {
            Copy-Item (Join-Path $tmp "platform-tools\$f") (Join-Path $dest $f) -Force
        }
        $v = (& (Join-Path $dest "adb.exe") version | Select-Object -First 2) -join " / "
        $versions += "  $v"
    } else {
        # photorec_win.exe a besoin des DLL livrées à côté : on garde tout le dossier.
        $inner = Get-ChildItem $tmp -Directory | Select-Object -First 1
        $target = Join-Path $dest "testdisk"
        if (Test-Path $target) { Remove-Item -Recurse -Force $target }
        Copy-Item $inner.FullName $target -Recurse
        if (-not (Test-Path (Join-Path $target "photorec_win.exe"))) { throw "photorec_win.exe absent de l'archive" }
        $versions += "  $($inner.Name)"
    }
    Remove-Item -Recurse -Force $tmp
    $versions += ""
}

$versions | Set-Content (Join-Path $dest "VERSIONS.txt") -Encoding utf8
Write-Host "Terminé. Détails dans tools\windows\VERSIONS.txt"
