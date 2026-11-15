# Prueft das Milo-Overlay von aussen: Fensterlage, Stil-Flags und ein Bildschirmfoto der Region.
# Nur lesend. Aufruf: powershell -File scripts\inspect-overlay.ps1 [-Shot pfad.png]
param([string]$Shot = "")

Add-Type -ReferencedAssemblies System.Drawing -TypeDefinition @'
using System;
using System.Collections.Generic;
using System.Runtime.InteropServices;
using System.Text;

public static class MiloWin {
    public delegate bool EnumProc(IntPtr hwnd, IntPtr lParam);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr lParam);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd, out RECT r);
    [DllImport("user32.dll")] public static extern int GetWindowLong(IntPtr hwnd, int index);
    [DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr hwnd);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowText(IntPtr hwnd, StringBuilder sb, int max);
    [DllImport("user32.dll")] public static extern bool GetCursorPos(out POINT p);
    [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
    [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }

    public static List<IntPtr> WindowsOf(uint pid) {
        var list = new List<IntPtr>();
        EnumWindows((h, l) => { uint p; GetWindowThreadProcessId(h, out p); if (p == pid && IsWindowVisible(h)) list.Add(h); return true; }, IntPtr.Zero);
        return list;
    }
    public static string Title(IntPtr h) { var sb = new StringBuilder(256); GetWindowText(h, sb, 256); return sb.ToString(); }
}
'@

[MiloWin]::SetProcessDPIAware() | Out-Null
$proc = Get-Process milo -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $proc) { "milo.exe laeuft nicht"; exit 1 }

$cursor = New-Object MiloWin+POINT
[MiloWin]::GetCursorPos([ref]$cursor) | Out-Null
"Maus: $($cursor.X),$($cursor.Y)   milo.exe RAM: $([math]::Round($proc.WorkingSet64/1MB)) MB"

$target = $null
foreach ($h in [MiloWin]::WindowsOf([uint32]$proc.Id)) {
    $r = New-Object MiloWin+RECT
    [MiloWin]::GetWindowRect($h, [ref]$r) | Out-Null
    $ex = [MiloWin]::GetWindowLong($h, -20)
    $flags = @()
    if ($ex -band 0x8)       { $flags += 'TOPMOST' }
    if ($ex -band 0x20)      { $flags += 'TRANSPARENT(klick-durchlaessig)' }
    if ($ex -band 0x80)      { $flags += 'TOOLWINDOW' }
    if ($ex -band 0x80000)   { $flags += 'LAYERED' }
    if ($ex -band 0x8000000) { $flags += 'NOACTIVATE' }
    "Fenster '$([MiloWin]::Title($h))': x=$($r.L) y=$($r.T) b=$($r.R - $r.L) h=$($r.B - $r.T)  [$($flags -join ', ')]"
    if ([MiloWin]::Title($h) -eq 'Milo') { $target = $r }
}

if ($Shot -and $target) {
    $w = $target.R - $target.L; $hgt = $target.B - $target.T
    $bmp = New-Object System.Drawing.Bitmap $w, $hgt
    $g = [System.Drawing.Graphics]::FromImage($bmp)
    $g.CopyFromScreen($target.L, $target.T, 0, 0, $bmp.Size)
    $bmp.Save($Shot, [System.Drawing.Imaging.ImageFormat]::Png)
    $g.Dispose(); $bmp.Dispose()
    "Bildschirmfoto: $Shot"
}

Add-Type -AssemblyName System.Windows.Forms
foreach ($s in [System.Windows.Forms.Screen]::AllScreens) {
    "Monitor $($s.DeviceName): $($s.Bounds.X),$($s.Bounds.Y) $($s.Bounds.Width)x$($s.Bounds.Height) primaer=$($s.Primary)"
}
