//! Nuetzliche, harmlose PC-Befehle: Energie sparen (auch zeitversetzt), Bildschirm aus, Ordner oeffnen,
//! Lautstaerke auf einen genauen Wert, Zwischenablage, Systeminfos. Nichts davon loescht etwas.

use std::process::Stdio;
use std::time::Duration;

use anyhow::{bail, Result};
use serde_json::Value;
use tokio::io::AsyncWriteExt;

/// Versetzt den PC in den Energiesparmodus (wie das Mondsymbol im Startmenue).
pub fn sleep_now() -> Result<()> {
    #[cfg(windows)]
    unsafe {
        // false, false, false: Energie sparen (nicht Ruhezustand), Weckereignisse erlaubt.
        if !windows::Win32::System::Power::SetSuspendState(false, false, false) {
            bail!(tr!("Windows hat das Energiesparen abgelehnt.", "Windows refused to go to sleep."));
        }
    }
    Ok(())
}

/// Schaltet die Bildschirme aus; eine Mausbewegung schaltet sie wieder ein.
pub fn screen_off() -> Result<()> {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::Foundation::{LPARAM, WPARAM};
        use windows::Win32::UI::WindowsAndMessaging::{PostMessageW, HWND_BROADCAST, SC_MONITORPOWER, WM_SYSCOMMAND};
        PostMessageW(Some(HWND_BROADCAST), WM_SYSCOMMAND, WPARAM(SC_MONITORPOWER as usize), LPARAM(2))?;
    }
    Ok(())
}

/// Pfad eines bekannten Ordners ("downloads", "dokumente", ...).
pub fn known_folder(name: &str) -> Option<std::path::PathBuf> {
    let lower = name.trim().to_lowercase();
    #[cfg(windows)]
    {
        use windows::Win32::System::Com::CoTaskMemFree;
        use windows::Win32::UI::Shell::{
            SHGetKnownFolderPath, FOLDERID_Desktop, FOLDERID_Documents, FOLDERID_Downloads, FOLDERID_Music, FOLDERID_Pictures, FOLDERID_Videos,
            KF_FLAG_DEFAULT,
        };
        let id = match lower.as_str() {
            n if n.contains("download") => FOLDERID_Downloads,
            n if n.contains("dokument") || n.contains("document") => FOLDERID_Documents,
            n if n.contains("bild") || n.contains("foto") || n.contains("picture") => FOLDERID_Pictures,
            n if n.contains("musik") || n.contains("music") => FOLDERID_Music,
            n if n.contains("video") => FOLDERID_Videos,
            n if n.contains("desktop") || n.contains("schreibtisch") => FOLDERID_Desktop,
            _ => return None,
        };
        unsafe {
            let path = SHGetKnownFolderPath(&id, KF_FLAG_DEFAULT, None).ok()?;
            let text = path.to_string().ok();
            CoTaskMemFree(Some(path.0 as *const _));
            return text.map(std::path::PathBuf::from);
        }
    }
    #[allow(unreachable_code)]
    None
}

/// Setzt die Lautstaerke des Standard-Ausgabegeraets (0 bis 100). Liefert den alten Wert.
pub fn set_volume(percent: u32) -> Result<u32> {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
        use windows::Win32::Media::Audio::{eConsole, eRender, IMMDeviceEnumerator, MMDeviceEnumerator};
        use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED};
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let devices: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let speaker = devices.GetDefaultAudioEndpoint(eRender, eConsole)?;
        let volume: IAudioEndpointVolume = speaker.Activate(CLSCTX_ALL, None)?;
        let before = (volume.GetMasterVolumeLevelScalar()? * 100.0).round() as u32;
        volume.SetMasterVolumeLevelScalar(percent.min(100) as f32 / 100.0, std::ptr::null())?;
        if percent > 0 {
            volume.SetMute(false, std::ptr::null())?;
        }
        return Ok(before);
    }
    #[allow(unreachable_code)]
    {
        bail!(tr!("Nur unter Windows verfügbar.", "Only available on Windows."))
    }
}

/// Aendert die Lautstaerke um `delta` Prozentpunkte (Mausrad ueber der Pille). Liefert den neuen Wert.
pub fn change_volume(delta: i32) -> Result<u32> {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
        use windows::Win32::Media::Audio::{eConsole, eRender, IMMDeviceEnumerator, MMDeviceEnumerator};
        use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED};
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let devices: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let speaker = devices.GetDefaultAudioEndpoint(eRender, eConsole)?;
        let volume: IAudioEndpointVolume = speaker.Activate(CLSCTX_ALL, None)?;
        let now = (volume.GetMasterVolumeLevelScalar()? * 100.0).round() as i32;
        let next = (now + delta).clamp(0, 100) as u32;
        volume.SetMasterVolumeLevelScalar(next as f32 / 100.0, std::ptr::null())?;
        if next > 0 {
            volume.SetMute(false, std::ptr::null())?;
        }
        return Ok(next);
    }
    #[allow(unreachable_code)]
    {
        let _ = delta;
        bail!(tr!("Nur unter Windows verfügbar.", "Only available on Windows."))
    }
}

/// PowerShell-Skript ohne Fenster ausfuehren und die Ausgabe liefern.
pub async fn powershell_script(script: &str) -> Result<String> {
    powershell(script, None).await
}

async fn powershell(script: &str, input: Option<&str>) -> Result<String> {
    let mut command = tokio::process::Command::new("powershell");
    command
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() })
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x0800_0000);
    let mut child = command.spawn()?;
    if let (Some(text), Some(mut stdin)) = (input, child.stdin.take()) {
        stdin.write_all(text.as_bytes()).await?;
    }
    let output = tokio::time::timeout(Duration::from_secs(20), child.wait_with_output()).await??;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Legt Text in die Zwischenablage.
pub async fn clipboard_write(text: &str) -> Result<()> {
    powershell("$t = [Console]::In.ReadToEnd(); Set-Clipboard -Value $t", Some(text)).await?;
    Ok(())
}

/// Speicherplatz, Arbeitsspeicher, die groessten Programme, Akku und Laufzeit, als lesbarer Text.
pub async fn system_info() -> Result<String> {
    let script = r#"[Console]::OutputEncoding = [Text.Encoding]::UTF8
$os = Get-CimInstance Win32_OperatingSystem
$drives = Get-PSDrive -PSProvider FileSystem | Where-Object { $_.Used -ne $null -and ($_.Used + $_.Free) -gt 0 } | ForEach-Object { @{ name = $_.Name; freeGb = [math]::Round($_.Free / 1GB, 1); totalGb = [math]::Round(($_.Used + $_.Free) / 1GB, 1) } }
$top = Get-Process | Group-Object ProcessName | ForEach-Object { @{ name = $_.Name; mb = [math]::Round((($_.Group | Measure-Object WorkingSet64 -Sum).Sum) / 1MB) } } | Sort-Object { $_.mb } -Descending | Select-Object -First 6
$battery = Get-CimInstance Win32_Battery -ErrorAction SilentlyContinue | Select-Object -First 1
$cpu = (Get-CimInstance Win32_Processor | Measure-Object LoadPercentage -Average).Average
@{ ramTotalGb = [math]::Round($os.TotalVisibleMemorySize / 1MB, 1); ramFreeGb = [math]::Round($os.FreePhysicalMemory / 1MB, 1); cpuPercent = $cpu; uptimeHours = [math]::Round(((Get-Date) - $os.LastBootUpTime).TotalHours, 1); drives = @($drives); top = @($top); battery = if ($battery) { $battery.EstimatedChargeRemaining } else { $null } } | ConvertTo-Json -Depth 4 -Compress"#;
    let raw = powershell(script, None).await?;
    let info: Value = serde_json::from_str(&raw).map_err(|e| anyhow::anyhow!("{}", tr!(format!("Systeminfos nicht lesbar ({e})."), format!("System info not readable ({e})."))))?;
    Ok(describe_system(&info))
}

/// Macht aus den Rohdaten einen kurzen Bericht fuer das Modell.
pub fn describe_system(info: &Value) -> String {
    let mut lines = Vec::new();
    let total = info["ramTotalGb"].as_f64().unwrap_or(0.0);
    let free = info["ramFreeGb"].as_f64().unwrap_or(0.0);
    lines.push(tr!(format!("Arbeitsspeicher: {:.1} von {:.1} GB belegt, {:.1} GB frei.", total - free, total, free), format!("Memory: {:.1} of {:.1} GB used, {:.1} GB free.", total - free, total, free)));
    if let Some(cpu) = info["cpuPercent"].as_f64() {
        lines.push(tr!(format!("Prozessorauslastung gerade: {cpu:.0} %."), format!("Current CPU load: {cpu:.0} %.")));
    }
    for drive in info["drives"].as_array().into_iter().flatten() {
        lines.push(tr!(format!("Laufwerk {}: {:.1} GB frei von {:.1} GB.",
            drive["name"].as_str().unwrap_or("?"),
            drive["freeGb"].as_f64().unwrap_or(0.0),
            drive["totalGb"].as_f64().unwrap_or(0.0)
        ), format!("Drive {}: {:.1} GB free of {:.1} GB.",
            drive["name"].as_str().unwrap_or("?"),
            drive["freeGb"].as_f64().unwrap_or(0.0),
            drive["totalGb"].as_f64().unwrap_or(0.0)
        )));
    }
    let top: Vec<String> = info["top"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|p| format!("{} ({} MB)", p["name"].as_str().unwrap_or("?"), p["mb"].as_i64().unwrap_or(0)))
        .collect();
    if !top.is_empty() {
        lines.push(tr!(format!("Größte Programme im Arbeitsspeicher: {}.", top.join(", ")), format!("Largest apps in memory: {}.", top.join(", "))));
    }
    if let Some(battery) = info["battery"].as_i64() {
        lines.push(tr!(format!("Akku: {battery} %."), format!("Battery: {battery} %.")));
    }
    if let Some(hours) = info["uptimeHours"].as_f64() {
        lines.push(tr!(format!("Der PC läuft seit {hours:.1} Stunden."), format!("The PC has been running for {hours:.1} hours.")));
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn system_report_is_readable() {
        let info = json!({
            "ramTotalGb": 15.7, "ramFreeGb": 5.2, "cpuPercent": 7, "uptimeHours": 3.5,
            "drives": [{ "name": "C", "freeGb": 33.0, "totalGb": 465.0 }],
            "top": [{ "name": "brave", "mb": 2100 }, { "name": "claude", "mb": 1500 }],
            "battery": null
        });
        let text = describe_system(&info);
        assert!(text.contains("10.5 von 15.7 GB belegt"));
        assert!(text.contains("Laufwerk C: 33.0 GB frei"));
        assert!(text.contains("brave (2100 MB)"));
        assert!(!text.contains("Akku"));
    }

    #[test]
    fn unknown_folders_are_not_guessed() {
        assert!(known_folder("irgendwas").is_none());
    }
}
