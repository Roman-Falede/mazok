# Сборка Mazok.exe (Rust) для Windows. Нужны Rust (rustup) и Visual Studio Build Tools с компонентом C++.
# Если путь к папке длинный или содержит кириллицу, задайте короткий CARGO_TARGET_DIR, например C:\mazok-target.
# Автотест интерфейса: $env:MAZOK_AUTOTEST = 'папка для отчёта'; $env:MAZOK_DATA = 'временная папка данных'; затем запустите exe.
$ErrorActionPreference = 'Stop'
$crate = $PSScriptRoot
$root = (Resolve-Path (Join-Path $crate '..')).Path
# cargo пишет ход сборки в stderr — в Windows PowerShell 5.1 это не должно считаться ошибкой
$ErrorActionPreference = 'Continue'
& cargo build --release --bin mazok-desktop --manifest-path (Join-Path $crate 'Cargo.toml')
$code = $LASTEXITCODE
$ErrorActionPreference = 'Stop'
if ($code -ne 0) { throw "cargo завершился с кодом $code" }
$targetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $crate 'target' }
$out = Join-Path $root 'dist\rust'
New-Item -ItemType Directory -Force $out | Out-Null
Copy-Item (Join-Path $targetDir 'release\mazok-desktop.exe') (Join-Path $out 'Mazok.exe') -Force
Write-Output ("Готово: {0} ({1:N1} МБ)" -f (Join-Path $out 'Mazok.exe'), ((Get-Item (Join-Path $out 'Mazok.exe')).Length / 1MB))
