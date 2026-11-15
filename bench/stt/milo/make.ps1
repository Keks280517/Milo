Add-Type -AssemblyName System.Speech
$fmt = New-Object System.Speech.AudioFormat.SpeechAudioFormatInfo(16000, [System.Speech.AudioFormat.AudioBitsPerSample]::Sixteen, [System.Speech.AudioFormat.AudioChannel]::Mono)
$items = @(
  @("Microsoft Hedda Desktop", "de-DE", "Hey Milo, wie wird das Wetter?", "de-hedda-1"),
  @("Microsoft Hedda Desktop", "de-DE", "Hey Milo.", "de-hedda-2"),
  @("Microsoft Hedda Desktop", "de-DE", "Hey Milo, stell einen Timer auf zehn Minuten.", "de-hedda-3"),
  @("Microsoft David Desktop", "en-US", "Hey Milo, what's the weather like?", "en-david-1"),
  @("Microsoft David Desktop", "en-US", "Hey Milo.", "en-david-2"),
  @("Microsoft Zira Desktop", "en-US", "Hey Milo, set a timer for ten minutes.", "en-zira-1"),
  @("Microsoft Zira Desktop", "en-US", "Hey Milo.", "en-zira-2"),
  @("Microsoft David Desktop", "en-US", "Hey Meelo, what time is it?", "de-david-meelo"),
  @("Microsoft Zira Desktop", "en-US", "Hey Meelo.", "de-zira-meelo"),
  @("Microsoft Hedda Desktop", "de-DE", "Hey Milo, wie spät ist es?", "neg-hedda-milo"),
  @("Microsoft Hedda Desktop", "de-DE", "Ich habe heute Milch gekauft und ein Kilo Mehl.", "neg-hedda-milch"),
  @("Microsoft David Desktop", "en-US", "My low battery warning came up again.", "neg-david-mylow"),
  @("Microsoft Zira Desktop", "en-US", "Hey, meet me at the mall later.", "neg-zira-meet"),
  @("Microsoft Hedda Desktop", "de-DE", "Hey Mila, kommst du heute Abend mit?", "neg-hedda-mila")
)
foreach ($i in $items) {
  $s = New-Object System.Speech.Synthesis.SpeechSynthesizer
  $s.SelectVoice($i[0])
  $s.Rate = 0
  $s.SetOutputToWaveFile("$PWD\$($i[3]).wav", $fmt)
  $s.Speak($i[2])
  $s.Dispose()
}
"ok"
