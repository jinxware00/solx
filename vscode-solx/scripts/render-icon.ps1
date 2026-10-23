$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$source = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../../assets/logo.png'))
$output = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../images/icon.png'))
$artwork = [Drawing.Image]::FromFile($source)
$bitmap = New-Object Drawing.Bitmap 512, 512
$graphics = [Drawing.Graphics]::FromImage($bitmap)
try {
    $graphics.Clear([Drawing.Color]::Transparent)
    $graphics.InterpolationMode = [Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
    $graphics.PixelOffsetMode = [Drawing.Drawing2D.PixelOffsetMode]::HighQuality
    # Fit the entire official PNG inside 10% clear space on each side.
    $scale = 410.0 / [Math]::Max($artwork.Width, $artwork.Height)
    $width = [int][Math]::Round($artwork.Width * $scale)
    $height = [int][Math]::Round($artwork.Height * $scale)
    $graphics.DrawImage($artwork, [int]((512 - $width) / 2), [int]((512 - $height) / 2), $width, $height)
    $bitmap.Save($output, [Drawing.Imaging.ImageFormat]::Png)
} finally {
    $graphics.Dispose()
    $bitmap.Dispose()
    $artwork.Dispose()
}
Write-Output 'Created images/icon.png (512 x 512) from assets/logo.png; original preserved.'
