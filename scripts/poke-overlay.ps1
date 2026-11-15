# Testet den Klick-Durchlass des Overlays mit kurzer, automatischer Mausbewegung.
# Bewegt den Zeiger ca. 3 s lang, klickt nur auf die Milo-Insel und stellt Zeiger und aktives Fenster wieder her.
param([string]$Shot = "")

Add-Type -ReferencedAssemblies System.Drawing -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;

public static class Poke {
    public delegate bool EnumProc(IntPtr hwnd, IntPtr lParam);
    [DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr lParam);
    [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hwnd, out uint pid);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hwnd, out RECT r);
    [DllImport("user32.dll")] public static extern int GetWindowLong(IntPtr hwnd, int index);
    [DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetWindowText(IntPtr hwnd, StringBuilder sb, int max);
    [DllImport("user32.dll")] public static extern bool GetCursorPos(out POINT p);
    [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
    [DllImport("user32.dll")] public static extern void mouse_event(uint flags, uint dx, uint dy, uint data, UIntPtr extra);
    [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hwnd);
    [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
    [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L, T, R, B; }
    [StructLayout(LayoutKind.Sequential)] public struct POINT { public int X, Y; }

    public static IntPtr Find(uint pid, string title) {
        IntPtr found = IntPtr.Zero;
        EnumWindows((h, l) => {
            uint p; GetWindowThreadProcessId(h, out p);
            var sb = new StringBuilder(256); GetWindowText(h, sb, 256);
            if (p == pid && sb.ToString() == title) { found = h; return false; }
            return true;
        }, IntPtr.Zero);
        return found;
    }
    public static bool ClickThrough(IntPtr h) { return (GetWindowLong(h, -20) & 0x20) != 0; }
    public static void Click() { mouse_event(0x2, 0, 0, 0, UIntPtr.Zero); mouse_event(0x4, 0, 0, 0, UIntPtr.Zero); }
}
'@

[Poke]::SetProcessDPIAware() | Out-Null
$proc = Get-Process milo -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $proc) { "milo.exe laeuft nicht"; exit 1 }
$hwnd = [Poke]::Find([uint32]$proc.Id, 'Milo')
if ($hwnd -eq [IntPtr]::Zero) { "Overlay-Fenster nicht gefunden"; exit 1 }

$r = New-Object Poke+RECT
[Poke]::GetWindowRect($hwnd, [ref]$r) | Out-Null
$scale = ($r.R - $r.L) / 600.0          # Fenster ist 600 logische Pixel breit
$cx = [int](($r.L + $r.R) / 2)
function At([double]$dx, [double]$dy) { [Poke]::SetCursorPos([int]($cx + $dx * $scale), [int]($r.T + $dy * $scale)) | Out-Null }

$home0 = New-Object Poke+POINT
[Poke]::GetCursorPos([ref]$home0) | Out-Null
$fg = [Poke]::GetForegroundWindow()
$results = @()
function Check([string]$name, [bool]$expectClickThrough) {
    Start-Sleep -Milliseconds 220
    $is = [Poke]::ClickThrough($hwnd)
    $script:results += "{0}  {1,-58} durchlaessig={2} (erwartet {3})" -f $(if ($is -eq $expectClickThrough) { 'OK    ' } else { 'FEHLER' }), $name, $is, $expectClickThrough
}

try {
    Check 'Maus ausserhalb des Fensters' $true
    At 0 26;      Check 'Maus auf der Pille' $false
    At -250 200;  Check 'Maus im durchsichtigen Bereich unter der Pille' $true
    At 0 26; Start-Sleep -Milliseconds 150; [Poke]::Click(); Start-Sleep -Milliseconds 800
    At -200 300;  Check 'aufgeklappt: Maus im Panel' $false
    if ($Shot) {
        $bmp = New-Object System.Drawing.Bitmap ($r.R - $r.L), ($r.B - $r.T)
        $g = [System.Drawing.Graphics]::FromImage($bmp); $g.CopyFromScreen($r.L, $r.T, 0, 0, $bmp.Size)
        $bmp.Save($Shot, [System.Drawing.Imaging.ImageFormat]::Png); $g.Dispose(); $bmp.Dispose()
    }
    At -292 300;  Check 'aufgeklappt: Maus knapp neben dem Panel' $true
    At 0 26; Start-Sleep -Milliseconds 150; [Poke]::Click(); Start-Sleep -Milliseconds 900
    At -200 300;  Check 'wieder zugeklappt: Maus wo das Panel war' $true
    At 0 26;      Check 'wieder zugeklappt: Maus auf der Pille' $false
} finally {
    [Poke]::SetCursorPos($home0.X, $home0.Y) | Out-Null
    if ($fg -ne [IntPtr]::Zero) { [Poke]::SetForegroundWindow($fg) | Out-Null }
}
$results
"Skalierung: $scale  Fenster: $($r.L),$($r.T) $($r.R - $r.L)x$($r.B - $r.T)"
