param([string]$Source = 'assets/branding/mio-note-v2.png')
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
if (-not [IO.Path]::IsPathRooted($Source)) { $Source = Join-Path $PSScriptRoot $Source }
$master = [Drawing.Image]::FromFile($Source)
try {
    if ($master.Width -ne $master.Height) { throw 'The icon master must be square.' }
    $frames = @()
    foreach ($size in @(16, 20, 24, 32, 40, 48, 64, 128, 256)) {
        $bitmap = [Drawing.Bitmap]::new($size, $size, [Drawing.Imaging.PixelFormat]::Format32bppArgb)
        $graphics = [Drawing.Graphics]::FromImage($bitmap)
        $attributes = [Drawing.Imaging.ImageAttributes]::new()
        $memory = [IO.MemoryStream]::new()
        try {
            $graphics.Clear([Drawing.Color]::Transparent)
            $graphics.CompositingMode = [Drawing.Drawing2D.CompositingMode]::SourceCopy
            $graphics.CompositingQuality = [Drawing.Drawing2D.CompositingQuality]::HighQuality
            $graphics.InterpolationMode = [Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
            $graphics.PixelOffsetMode = [Drawing.Drawing2D.PixelOffsetMode]::HighQuality
            $attributes.SetWrapMode([Drawing.Drawing2D.WrapMode]::TileFlipXY)
            $rect = [Drawing.Rectangle]::new(0, 0, $size, $size)
            $graphics.DrawImage($master, $rect, 0, 0, $master.Width, $master.Height, [Drawing.GraphicsUnit]::Pixel, $attributes)
            $bitmap.Save($memory, [Drawing.Imaging.ImageFormat]::Png)
            $frames += [PSCustomObject]@{ Size = $size; Bytes = $memory.ToArray() }
            if ($size -eq 256) {
                $bitmap.Save((Join-Path $PSScriptRoot 'assets/branding/mio-note-256.png'), [Drawing.Imaging.ImageFormat]::Png)
            }
        } finally { $memory.Dispose(); $attributes.Dispose(); $graphics.Dispose(); $bitmap.Dispose() }
    }

    $icoPath = Join-Path $PSScriptRoot 'assets/mio-note.ico'
    $writer = [IO.BinaryWriter]::new([IO.File]::Create($icoPath))
    try {
        $writer.Write([uint16]0)
        $writer.Write([uint16]1)
        $writer.Write([uint16]$frames.Count)
        $offset = 6 + 16 * $frames.Count
        foreach ($frame in $frames) {
            $dimension = if ($frame.Size -eq 256) { 0 } else { $frame.Size }
            $writer.Write([byte]$dimension)
            $writer.Write([byte]$dimension)
            $writer.Write([byte]0)
            $writer.Write([byte]0)
            $writer.Write([uint16]1)
            $writer.Write([uint16]32)
            $writer.Write([uint32]$frame.Bytes.Length)
            $writer.Write([uint32]$offset)
            $offset += $frame.Bytes.Length
        }
        foreach ($frame in $frames) { $writer.Write([byte[]]$frame.Bytes) }
    } finally { $writer.Dispose() }
    Write-Host "Windows icon: $icoPath ($($frames.Count) sizes, transparent PNG frames)"
} finally { $master.Dispose() }
