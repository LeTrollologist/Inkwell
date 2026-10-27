# Generates Inkwell's icon set (PNG + ICO) with no external dependencies.
# Usage: powershell -ExecutionPolicy Bypass -File scripts\make-icons.ps1
Add-Type -AssemblyName System.Drawing

$outDir = Join-Path $PSScriptRoot '..\assets\icons'
New-Item -ItemType Directory -Force -Path $outDir | Out-Null

function New-RoundedPath([System.Drawing.RectangleF]$rect, [float]$radius) {
  $path = New-Object System.Drawing.Drawing2D.GraphicsPath
  $d = $radius * 2
  $path.AddArc($rect.X, $rect.Y, $d, $d, 180, 90)
  $path.AddArc($rect.Right - $d, $rect.Y, $d, $d, 270, 90)
  $path.AddArc($rect.Right - $d, $rect.Bottom - $d, $d, $d, 0, 90)
  $path.AddArc($rect.X, $rect.Bottom - $d, $d, $d, 90, 90)
  $path.CloseFigure()
  return $path
}

function New-IconBitmap([int]$size) {
  $bmp = New-Object System.Drawing.Bitmap($size, $size, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
  $g = [System.Drawing.Graphics]::FromImage($bmp)
  $g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
  $g.TextRenderingHint = [System.Drawing.Text.TextRenderingHint]::AntiAliasGridFit
  $g.Clear([System.Drawing.Color]::Transparent)

  $inset = [float]($size * 0.055)
  $rect = New-Object System.Drawing.RectangleF($inset, $inset, ($size - $inset * 2), ($size - $inset * 2))
  $radius = [float]($size * 0.235)
  $shape = New-RoundedPath $rect $radius

  # Deep, slightly blue-black body.
  $body = New-Object System.Drawing.Drawing2D.LinearGradientBrush($rect, [System.Drawing.Color]::FromArgb(255, 26, 27, 38), [System.Drawing.Color]::FromArgb(255, 10, 11, 16), 55)
  $g.FillPath($body, $shape)

  # Signature gradient: violet -> cyan, used for the border ring and underline.
  $c1 = [System.Drawing.Color]::FromArgb(255, 139, 124, 246)
  $c2 = [System.Drawing.Color]::FromArgb(255, 34, 211, 238)
  $ring = New-Object System.Drawing.Drawing2D.LinearGradientBrush($rect, $c1, $c2, 45)
  $pen = New-Object System.Drawing.Pen($ring, [float][Math]::Max(1.0, $size * 0.032))
  $g.DrawPath($pen, $shape)

  # The mark: an M with a markdown "caret" underline.
  $fontSize = [float]($size * 0.46)
  $font = New-Object System.Drawing.Font('Segoe UI', $fontSize, [System.Drawing.FontStyle]::Bold, [System.Drawing.GraphicsUnit]::Pixel)
  $fmt = New-Object System.Drawing.StringFormat
  $fmt.Alignment = [System.Drawing.StringAlignment]::Center
  $fmt.LineAlignment = [System.Drawing.StringAlignment]::Center
  $glyphRect = New-Object System.Drawing.RectangleF(0, ($size * 0.02), $size, $size)
  $white = New-Object System.Drawing.SolidBrush([System.Drawing.Color]::FromArgb(255, 244, 245, 250))
  $g.DrawString('M', $font, $white, $glyphRect, $fmt)

  $barW = [float]($size * 0.34)
  $barH = [float][Math]::Max(2.0, $size * 0.058)
  $barX = [float](($size - $barW) / 2)
  $barY = [float]($size * 0.715)
  $barRect = New-Object System.Drawing.RectangleF($barX, $barY, $barW, $barH)
  $barBrush = New-Object System.Drawing.Drawing2D.LinearGradientBrush($barRect, $c1, $c2, 0)
  $g.FillRectangle($barBrush, $barRect)

  $g.Dispose()
  return $bmp
}

$sizes = @(16, 24, 32, 48, 64, 128, 256)
$pngs = @{}
foreach ($s in $sizes) {
  $bmp = New-IconBitmap $s
  $path = Join-Path $outDir ("{0}x{0}.png" -f $s)
  $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
  $pngs[$s] = [System.IO.File]::ReadAllBytes($path)
  $bmp.Dispose()
}

# Tauri's conventional names.
Copy-Item (Join-Path $outDir '32x32.png') (Join-Path $outDir 'icon.png') -Force
Copy-Item (Join-Path $outDir '256x256.png') (Join-Path $outDir '128x128@2x.png') -Force

# ICO container with PNG-compressed entries (Vista+).
$icoSizes = @(16, 24, 32, 48, 64, 128, 256)
$icoPath = Join-Path $outDir 'icon.ico'
$stream = New-Object System.IO.MemoryStream
$w = New-Object System.IO.BinaryWriter($stream)
$w.Write([UInt16]0)            # reserved
$w.Write([UInt16]1)            # type: icon
$w.Write([UInt16]$icoSizes.Count)
$offset = 6 + (16 * $icoSizes.Count)
foreach ($s in $icoSizes) {
  $bytes = $pngs[$s]
  $dim = if ($s -ge 256) { 0 } else { $s }
  $w.Write([Byte]$dim)         # width
  $w.Write([Byte]$dim)         # height
  $w.Write([Byte]0)            # palette
  $w.Write([Byte]0)            # reserved
  $w.Write([UInt16]1)          # color planes
  $w.Write([UInt16]32)         # bits per pixel
  $w.Write([UInt32]$bytes.Length)
  $w.Write([UInt32]$offset)
  $offset += $bytes.Length
}
foreach ($s in $icoSizes) { $w.Write($pngs[$s]) }
$w.Flush()
[System.IO.File]::WriteAllBytes($icoPath, $stream.ToArray())
$w.Dispose(); $stream.Dispose()

Get-ChildItem $outDir | Select-Object Name, Length | Format-Table -AutoSize
