//! Sprachausgabe ueber die in Windows eingebaute Stimme (System.Speech). Lokal, ohne Zusatzdateien.
//! Der Text geht ueber die Standardeingabe an PowerShell, nie ueber die Befehlszeile.

use std::io::Write;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

#[derive(Default)]
pub struct Speaker {
    current: Mutex<Option<Child>>,
}

const SCRIPT: &str = "Add-Type -AssemblyName System.Speech; \
$s = New-Object System.Speech.Synthesis.SpeechSynthesizer; \
$de = $s.GetInstalledVoices() | Where-Object { $_.Enabled -and $_.VoiceInfo.Culture.Name -like 'de*' } | Select-Object -First 1; \
if ($de) { $s.SelectVoice($de.VoiceInfo.Name) }; \
$s.Rate = 1; $s.Speak([Console]::In.ReadToEnd())";

/// Macht aus einer Chat-Antwort vorlesbaren Text: ohne Codebloecke, Formatierungszeichen und Adressen.
pub fn speakable(text: &str) -> String {
    let mut out = String::new();
    for (index, chunk) in text.split("```").enumerate() {
        if index % 2 == 1 {
            out.push_str(tr!("Den Code siehst du in der Antwort. ", "You'll see the code in the answer. "));
            continue;
        }
        for word in chunk.split_whitespace() {
            if word.starts_with("http://") || word.starts_with("https://") {
                continue;
            }
            let clean: String = word.chars().filter(|c| !matches!(c, '*' | '`' | '#' | '_' | '$' | '\\' | '|')).collect();
            if !clean.is_empty() {
                out.push_str(&clean);
                out.push(' ');
            }
        }
    }
    out.trim().chars().take(1200).collect()
}

impl Speaker {
    pub fn stop(&self) {
        if let Some(mut child) = self.current.lock().unwrap().take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    pub fn speak(&self, text: &str) -> Result<(), String> {
        self.stop();
        let text = speakable(text);
        if text.is_empty() {
            return Ok(());
        }
        let mut command = Command::new("powershell");
        let script = SCRIPT.replace("'de*'", tr!("'de*'", "'en*'"));
        command.args(["-NoProfile", "-NonInteractive", "-Command", &script]).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let mut child = command.spawn().map_err(|e| tr!(format!("Die Sprachausgabe startet nicht ({e})."), format!("Speech output won't start ({e}).")))?;
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(text.as_bytes());
        }
        *self.current.lock().unwrap() = Some(child);
        Ok(())
    }
}

#[tauri::command]
pub fn speak(speaker: tauri::State<'_, Speaker>, text: String) -> Result<(), String> {
    speaker.speak(&text)
}

#[tauri::command]
pub fn stop_speaking(speaker: tauri::State<'_, Speaker>) {
    speaker.stop();
}

#[cfg(test)]
mod tests {
    use super::speakable;

    #[test]
    fn answers_become_plain_speech() {
        assert_eq!(speakable("**Fertig**, der `Timer` läuft."), "Fertig, der Timer läuft.");
        assert_eq!(speakable("Schau hier: https://example.com/x danke"), "Schau hier: danke");
        assert_eq!(speakable("So:\n```python\nprint(1)\n```\nDas war's."), "So: Den Code siehst du in der Antwort. Das war's.");
        assert_eq!(speakable("  "), "");
    }
}
