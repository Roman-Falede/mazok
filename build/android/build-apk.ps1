# Сборка Mazok.apk без Android Studio и Gradle.
# Нужны: JDK 17+ (java, javac, keytool в PATH), Android build-tools 35 (папка sdk\build-tools) и android.jar платформы 35 (sdk\android.jar).
param(
  [string]$BuildTools = (Join-Path $PSScriptRoot 'sdk\build-tools'),
  [string]$AndroidJar = (Join-Path $PSScriptRoot 'sdk\android.jar'),
  [string]$VersionName = '2.0.0',
  [int]$VersionCode = 2
)
$ErrorActionPreference = 'Stop'
$here = $PSScriptRoot
$root = (Resolve-Path (Join-Path $here '..\..')).Path
$out = Join-Path $root 'dist'
New-Item -ItemType Directory -Force $out | Out-Null
$aapt2 = Join-Path $BuildTools 'aapt2.exe'; $zipalign = Join-Path $BuildTools 'zipalign.exe'
$d8 = Join-Path $BuildTools 'd8.bat'; $apksigner = Join-Path $BuildTools 'apksigner.bat'
foreach ($p in $aapt2, $zipalign, $d8, $apksigner, $AndroidJar) { if (-not (Test-Path $p)) { throw "Не найден $p" } }
# aapt2 и zipalign не работают с не-латинскими путями — временная папка должна быть в ASCII
$base = [IO.Path]::GetTempPath()
try { $base = (New-Object -ComObject Scripting.FileSystemObject).GetFolder($base).ShortPath } catch { }
if ($base -match '[^\x20-\x7E]') { $base = Join-Path $env:PUBLIC 'Documents' }
$tmp = Join-Path $base ('mazok-apk-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory $tmp | Out-Null
Copy-Item $AndroidJar (Join-Path $tmp 'android.jar'); $AndroidJar = Join-Path $tmp 'android.jar'
Copy-Item (Join-Path $here 'AndroidManifest.xml') (Join-Path $tmp 'AndroidManifest.xml')
function Run($exe, [string[]]$a) { & $exe @a; if ($LASTEXITCODE -ne 0) { throw "$([IO.Path]::GetFileName($exe)) завершился с кодом $LASTEXITCODE" } }
try {
  # 1. ресурсы и значки
  $res = Join-Path $tmp 'res'
  Copy-Item (Join-Path $here 'res') $res -Recurse
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
  # 2. веб-приложение -> assets/www
  $www = Join-Path $tmp 'assets\www'
  New-Item -ItemType Directory -Force $www | Out-Null
  foreach ($f in 'index.html', 'manifest.webmanifest', 'css', 'js', 'icons') { Copy-Item (Join-Path $root $f) $www -Recurse }
  # 3. aapt2: компиляция и сборка ресурсов
  Run $aapt2 @('compile', '--dir', $res, '-o', (Join-Path $tmp 'res.zip'))
  $gen = Join-Path $tmp 'gen'; New-Item -ItemType Directory $gen | Out-Null
  Run $aapt2 @('link', '-o', (Join-Path $tmp 'base.apk'), '-I', $AndroidJar, '--manifest', (Join-Path $tmp 'AndroidManifest.xml'),
    '--min-sdk-version', '24', '--target-sdk-version', '35', '--version-code', "$VersionCode", '--version-name', $VersionName,
    '-A', (Join-Path $tmp 'assets'), '--java', $gen, '--auto-add-overlay', (Join-Path $tmp 'res.zip'))
  # 4. javac + d8
  $cls = Join-Path $tmp 'classes'; New-Item -ItemType Directory $cls | Out-Null
  $srcs = @(Get-ChildItem -Recurse (Join-Path $here 'src'), $gen -Filter *.java | ForEach-Object { $_.FullName })
  Run 'javac' (@('-nowarn', '-encoding', 'UTF-8', '-source', '11', '-target', '11', '-classpath', $AndroidJar, '-d', $cls) + $srcs)
  $dex = Join-Path $tmp 'dex'; New-Item -ItemType Directory $dex | Out-Null
  $classFiles = @(Get-ChildItem -Recurse $cls -Filter *.class | ForEach-Object { $_.FullName })
  Run $d8 (@('--release', '--min-api', '24', '--lib', $AndroidJar, '--output', $dex) + $classFiles)
  # 5. APK = ресурсы + classes.dex, выравнивание, подпись
  Run 'java' @((Join-Path $here 'ApkMerge.java'), (Join-Path $tmp 'base.apk'), (Join-Path $dex 'classes.dex'), (Join-Path $tmp 'unsigned.apk'))
  Run $zipalign @('-p', '-f', '4', (Join-Path $tmp 'unsigned.apk'), (Join-Path $tmp 'aligned.apk'))
  $ks = Join-Path $here 'mazok-release.keystore'; $kp = Join-Path $here 'keystore.password'
  if (-not (Test-Path $ks)) {
    $pw = -join ((48..57) + (65..90) + (97..122) | Get-Random -Count 24 | ForEach-Object { [char]$_ })
    [IO.File]::WriteAllText($kp, $pw)
    Run 'keytool' @('-genkeypair', '-keystore', $ks, '-alias', 'mazok', '-keyalg', 'RSA', '-keysize', '2048', '-validity', '10000',
      '-storepass', $pw, '-keypass', $pw, '-dname', 'CN=Mazok, O=Mazok')
  }
  Run $apksigner @('sign', '--ks', $ks, '--ks-key-alias', 'mazok', '--ks-pass', "file:$kp", '--out', (Join-Path $tmp 'Mazok.apk'), (Join-Path $tmp 'aligned.apk'))
  Copy-Item (Join-Path $tmp 'Mazok.apk') (Join-Path $out 'Mazok.apk') -Force
  Run $apksigner @('verify', (Join-Path $out 'Mazok.apk'))
  Write-Output ("Готово: {0} ({1:N1} МБ)" -f (Join-Path $out 'Mazok.apk'), ((Get-Item (Join-Path $out 'Mazok.apk')).Length / 1MB))
} finally {
  try { [IO.Directory]::Delete($tmp, $true) } catch { }
}
