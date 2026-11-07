Add-Type -AssemblyName System.Drawing
$W = 1200; $H = 630
$bmp = New-Object System.Drawing.Bitmap($W, $H)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::AntiAlias
$g.TextRenderingHint = [System.Drawing.Text.TextRenderingHint]::AntiAliasGridFit
$g.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic

function C([int]$r,[int]$gg,[int]$b,[int]$a=255) { [System.Drawing.Color]::FromArgb($a,$r,$gg,$b) }
function Brush($c) { New-Object System.Drawing.SolidBrush($c) }

# Base
$base = New-Object System.Drawing.Drawing2D.LinearGradientBrush((New-Object System.Drawing.Rectangle(0,0,$W,$H)), (C 9 11 17), (C 7 8 12), 25)
$g.FillRectangle($base, 0, 0, $W, $H)

# Glows
for ($i = 24; $i -ge 1; $i--) {
  $a = [int](3 + (24 - $i) * 0.55)
  $size = 120 + $i * 26
  $b1 = Brush (C 232 163 95 $a)
  $g.FillEllipse($b1, $W - 300 - $size/2, -220 - $size/2, $size, $size)
  $b1.Dispose()
}
for ($i = 20; $i -ge 1; $i--) {
  $a = [int](3 + (20 - $i) * 0.5)
  $size = 140 + $i * 24
  $b2 = Brush (C 84 179 156 $a)
  $g.FillEllipse($b2, -180 - $size/2, $H - 150 - $size/2, $size, $size)
  $b2.Dispose()
}

# Grid
$gridPen = New-Object System.Drawing.Pen((C 255 255 255 9), 1)
for ($x = 0; $x -le $W; $x += 60) { $g.DrawLine($gridPen, $x, 0, $x, $H) }
for ($y = 0; $y -le $H; $y += 60) { $g.DrawLine($gridPen, 0, $y, $W, $y) }
$gridPen.Dispose()

# Border
$borderPen = New-Object System.Drawing.Pen((C 255 255 255 26), 2)
$g.DrawRectangle($borderPen, 1, 1, $W-2, $H-2)
$borderPen.Dispose()

# Mark
$markPath = Join-Path $PSScriptRoot 'assets/img/asraab-mark.png'
if (Test-Path $markPath) {
  $mark = [System.Drawing.Image]::FromFile($markPath)
  $g.DrawImage($mark, 78, 64, 108, 108)
  $mark.Dispose()
}

# Eyebrow
$fontSmall = New-Object System.Drawing.Font('Segoe UI Semibold', 17, [System.Drawing.FontStyle]::Regular)
$amber = Brush (C 242 180 109)
$g.DrawString('ASRAAB  /  INDEPENDENT SOFTWARE STUDIO', $fontSmall, $amber, 214, 96)

# Title
$fontTitle = New-Object System.Drawing.Font('Segoe UI Semibold', 86, [System.Drawing.FontStyle]::Regular)
$titleRect1 = New-Object System.Drawing.RectangleF(76, 190, 1000, 120)
$titleRect2 = New-Object System.Drawing.RectangleF(76, 296, 1000, 130)
$white = Brush (C 246 242 234)
$g.DrawString('Apps that', $fontTitle, $white, $titleRect1)
$gradRect = New-Object System.Drawing.RectangleF(76, 296, 760, 130)
$grad = New-Object System.Drawing.Drawing2D.LinearGradientBrush($gradRect, (C 255 244 228), (C 201 116 31), 0)
$g.DrawString('fly together.', $fontTitle, $grad, $titleRect2)
$grad.Dispose()

# Subtitle
$fontBody = New-Object System.Drawing.Font('Segoe UI', 27, [System.Drawing.FontStyle]::Regular)
$muted = Brush (C 168 164 155)
$g.DrawString('Two focused Windows tools. One standard.', $fontBody, $muted, 80, 442)

# Chips
$fontChip = New-Object System.Drawing.Font('Segoe UI Semibold', 16, [System.Drawing.FontStyle]::Regular)
$chipY = 510
$chips = @(
  @{ text = 'MAJRA 1.2.3'; color = (C 242 180 109) },
  @{ text = 'DROIDCRATE 0.2.1'; color = (C 84 179 156) },
  @{ text = ('FREE  ' + [char]0xB7 + '  OPEN SOURCE  ' + [char]0xB7 + '  NO TELEMETRY'); color = (C 216 211 200) }
)
$x = 80
foreach ($chip in $chips) {
  $size = $g.MeasureString($chip.text, $fontChip)
  $w = [int]$size.Width + 34
  $path = New-Object System.Drawing.Drawing2D.GraphicsPath
  $r = 18
  $path.AddArc($x, $chipY, $r*2, $r*2, 180, 90)
  $path.AddArc($x + $w - $r*2, $chipY, $r*2, $r*2, 270, 90)
  $path.AddArc($x + $w - $r*2, $chipY + 38 - $r*2, $r*2, $r*2, 0, 90)
  $path.AddArc($x, $chipY + 38 - $r*2, $r*2, $r*2, 90, 90)
  $path.CloseFigure()
  $fill = Brush (C 255 255 255 12)
  $g.FillPath($fill, $path)
  $pen = New-Object System.Drawing.Pen((C 255 255 255 34), 1)
  $g.DrawPath($pen, $path)
  $fill.Dispose(); $pen.Dispose(); $path.Dispose()
  $cb = Brush $chip.color
  $g.DrawString($chip.text, $fontChip, $cb, $x + 17, $chipY + 8)
  $cb.Dispose()
  $x += $w + 14
}

# Corner marks
$corner = New-Object System.Drawing.Pen((C 242 180 109 120), 2)
$g.DrawLine($corner, $W-170, 64, $W-80, 64)
$g.DrawLine($corner, $W-80, 64, $W-80, 112)
$corner.Dispose()

# URL
$fontUrl = New-Object System.Drawing.Font('Consolas', 15, [System.Drawing.FontStyle]::Regular)
$dim = Brush (C 124 120 111)
$g.DrawString('bouadjadjatamer2-hub.github.io/Majra', $fontUrl, $dim, 80, 570)

$g.Dispose()
$out = Join-Path $PSScriptRoot 'assets/img/og.png'
$bmp.Save($out, [System.Drawing.Imaging.ImageFormat]::Png)
$bmp.Dispose()
Write-Output "saved $out"
