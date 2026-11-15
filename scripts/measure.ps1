# Misst den Verbrauch einer laufenden Milo-Instanz samt Fensterprozessen.
#   scripts\measure.ps1 -Seconds 60 -Label "Leerlauf, Insel zu"
param(
    [int]$Seconds = 60,
    [string]$Label = "Messung"
)

$milo = Get-Process milo -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $milo) { "Milo läuft nicht."; return }

# Milo und alle Prozesse, die es gestartet hat (WebView2).
$all = Get-CimInstance Win32_Process
$ids = @($milo.Id)
$grew = $true
while ($grew) {
    $grew = $false
    foreach ($x in $all) {
        if ($ids -contains $x.ParentProcessId -and $ids -notcontains $x.ProcessId) { $ids += $x.ProcessId; $grew = $true }
    }
}
function Mine($samples) { $samples | Where-Object { $n = $_.InstanceName; ($ids | Where-Object { $n -like "pid_$($_)_*" }).Count -gt 0 } }
function CpuMs { $sum = 0; Get-Process -Id $ids -ErrorAction SilentlyContinue | ForEach-Object { $sum += $_.TotalProcessorTime.TotalMilliseconds }; $sum }

$c1 = CpuMs
$gpuLoad = @()
for ($i = 0; $i -lt $Seconds; $i += 5) {
    Start-Sleep -Seconds 5
    $gpuLoad += ((Mine (Get-Counter '\GPU Engine(*engtype_3D)\Utilization Percentage' -ErrorAction SilentlyContinue).CounterSamples) | Measure-Object CookedValue -Sum).Sum
}
$c2 = CpuMs
$procs = Get-Process -Id $ids -ErrorAction SilentlyContinue
$vram = ((Mine (Get-Counter '\GPU Process Memory(*)\Dedicated Usage' -ErrorAction SilentlyContinue).CounterSamples) | Measure-Object CookedValue -Sum).Sum / 1MB
$core = ($c2 - $c1) / ($Seconds * 10)
"{0}: {1} Prozesse | RAM {2:N0} MB (privat {3:N0} MB) | Grafikspeicher {4:N0} MB | CPU {5:N2} % eines Kerns ({6:N2} % gesamt) | GPU 3D Ø {7:N2} %" -f `
    $Label, $procs.Count, (($procs | Measure-Object WorkingSet64 -Sum).Sum / 1MB), (($procs | Measure-Object PrivateMemorySize64 -Sum).Sum / 1MB), `
    $vram, $core, ($core / [Environment]::ProcessorCount), (($gpuLoad | Measure-Object -Average).Average)
