# Сборка Mazok.exe (Windows, WebView2) без Visual Studio: компилятор csc из .NET Framework 4.8.
# Нужны файлы WebView2 SDK в папке sdk\: Microsoft.Web.WebView2.Core.dll, Microsoft.Web.WebView2.WinForms.dll (lib\net462)
# и WebView2Loader.dll (runtimes\win-x64\native) из пакета NuGet Microsoft.Web.WebView2.
param([string]$Sdk = (Join-Path $PSScriptRoot 'sdk'))
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$out = Join-Path $root 'dist'
New-Item -ItemType Directory -Force $out | Out-Null
$tmp = Join-Path ([IO.Path]::GetTempPath()) ('mazok-exe-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory $tmp | Out-Null
try {
  # 1. веб-приложение -> app.zip
  $www = Join-Path $tmp 'www'
  New-Item -ItemType Directory $www | Out-Null
  foreach ($f in 'index.html', 'manifest.webmanifest', 'sw.js', 'css', 'js', 'icons') { Copy-Item (Join-Path $root $f) $www -Recurse }
  Add-Type -AssemblyName System.IO.Compression.FileSystem
  [IO.Compression.ZipFile]::CreateFromDirectory($www, (Join-Path $tmp 'app.zip'))

  # 2. значок .ico из icons\icon-512.png
  Add-Type -AssemblyName System.Drawing
  $src = [Drawing.Image]::FromFile((Join-Path $root 'icons\icon-512.png'))
  $sizes = 256, 64, 48, 32, 24, 16
  $pngs = @()
  foreach ($s in $sizes) {
    $bmp = New-Object Drawing.Bitmap $s, $s
    $g = [Drawing.Graphics]::FromImage($bmp)
    $g.InterpolationMode = 'HighQualityBicubic'; $g.SmoothingMode = 'HighQuality'; $g.PixelOffsetMode = 'HighQuality'
    $g.DrawImage($src, 0, 0, $s, $s)
    $ms = New-Object IO.MemoryStream
    $bmp.Save($ms, [Drawing.Imaging.ImageFormat]::Png)
    $pngs += , $ms.ToArray()
    $g.Dispose(); $bmp.Dispose()
  }
  $src.Dispose()
  $ico = Join-Path $tmp 'mazok.ico'
  $fs = [IO.File]::Create($ico); $bw = New-Object IO.BinaryWriter $fs
  $bw.Write([uint16]0); $bw.Write([uint16]1); $bw.Write([uint16]$sizes.Count)
  $offset = 6 + 16 * $sizes.Count
  for ($i = 0; $i -lt $sizes.Count; $i++) {
    $d = $sizes[$i]; if ($d -ge 256) { $d = 0 }
    $bw.Write([byte]$d); $bw.Write([byte]$d); $bw.Write([byte]0); $bw.Write([byte]0)
    $bw.Write([uint16]1); $bw.Write([uint16]32); $bw.Write([uint32]$pngs[$i].Length); $bw.Write([uint32]$offset)
    $offset += $pngs[$i].Length
  }
  foreach ($p in $pngs) { $bw.Write($p) }
  $bw.Close()

  # 3. компиляция
  $csc = Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319\csc.exe'
  $core = Join-Path $Sdk 'Microsoft.Web.WebView2.Core.dll'
  $wf = Join-Path $Sdk 'Microsoft.Web.WebView2.WinForms.dll'
  $loader = Join-Path $Sdk 'WebView2Loader.dll'
  foreach ($p in $core, $wf, $loader) { if (-not (Test-Path $p)) { throw "Не найден $p" } }
  & $csc /nologo /target:winexe /platform:x64 /optimize+ /codepage:65001 "/out:$out\Mazok.exe" "/win32icon:$ico" `
    /r:System.Windows.Forms.dll /r:System.Drawing.dll /r:System.IO.Compression.dll /r:Microsoft.CSharp.dll /r:System.Core.dll `
    "/r:$core" "/r:$wf" `
    "/resource:$tmp\app.zip,app.zip" "/resource:$core,Microsoft.Web.WebView2.Core.dll" "/resource:$wf,Microsoft.Web.WebView2.WinForms.dll" "/resource:$loader,WebView2Loader.dll" `
    (Join-Path $PSScriptRoot 'Mazok.cs')
  if ($LASTEXITCODE -ne 0) { throw "csc завершился с кодом $LASTEXITCODE" }
  Write-Output ("Готово: {0} ({1:N1} МБ)" -f "$out\Mazok.exe", ((Get-Item "$out\Mazok.exe").Length / 1MB))
} finally {
  Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}
