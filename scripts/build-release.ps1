param(
    [switch]$SkipTests
)

$ErrorActionPreference = "Stop"

$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$cargoToml = Get-Content (Join-Path $root "Cargo.toml") -Raw
$versionMatch = [regex]::Match($cargoToml, '(?m)^version\s*=\s*"([^"]+)"')
if (-not $versionMatch.Success) {
    throw "Не удалось определить версию из Cargo.toml"
}
$version = $versionMatch.Groups[1].Value
if ($version -notmatch '^\d+\.\d+\.\d+$') {
    throw "Для установщика версия должна иметь формат x.y.z, получено: $version"
}

$exe = Join-Path $root "target\release\crosshair-overlay.exe"
$dist = Join-Path $root "dist"
$portableDir = Join-Path $dist "portable"
$portableZip = Join-Path $dist "CrosshairOverlay-$version-portable.zip"
$installer = Join-Path $dist "CrosshairOverlay-Setup-$version.exe"
$icon = Join-Path $root "assets\icon.ico"
$license = Join-Path $root "LICENSE"
$readme = Join-Path $root "README.md"
$nsi = Join-Path $root "installer\crosshair-overlay.nsi"

if (-not $SkipTests) {
    cargo test --locked
    if ($LASTEXITCODE -ne 0) { throw "Тесты завершились с ошибкой" }
}

cargo build --release --locked
if ($LASTEXITCODE -ne 0) { throw "Сборка завершилась с ошибкой" }
if (-not (Test-Path $exe)) { throw "Не найден exe: $exe" }

if (Test-Path $dist) {
    Remove-Item $dist -Recurse -Force
}
New-Item -ItemType Directory -Force -Path $portableDir | Out-Null

Copy-Item $exe (Join-Path $portableDir "crosshair-overlay.exe")
Copy-Item $license (Join-Path $portableDir "LICENSE.txt")
Copy-Item $readme (Join-Path $portableDir "README.md")
Compress-Archive -Path (Join-Path $portableDir "*") -DestinationPath $portableZip -Force
Remove-Item $portableDir -Recurse -Force

$makensis = $null
$command = Get-Command makensis.exe -ErrorAction SilentlyContinue
if ($command) {
    $makensis = $command.Source
} else {
    $candidates = @(
        (Join-Path ${env:ProgramFiles(x86)} "NSIS\makensis.exe"),
        (Join-Path $env:ProgramFiles "NSIS\makensis.exe")
    )
    foreach ($candidate in $candidates) {
        if (Test-Path $candidate) {
            $makensis = $candidate
            break
        }
    }
}
if (-not $makensis) {
    throw "NSIS не найден. Установите его командой: choco install nsis"
}

& $makensis `
    "/DAPP_VERSION=$version" `
    "/DAPP_EXE=$exe" `
    "/DICON_FILE=$icon" `
    "/DLICENSE_FILE=$license" `
    "/DOUT_FILE=$installer" `
    $nsi
if ($LASTEXITCODE -ne 0) { throw "NSIS завершился с ошибкой" }
if (-not (Test-Path $installer)) { throw "Не найден установщик: $installer" }

$hashes = @($installer, $portableZip) | ForEach-Object {
    $hash = (Get-FileHash $_ -Algorithm SHA256).Hash.ToLower()
    "$hash  $(Split-Path -Leaf $_)"
}
$hashes | Set-Content (Join-Path $dist "SHA256SUMS.txt") -Encoding ascii

Write-Host ""
Write-Host "Готово:"
Write-Host "  $installer"
Write-Host "  $portableZip"
Write-Host "  $(Join-Path $dist 'SHA256SUMS.txt')"
