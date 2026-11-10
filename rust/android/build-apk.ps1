# Сборка Mazok.apk из Rust-версии без Android Studio и Gradle.
# Нужны: Rust с целью aarch64-linux-android (rustup target add aarch64-linux-android), JDK 17+ (java в PATH)
# и папка Android SDK с build-tools\35.0.0, platforms\android-35 и ndk\<версия NDK>.
param(
  [string]$Sdk = $env:ANDROID_HOME,
  [string]$Ndk = '',
  [string]$VersionName = '3.1.0',
  [int]$VersionCode = 4,
  [switch]$SkipCargo
)
$ErrorActionPreference = 'Stop'
$here = $PSScriptRoot
$crate = (Resolve-Path (Join-Path $here '..')).Path
$root = (Resolve-Path (Join-Path $here '..\..')).Path
if (-not $Sdk -or -not (Test-Path $Sdk)) { throw 'Укажите папку Android SDK: -Sdk <путь> или переменную ANDROID_HOME' }
if (-not $Ndk) { $Ndk = (Get-ChildItem (Join-Path $Sdk 'ndk') -Directory | Sort-Object Name -Descending | Select-Object -First 1).FullName }
$bt = (Get-ChildItem (Join-Path $Sdk 'build-tools') -Directory | Sort-Object Name -Descending | Select-Object -First 1).FullName
$aapt2 = Join-Path $bt 'aapt2.exe'; $zipalign = Join-Path $bt 'zipalign.exe'; $apksigner = Join-Path $bt 'apksigner.bat'; $d8 = Join-Path $bt 'd8.bat'
$jar = Join-Path $Sdk 'platforms\android-35\android.jar'
$llvm = Join-Path $Ndk 'toolchains\llvm\prebuilt\windows-x86_64\bin'
foreach ($p in $aapt2, $zipalign, $apksigner, $d8, $jar, $llvm) { if (-not (Test-Path $p)) { throw "Не найден $p" } }
# Инструменты пишут ход работы в stderr — в Windows PowerShell 5.1 это не должно считаться ошибкой
function Run($exe, [string[]]$a) {
  $ErrorActionPreference = 'Continue'
  & $exe @a
  $code = $LASTEXITCODE
  $ErrorActionPreference = 'Stop'
  if ($code -ne 0) { throw "$([IO.Path]::GetFileName($exe)) завершился с кодом $code" }
}

# 1. Библиотека libmazok.so
$target = 'aarch64-linux-android'
$clang = Join-Path $llvm 'aarch64-linux-android26-clang.cmd'
$env:CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER = $clang
$env:CC_aarch64_linux_android = $clang
$env:CXX_aarch64_linux_android = Join-Path $llvm 'aarch64-linux-android26-clang++.cmd'
$env:AR_aarch64_linux_android = Join-Path $llvm 'llvm-ar.exe'
$targetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $crate 'target' }
if (-not $SkipCargo) { Run 'cargo' @('build', '--release', '--lib', '--target', $target, '--manifest-path', (Join-Path $crate 'Cargo.toml')) }
$so = Join-Path $targetDir "$target\release\libmazok.so"
if (-not (Test-Path $so)) { throw "Не найдена библиотека $so" }

# aapt2 и zipalign не работают с не-латинскими путями — временная папка должна быть в ASCII
$base = [IO.Path]::GetTempPath()
try { $base = (New-Object -ComObject Scripting.FileSystemObject).GetFolder($base).ShortPath } catch { }
if ($base -match '[^\x20-\x7E]') { $base = Join-Path $env:PUBLIC 'Documents' }
$tmp = Join-Path $base ('mazok-rs-apk-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory $tmp | Out-Null
$out = Join-Path $root 'dist\rust'
New-Item -ItemType Directory -Force $out | Out-Null
try {
  Copy-Item $jar (Join-Path $tmp 'android.jar')
  Copy-Item (Join-Path $here 'AndroidManifest.xml') (Join-Path $tmp 'AndroidManifest.xml')
  Copy-Item $so (Join-Path $tmp 'libmazok.so')
  # 2. Ресурсы и значки (общие с прежней версией)
  $res = Join-Path $tmp 'res'
  Copy-Item (Join-Path $root 'build\android\res') $res -Recurse
  Add-Type -AssemblyName System.Drawing
  function SavePng($srcPath, $size, $dst) {
    $src = [Drawing.Image]::FromFile($srcPath)
    $bmp = New-Object Drawing.Bitmap $size, $size
    $g = [Drawing.Graphics]::FromImage($bmp)
    $g.InterpolationMode = 'HighQualityBicubic'; $g.SmoothingMode = 'HighQuality'; $g.PixelOffsetMode = 'HighQuality'
    $g.DrawImage($src, 0, 0, $size, $size)
    New-Item -ItemType Directory -Force (Split-Path $dst) | Out-Null
    $bmp.Save($dst, [Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose(); $bmp.Dispose(); $src.Dispose()
  }
  $dens = @{ 'mdpi' = 1; 'hdpi' = 1.5; 'xhdpi' = 2; 'xxhdpi' = 3; 'xxxhdpi' = 4 }
  foreach ($k in $dens.Keys) {
    SavePng (Join-Path $root 'icons\icon-512.png') ([int](48 * $dens[$k])) (Join-Path $res "mipmap-$k\ic_launcher.png")
    SavePng (Join-Path $root 'icons\icon-maskable-512.png') ([int](108 * $dens[$k])) (Join-Path $res "mipmap-$k\ic_launcher_fg.png")
  }
  # 3. aapt2: ресурсы и манифест (targetSdk 34 — без принудительного режима edge-to-edge)
  Run $aapt2 @('compile', '--dir', $res, '-o', (Join-Path $tmp 'res.zip'))
  Run $aapt2 @('link', '-o', (Join-Path $tmp 'base.apk'), '-I', (Join-Path $tmp 'android.jar'), '--manifest', (Join-Path $tmp 'AndroidManifest.xml'),
    '--min-sdk-version', '26', '--target-sdk-version', '34', '--version-code', "$VersionCode", '--version-name', $VersionName,
    '--auto-add-overlay', (Join-Path $tmp 'res.zip'))
  # 4. Java-часть (выбор файлов и папок, буфер обмена) → classes.dex
  $cls = Join-Path $tmp 'classes'; New-Item -ItemType Directory $cls | Out-Null
  Copy-Item (Join-Path $here 'MazokActivity.java') (Join-Path $tmp 'MazokActivity.java')
  Run 'javac' @('--release', '11', '-encoding', 'UTF-8', '-nowarn', '-classpath', (Join-Path $tmp 'android.jar'), '-d', $cls, (Join-Path $tmp 'MazokActivity.java'))
  $classFiles = @(Get-ChildItem $cls -Recurse -Filter *.class | ForEach-Object { $_.FullName })
  Run $d8 (@('--release', '--min-api', '26', '--lib', (Join-Path $tmp 'android.jar'), '--output', $tmp) + $classFiles)
  # 5. Добавляем библиотеку и код, выравниваем по страницам 16 КБ, подписываем
  Run 'java' @((Join-Path $here 'ApkAdd.java'), (Join-Path $tmp 'base.apk'), (Join-Path $tmp 'unsigned.apk'), ('lib/arm64-v8a/libmazok.so=' + (Join-Path $tmp 'libmazok.so')), ('classes.dex=' + (Join-Path $tmp 'classes.dex')))
  Run $zipalign @('-P', '16', '-f', '4', (Join-Path $tmp 'unsigned.apk'), (Join-Path $tmp 'aligned.apk'))
  $ks = Join-Path $root 'build\android\mazok-release.keystore'; $kp = Join-Path $root 'build\android\keystore.password'
  if (-not (Test-Path $ks)) {
    $pw = -join ((48..57) + (65..90) + (97..122) | Get-Random -Count 24 | ForEach-Object { [char]$_ })
    [IO.File]::WriteAllText($kp, $pw)
    Run 'keytool' @('-genkeypair', '-keystore', $ks, '-alias', 'mazok', '-keyalg', 'RSA', '-keysize', '2048', '-validity', '10000',
      '-storepass', $pw, '-keypass', $pw, '-dname', 'CN=Mazok, O=Mazok')
  }
  Run $apksigner @('sign', '--ks', $ks, '--ks-key-alias', 'mazok', '--ks-pass', "file:$kp", '--out', (Join-Path $tmp 'Mazok.apk'), (Join-Path $tmp 'aligned.apk'))
  Run $zipalign @('-c', '-P', '16', '4', (Join-Path $tmp 'Mazok.apk'))
  Copy-Item (Join-Path $tmp 'Mazok.apk') (Join-Path $out 'Mazok.apk') -Force
  Run $apksigner @('verify', (Join-Path $out 'Mazok.apk'))
  Write-Output ("Готово: {0} ({1:N1} МБ)" -f (Join-Path $out 'Mazok.apk'), ((Get-Item (Join-Path $out 'Mazok.apk')).Length / 1MB))
} finally {
  try { [IO.Directory]::Delete($tmp, $true) } catch { }
}
