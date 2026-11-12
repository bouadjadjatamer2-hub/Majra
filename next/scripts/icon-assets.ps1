# Regenerates every Majra icon from src-tauri/icons/icon-source.svg.
#
#   powershell -ExecutionPolicy Bypass -File next/scripts/icon-assets.ps1
#
# Tauri's own generator produces icon.ico, icon.png and the Windows sizes. The browser
# extension and the site need 16/48 px copies, the grey "paused" variant, and a favicon,
# so those are derived here from the same 512 px master. Run this after editing the SVG.

$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing

$next = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$root = Split-Path -Parent $next
$icons = Join-Path $next 'src-tauri\icons'
$svg = Join-Path $icons 'icon-source.svg'
$master = Join-Path $icons 'icon.png'
$temp = Join-Path $env:TEMP ("majra-icons-" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $temp | Out-Null

Push-Location $next
try {
    & cmd /c "npx tauri icon `"$svg`" -o `"$icons`"" | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "tauri icon failed with exit code $LASTEXITCODE" }
} finally {
    Pop-Location
}

function Resize-Png {
    param([string]$Source, [string]$Destination, [int]$Size)
    $image = [System.Drawing.Image]::FromFile($Source)
    try {
        $bitmap = New-Object System.Drawing.Bitmap($Size, $Size, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
        $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
        try {
            $graphics.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
            $graphics.PixelOffsetMode = [System.Drawing.Drawing2D.PixelOffsetMode]::HighQuality
            $graphics.SmoothingMode = [System.Drawing.Drawing2D.SmoothingMode]::HighQuality
            $graphics.CompositingQuality = [System.Drawing.Drawing2D.CompositingQuality]::HighQuality
            $graphics.DrawImage($image, 0, 0, $Size, $Size)
        } finally {
            $graphics.Dispose()
        }
        $bitmap.Save($Destination, [System.Drawing.Imaging.ImageFormat]::Png)
        $bitmap.Dispose()
    } finally {
        $image.Dispose()
    }
}

# The extension's "paused" state is the same mark, drained of colour and dimmed.
function Grayscale-Png {
    param([string]$Source, [string]$Destination, [int]$Size)
    $image = [System.Drawing.Image]::FromFile($Source)
    try {
        $bitmap = New-Object System.Drawing.Bitmap($Size, $Size, [System.Drawing.Imaging.PixelFormat]::Format32bppArgb)
        $graphics = [System.Drawing.Graphics]::FromImage($bitmap)
        try {
            $graphics.InterpolationMode = [System.Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
            $matrix = New-Object System.Drawing.Imaging.ColorMatrix
            $matrix.Matrix00 = 0.216; $matrix.Matrix01 = 0.216; $matrix.Matrix02 = 0.216
            $matrix.Matrix10 = 0.425; $matrix.Matrix11 = 0.425; $matrix.Matrix12 = 0.425
            $matrix.Matrix20 = 0.079; $matrix.Matrix21 = 0.079; $matrix.Matrix22 = 0.079
            $matrix.Matrix33 = 1
            $matrix.Matrix44 = 1
            $attributes = New-Object System.Drawing.Imaging.ImageAttributes
            $attributes.SetColorMatrix($matrix)
            $rect = New-Object System.Drawing.Rectangle(0, 0, $Size, $Size)
            $graphics.DrawImage(
                $image,
                $rect,
                0,
                0,
                $image.Width,
                $image.Height,
                [System.Drawing.GraphicsUnit]::Pixel,
                $attributes
            )
            $attributes.Dispose()
        } finally {
            $graphics.Dispose()
        }
        $bitmap.Save($Destination, [System.Drawing.Imaging.ImageFormat]::Png)
        $bitmap.Dispose()
    } finally {
        $image.Dispose()
    }
}

# PNG-compressed ICO: one entry per size, which Vista and later read natively.
function Write-Ico {
    param([string]$Destination, [int[]]$Sizes)
    $blobs = @()
    foreach ($size in $Sizes) {
        $file = Join-Path $temp "ico-$size.png"
        Resize-Png -Source $master -Destination $file -Size $size
        $blobs += [System.IO.File]::ReadAllBytes($file)
    }
    $stream = New-Object System.IO.MemoryStream
    $writer = New-Object System.IO.BinaryWriter($stream)
    $writer.Write([uint16]0)
    $writer.Write([uint16]1)
    $writer.Write([uint16]$Sizes.Count)
    $offset = 6 + 16 * $Sizes.Count
    for ($index = 0; $index -lt $Sizes.Count; $index++) {
        $size = $Sizes[$index]
        $writer.Write([byte]$(if ($size -ge 256) { 0 } else { $size }))
        $writer.Write([byte]$(if ($size -ge 256) { 0 } else { $size }))
        $writer.Write([byte]0)
        $writer.Write([byte]0)
        $writer.Write([uint16]1)
        $writer.Write([uint16]32)
        $writer.Write([uint32]$blobs[$index].Length)
        $writer.Write([uint32]$offset)
        $offset += $blobs[$index].Length
    }
    foreach ($blob in $blobs) { $writer.Write($blob) }
    $writer.Flush()
    [System.IO.File]::WriteAllBytes($Destination, $stream.ToArray())
    $writer.Dispose()
    $stream.Dispose()
}

$extension = Join-Path $root 'extension\icons'
foreach ($size in 16, 32, 48, 128) {
    Resize-Png -Source $master -Destination (Join-Path $extension "icon-$size.png") -Size $size
    Grayscale-Png -Source $master -Destination (Join-Path $extension "icon-off-$size.png") -Size $size
}

Resize-Png -Source $master -Destination (Join-Path $root 'docs\assets\img\majra-icon.png') -Size 512
Resize-Png -Source $master -Destination (Join-Path $root 'docs\icon.png') -Size 512
Resize-Png -Source $master -Destination (Join-Path $root 'docs\assets\img\apple-touch-icon.png') -Size 180
Write-Ico -Destination (Join-Path $root 'docs\favicon.ico') -Sizes 16, 32, 48

Remove-Item -Recurse -Force $temp
Write-Output "icons updated from $svg"
