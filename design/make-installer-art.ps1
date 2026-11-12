# Draws the installer artwork (NSIS + MSI) in the Ink Archive style.
# Run from the repo root with Windows PowerShell:  .\design\make-installer-art.ps1
# Colors are the tokens from docs/DESIGN.md. Impact stands in for Anton, which
# GDI+ cannot load from the bundled .woff2 files.
Add-Type -AssemblyName System.Drawing

$out = Join-Path $PSScriptRoot '..\src-tauri\installer'
New-Item -ItemType Directory -Force $out | Out-Null

$ink = [System.Drawing.ColorTranslator]::FromHtml('#0e0e0e')
$paper = [System.Drawing.ColorTranslator]::FromHtml('#f1eee6')
$red = [System.Drawing.ColorTranslator]::FromHtml('#e3242b')
$muted = [System.Drawing.ColorTranslator]::FromHtml('#a9a296')

function New-Canvas([int]$w, [int]$h, $background) {
  $bitmap = New-Object System.Drawing.Bitmap $w, $h, ([System.Drawing.Imaging.PixelFormat]::Format24bppRgb)
  $g = [System.Drawing.Graphics]::FromImage($bitmap)
  $g.SmoothingMode = 'AntiAlias'
  $g.TextRenderingHint = 'AntiAliasGridFit'
  $g.Clear($background)
  return $bitmap, $g
}

# Halftone dots, as on the Home screen.
function Add-Halftone($g, [int]$x0, [int]$y0, [int]$w, [int]$h, $color, [int]$alpha) {
  $brush = New-Object System.Drawing.SolidBrush ([System.Drawing.Color]::FromArgb($alpha, $color))
  for ($y = $y0 + 3; $y -lt $y0 + $h; $y += 7) {
    for ($x = $x0 + 3; $x -lt $x0 + $w; $x += 7) { $g.FillEllipse($brush, $x, $y, 2.2, 2.2) }
  }
  $brush.Dispose()
}

# The logo: accent square, hard offset shadow, katakana "ko" drawn as strokes.
function Add-Mark($g, [float]$x, [float]$y, [float]$size, $shadow) {
  $offset = $size * 0.09
  $g.FillRectangle((New-Object System.Drawing.SolidBrush $shadow), $x + $offset, $y + $offset, $size, $size)
  $g.FillRectangle((New-Object System.Drawing.SolidBrush $red), $x, $y, $size, $size)
  $pen = New-Object System.Drawing.Pen ([System.Drawing.Color]::White), ($size * 0.136)
  $pen.LineJoin = 'Miter'
  $l = $x + $size * 0.24; $r = $x + $size * 0.694; $t = $y + $size * 0.286; $b = $y + $size * 0.714
  $points = [System.Drawing.PointF[]]@(
    (New-Object System.Drawing.PointF $l, $t), (New-Object System.Drawing.PointF $r, $t),
    (New-Object System.Drawing.PointF $r, $b), (New-Object System.Drawing.PointF $l, $b))
  $g.DrawLines($pen, $points)
  $pen.Dispose()
}

function Add-Text($g, [string]$text, [string]$font, [float]$px, $color, [float]$x, [float]$y, $style = 'Regular') {
  $f = New-Object System.Drawing.Font $font, $px, ([System.Drawing.FontStyle]$style), ([System.Drawing.GraphicsUnit]::Pixel)
  $g.DrawString($text, $f, (New-Object System.Drawing.SolidBrush $color), $x, $y)
  $f.Dispose()
}

function Save-Bmp($bitmap, $g, [string]$name) {
  $g.Dispose()
  $bitmap.Save((Join-Path $out $name), [System.Drawing.Imaging.ImageFormat]::Bmp)
  $bitmap.Dispose()
}

# Tall ink panel used on the welcome and finish pages (NSIS 164x314, and the left of the MSI dialog).
function Add-Sidebar($g, [int]$h) {
  $g.FillRectangle((New-Object System.Drawing.SolidBrush $ink), 0, 0, 164, $h)
  Add-Halftone $g 0 0 164 $h $paper 26
  Add-Mark $g 22 30 56 $paper
  Add-Text $g 'KOMA' 'Impact' 50 $paper 17 104
  $g.FillRectangle((New-Object System.Drawing.SolidBrush $red), 22, 168, 120, 4)
  Add-Text $g 'OFFLINE' 'Consolas' 11 $muted 21 182
  Add-Text $g 'MANGA + MANHWA' 'Consolas' 11 $muted 21 197
  Add-Text $g 'READER' 'Consolas' 11 $muted 21 212
  Add-Text $g 'INK ARCHIVE' 'Consolas' 10 $red 21 ($h - 30) 'Bold'
}

# NSIS sidebar, 164 x 314.
$bitmap, $g = New-Canvas 164 314 $ink
Add-Sidebar $g 314
Save-Bmp $bitmap $g 'nsis-sidebar.bmp'

# NSIS header, 150 x 57 (shown at the right of the white header strip).
$bitmap, $g = New-Canvas 150 57 ([System.Drawing.Color]::White)
Add-Mark $g 18 12 30 $ink
Add-Text $g 'KOMA' 'Impact' 31 $ink 58 9
Save-Bmp $bitmap $g 'nsis-header.bmp'

# MSI dialog background, 493 x 312: ink panel on the left, text area stays light.
$bitmap, $g = New-Canvas 493 312 ([System.Drawing.Color]::White)
Add-Sidebar $g 312
Save-Bmp $bitmap $g 'wix-dialog.bmp'

# MSI top banner, 493 x 58: text is drawn over the left, so the logo sits right.
$bitmap, $g = New-Canvas 493 58 ([System.Drawing.Color]::White)
Add-Mark $g 372 13 30 $ink
Add-Text $g 'KOMA' 'Impact' 31 $ink 412 10
$g.FillRectangle((New-Object System.Drawing.SolidBrush $red), 0, 55, 493, 3)
Save-Bmp $bitmap $g 'wix-banner.bmp'

Write-Host "Installer artwork written to $out"
