$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Drawing
$source = Join-Path $PSScriptRoot 'assets/branding/mio-note-v2.png'
$destination = Join-Path $PSScriptRoot 'distribution/store/Assets'
New-Item -ItemType Directory -Path $destination -Force | Out-Null
$master = [Drawing.Image]::FromFile($source)
try {
    foreach ($item in @(@('StoreLogo.png',50),@('Square44x44Logo.png',44),@('Square150x150Logo.png',150))) {
        $size = [int]$item[1]
        $bitmap = [Drawing.Bitmap]::new($size,$size,[Drawing.Imaging.PixelFormat]::Format32bppArgb)
        $graphics = [Drawing.Graphics]::FromImage($bitmap)
        try {
            $graphics.Clear([Drawing.Color]::Transparent)
            $graphics.InterpolationMode = [Drawing.Drawing2D.InterpolationMode]::HighQualityBicubic
            $graphics.DrawImage($master,[Drawing.Rectangle]::new(0,0,$size,$size))
            $bitmap.Save((Join-Path $destination $item[0]),[Drawing.Imaging.ImageFormat]::Png)
        } finally { $graphics.Dispose(); $bitmap.Dispose() }
    }
} finally { $master.Dispose() }
