//! Klingelton fuer abgelaufene Timer. Der Ton wird berechnet (keine Audiodatei) und ueber die
//! Windows-Funktion PlaySound in Schleife gespielt. Das laeuft im Rust-Kern und klingelt deshalb
//! auch dann, wenn die Oberflaeche gerade nicht existiert (Spielemodus).

use std::sync::OnceLock;

const RATE: u32 = 22_050;
/// Laenge einer Runde in Sekunden; passt zum Takt der Klingel-Animation.
const LOOP_SECONDS: f32 = 1.2;
/// Aufsteigender Dreiklang (C6, E6, G6) als (Einsatz in Sekunden, Frequenz).
const NOTES: [(f32, f32); 3] = [(0.0, 1046.5), (0.14, 1318.5), (0.28, 1568.0)];
/// Bewusst leise gehalten, die Systemlautstaerke kommt noch obendrauf.
const VOLUME: f32 = 0.22;

/// Eine Runde des Klingeltons als WAV (16 Bit, mono): drei weiche Glockentoene, dann Stille.
pub fn chime_wav() -> Vec<u8> {
    let total = (RATE as f32 * LOOP_SECONDS) as usize;
    let mut samples = vec![0f32; total];
    for (start, freq) in NOTES {
        let first = (start * RATE as f32) as usize;
        for (i, sample) in samples.iter_mut().enumerate().skip(first) {
            let t = (i - first) as f32 / RATE as f32;
            // Kurzer weicher Anschlag, dann klingt der Ton wie bei einer Glocke aus.
            let envelope = (t / 0.006).min(1.0) * (-t / 0.17).exp();
            let phase = std::f32::consts::TAU * freq * t;
            *sample += envelope * (phase.sin() + 0.26 * (2.0 * phase).sin() + 0.08 * (3.0 * phase).sin());
        }
    }
    // Am Ende ausblenden, damit die Schleife nicht knackt.
    let fade = (RATE as f32 * 0.03) as usize;
    for i in 0..fade {
        samples[total - 1 - i] *= i as f32 / fade as f32;
    }

    let data_len = (total * 2) as u32;
    let mut wav = Vec::with_capacity(44 + total * 2);
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_len).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes()); // PCM
    wav.extend_from_slice(&1u16.to_le_bytes()); // mono
    wav.extend_from_slice(&RATE.to_le_bytes());
    wav.extend_from_slice(&(RATE * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());
    for sample in samples {
        let value = (sample * VOLUME).clamp(-1.0, 1.0) * i16::MAX as f32;
        wav.extend_from_slice(&(value as i16).to_le_bytes());
    }
    wav
}

/// Der Ton muss gueltig bleiben, solange Windows ihn abspielt, deshalb lebt er fuer immer.
fn chime() -> &'static [u8] {
    static CHIME: OnceLock<Vec<u8>> = OnceLock::new();
    CHIME.get_or_init(chime_wav)
}

#[cfg(windows)]
mod native {
    use windows::core::PCWSTR;
    use windows::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_FLAGS, SND_LOOP, SND_MEMORY, SND_NODEFAULT};

    pub fn play_loop(wav: &'static [u8]) {
        // SAFETY: `wav` ist ein vollstaendiges WAV im Speicher und bleibt fuer die Laufzeit gueltig.
        unsafe {
            let _ = PlaySoundW(PCWSTR(wav.as_ptr().cast()), None, SND_MEMORY | SND_ASYNC | SND_LOOP | SND_NODEFAULT);
        }
    }

    pub fn play_once(wav: &'static [u8]) {
        // SAFETY: wie oben, nur ohne Schleife.
        unsafe {
            let _ = PlaySoundW(PCWSTR(wav.as_ptr().cast()), None, SND_MEMORY | SND_ASYNC | SND_NODEFAULT);
        }
    }

    pub fn stop() {
        // SAFETY: Ein Null-Zeiger beendet laut Windows-Dokumentation die laufende Wiedergabe.
        unsafe {
            let _ = PlaySoundW(PCWSTR::null(), None, SND_FLAGS(0));
        }
    }
}

#[cfg(not(windows))]
mod native {
    pub fn play_loop(_wav: &'static [u8]) {}
    pub fn play_once(_wav: &'static [u8]) {}
    pub fn stop() {}
}

pub fn start_ringing() {
    native::play_loop(chime());
}

pub fn stop_ringing() {
    native::stop();
}

/// Einmal kurz klingeln (Termin beginnt), ohne Schleife.
pub fn chime_once() {
    native::play_once(chime());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chime_is_a_valid_quiet_wav() {
        let wav = chime_wav();
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..16], b"WAVEfmt ");
        let data_len = u32::from_le_bytes(wav[40..44].try_into().unwrap()) as usize;
        assert_eq!(wav.len(), 44 + data_len);
        assert_eq!(u32::from_le_bytes(wav[4..8].try_into().unwrap()) as usize, wav.len() - 8);

        let samples: Vec<i16> = wav[44..].chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect();
        let peak = samples.iter().map(|s| s.unsigned_abs()).max().unwrap();
        // Hoerbar, aber weit weg von der Uebersteuerung.
        assert!(peak > 2000 && peak < 16000, "Spitze {peak}");
        // Anfang und Ende sind still, sonst knackt die Schleife.
        assert!(samples[0].abs() < 50 && samples[samples.len() - 1].abs() < 50);
    }
}
