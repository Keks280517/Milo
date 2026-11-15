# Zieht mit kurzer automatischer Mausbewegung von einem Bildschirmpunkt zu einem anderen und laesst los
# (Test fuer das Herausziehen von Milo oder Deko). Stellt den Zeiger danach wieder her und macht ein Foto
# des unteren Bildschirmrands. Aufruf:
#   powershell -File scripts\drag-test.ps1 -FromX 1240 -FromY 40 -ToX 1300 -ToY 700 -Shot unten.png
param([int]$FromX, [int]$FromY, [int]$ToX, [int]$ToY, [string]$Shot = "", [int]$Wait = 2500, [int]$Hover = 700)

Add-Type -ReferencedAssemblies System.Drawing, System.Windows.Forms -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class Drag {
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern bool GetCursorPos(out POINT p);
    [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
    [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
    public struct POINT { public int X; public int Y; }
}
'@
[Drag]::SetProcessDPIAware() | Out-Null
$start = New-Object Drag+POINT
[Drag]::GetCursorPos([ref]$start) | Out-Null

[Drag]::SetCursorPos($FromX, $FromY) | Out-Null
# Das Overlay schaltet erst nach der naechsten Zeiger-Abfrage auf klickbar.
Start-Sleep -Milliseconds $Hover
[Drag]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero)   # links runter
Start-Sleep -Milliseconds 120
# Erst ein paar kleine Schritte innerhalb der Insel, damit aus dem Klick ein Ziehen wird.
for ($i = 1; $i -le 4; $i++) {
    [Drag]::SetCursorPos($FromX + $i * 3, $FromY + $i * 2) | Out-Null
    Start-Sleep -Milliseconds 40
}
$steps = 30
for ($i = 1; $i -le $steps; $i++) {
    $x = [int]($FromX + ($ToX - $FromX) * $i / $steps)
    $y = [int]($FromY + ($ToY - $FromY) * $i / $steps)
    [Drag]::SetCursorPos($x, $y) | Out-Null
    Start-Sleep -Milliseconds 25
}
Start-Sleep -Milliseconds 200
[Drag]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)   # links hoch
[Drag]::SetCursorPos($start.X, $start.Y) | Out-Null
Start-Sleep -Milliseconds $Wait

if ($Shot) {
    $screen = [System.Windows.Forms.Screen]::PrimaryScreen.Bounds
    $h = 260
    $bmp = New-Object System.Drawing.Bitmap $screen.Width, $h
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen(0, $screen.Height - $h, 0, 0, $bmp.Size)
    $bmp.Save($Shot, [System.Drawing.Imaging.ImageFormat]::Png)
    "Foto: $Shot"
}
