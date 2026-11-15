//! Skripte und Bildschirmfoto.
//!
//! Skripte: Das Modell darf ein PowerShell-Skript vorschlagen. Ausgefuehrt wird es erst, nachdem der
//! Nutzer den vollstaendigen Text gesehen und bestaetigt hat, mit normalen Benutzerrechten und
//! einer Zeitgrenze. Skripte, die sich selbst Admin-Rechte holen wollen, werden abgelehnt.
//!
//! Bildschirmfoto: wird nur auf ausdruecklichen Wunsch gemacht, bleibt im Arbeitsspeicher und geht
//! ausschliesslich an das lokale Modell.

use std::process::Stdio;
use std::time::Duration;

use anyhow::{bail, Result};
use tokio::io::AsyncWriteExt;

/// Laengere Skripte sind kein "kleiner Helfer" mehr und lassen sich nicht sinnvoll pruefen.
pub const MAX_SCRIPT_CHARS: usize = 4000;
const TIMEOUT: Duration = Duration::from_secs(60);
const MAX_OUTPUT_CHARS: usize = 3000;

/// Prueft ein Skript, bevor es dem Nutzer ueberhaupt vorgelegt wird.
pub fn check(code: &str) -> Result<()> {
    let lower = code.to_lowercase();
    if code.trim().is_empty() {
        bail!(tr!("Das Skript ist leer.", "The script is empty."));
    }
    if code.chars().count() > MAX_SCRIPT_CHARS {
        bail!(tr!("Das Skript ist zu lang, um es vor dem Ausführen sinnvoll zu prüfen.", "The script is too long to check properly before running it."));
    }
    if lower.contains("runas") || lower.contains("-verb") && lower.contains("admin") {
        bail!(tr!("Das Skript will Administratorrechte. So etwas führe ich nicht aus.", "The script wants administrator rights. I don't run things like that."));
    }
    if lower.contains("-encodedcommand") || lower.contains("frombase64string") {
        bail!(tr!("Das Skript versteckt seinen Inhalt. So etwas führe ich nicht aus.", "The script hides its content. I don't run things like that."));
    }
    if let Some(reason) = crate::guard::script_forbidden(code) {
        bail!("{reason}");
    }
    Ok(())
}

pub struct Output {
    pub ok: bool,
    pub text: String,
}

fn shorten(text: &str) -> String {
    let text = text.trim();
    if text.chars().count() <= MAX_OUTPUT_CHARS {
        return text.to_string();
    }
    tr!(format!("{} … (gekürzt)", text.chars().take(MAX_OUTPUT_CHARS).collect::<String>()), format!("{} … (shortened)", text.chars().take(MAX_OUTPUT_CHARS).collect::<String>()))
}

/// Fuehrt ein bereits bestaetigtes Skript aus. Der Text geht ueber die Standardeingabe.
pub async fn run(code: &str) -> Result<Output> {
    let mut command = tokio::process::Command::new("powershell");
    command
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    let mut child = command.spawn()?;
    if let Some(mut stdin) = child.stdin.take() {
        // UTF-8 ansagen, damit Umlaute in der Ausgabe stimmen.
        let script = format!("[Console]::OutputEncoding = [Text.Encoding]::UTF8\n{code}\n");
        stdin.write_all(script.as_bytes()).await?;
    }
    let output = match tokio::time::timeout(TIMEOUT, child.wait_with_output()).await {
        Ok(result) => result?,
        Err(_) => bail!(tr!("Das Skript lief länger als eine Minute und wurde beendet.", "The script ran longer than a minute and was stopped.")),
    };
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let text = if stderr.trim().is_empty() { shorten(&stdout) } else { shorten(&tr!(format!("{stdout}\nFehlerausgabe:\n{stderr}"), format!("{stdout}\nError output:\n{stderr}"))) };
    Ok(Output { ok: output.status.success() && stderr.trim().is_empty(), text })
}

/// Fotografiert den Hauptbildschirm und liefert ihn verkleinert als PNG.
#[cfg(windows)]
pub fn screenshot(max_width: u32) -> Result<Vec<u8>> {
    use windows::Win32::UI::WindowsAndMessaging::{GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN};
    let (width, height) = unsafe { (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN)) };
    capture(0, 0, width, height, max_width)
}

/// Fotografiert einen Bereich des (virtuellen) Bildschirms, z. B. einen ganzen zweiten Monitor, als PNG.
#[cfg(windows)]
pub fn capture(left: i32, top: i32, width: i32, height: i32, max_width: u32) -> Result<Vec<u8>> {
    use windows::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits, ReleaseDC, SelectObject, BITMAPINFO,
        BITMAPINFOHEADER, DIB_RGB_COLORS, SRCCOPY,
    };

    if width <= 0 || height <= 0 {
        bail!(tr!("Die Bildschirmgröße ließ sich nicht ermitteln.", "The screen size couldn't be determined."));
    }
    let mut pixels = vec![0u8; (width * height * 4) as usize];
    unsafe {
        let screen = GetDC(None);
        let memory = CreateCompatibleDC(Some(screen));
        let bitmap = CreateCompatibleBitmap(screen, width, height);
        let old = SelectObject(memory, bitmap.into());
        let copied = BitBlt(memory, 0, 0, width, height, Some(screen), left, top, SRCCOPY);
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                // Negativ = Zeilen von oben nach unten.
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                ..Default::default()
            },
            ..Default::default()
        };
        let lines = GetDIBits(memory, bitmap, 0, height as u32, Some(pixels.as_mut_ptr().cast()), &mut info, DIB_RGB_COLORS);
        SelectObject(memory, old);
        let _ = DeleteObject(bitmap.into());
        let _ = DeleteDC(memory);
        ReleaseDC(None, screen);
        if copied.is_err() || lines == 0 {
            bail!(tr!("Das Bildschirmfoto ist fehlgeschlagen.", "The screenshot failed."));
        }
    }
    let (w, h, rgb) = shrink(&pixels, width as u32, height as u32, max_width);
    let mut png_bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png_bytes, w, h);
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header()?;
        writer.write_image_data(&rgb)?;
    }
    Ok(png_bytes)
}

#[cfg(not(windows))]
pub fn screenshot(_max_width: u32) -> Result<Vec<u8>> {
    bail!(tr!("Nur unter Windows verfügbar.", "Only available on Windows."))
}

#[cfg(not(windows))]
pub fn capture(_left: i32, _top: i32, _width: i32, _height: i32, _max_width: u32) -> Result<Vec<u8>> {
    bail!(tr!("Nur unter Windows verfügbar.", "Only available on Windows."))
}

/// Verkleinert ein BGRA-Bild auf hoechstens `max_width` (Mittelwert je Zielpixel) und liefert RGB.
pub fn shrink(bgra: &[u8], width: u32, height: u32, max_width: u32) -> (u32, u32, Vec<u8>) {
    let factor = width.div_ceil(max_width.max(1)).max(1);
    let (w, h) = (width / factor, height / factor);
    let mut rgb = Vec::with_capacity((w * h * 3) as usize);
    for y in 0..h {
        for x in 0..w {
            let mut sum = [0u32; 3];
            for dy in 0..factor {
                for dx in 0..factor {
                    let i = (((y * factor + dy) * width + x * factor + dx) * 4) as usize;
                    sum[0] += bgra[i + 2] as u32;
                    sum[1] += bgra[i + 1] as u32;
                    sum[2] += bgra[i] as u32;
                }
            }
            let n = factor * factor;
            rgb.extend(sum.iter().map(|s| (s / n) as u8));
        }
    }
    (w, h, rgb)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dangerous_scripts_are_rejected_before_asking() {
        assert!(check("Get-Date").is_ok());
        assert!(check("   ").is_err());
        assert!(check("Start-Process powershell -Verb RunAs").is_err());
        assert!(check("powershell -EncodedCommand abc").is_err());
        assert!(check(&"x".repeat(MAX_SCRIPT_CHARS + 1)).is_err());
    }

    #[test]
    fn shrink_averages_and_swaps_channels() {
        // 2x2 Bild, BGRA: zwei blaue und zwei rote Pixel -> ein Pixel mit halbem Rot und halbem Blau.
        let bgra = [255, 0, 0, 255, 255, 0, 0, 255, 0, 0, 255, 255, 0, 0, 255, 255];
        let (w, h, rgb) = shrink(&bgra, 2, 2, 1);
        assert_eq!((w, h), (1, 1));
        assert_eq!(rgb, vec![127, 0, 127]);
        let (w, h, rgb) = shrink(&bgra, 2, 2, 2);
        assert_eq!((w, h), (2, 2));
        assert_eq!(&rgb[..3], &[0, 0, 255]);
    }

    #[test]
    fn long_output_is_shortened() {
        assert_eq!(shorten("  kurz "), "kurz");
        assert!(shorten(&"a".repeat(5000)).ends_with("(gekürzt)"));
    }
}
