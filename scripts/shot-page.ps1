# Nimmt ein Bild einer Entwicklungsseite auf (z. B. das Charakter-Labor), ohne ein sichtbares
# Fenster zu oeffnen. Nutzt einen installierten Chromium-Browser im Hintergrundmodus.
#
#   scripts\shot-page.ps1 -Url "http://localhost:1420/lab?strip=walk&n=10" -Out bild.png -Width 1500 -Height 700
param(
    [Parameter(Mandatory)] [string]$Url,
    [Parameter(Mandatory)] [string]$Out,
    [int]$Width = 1500,
    [int]$Height = 900,
    # So viele Millisekunden darf die Seite rechnen, bevor das Bild entsteht.
    [int]$Budget = 2500
)

$edge = @(
    "$env:ProgramFiles\Google\Chrome\Application\chrome.exe",
    "${env:ProgramFiles(x86)}\Microsoft\Edge\Application\msedge.exe",
    "$env:ProgramFiles\Microsoft\Edge\Application\msedge.exe",
    "$env:ProgramFiles\BraveSoftware\Brave-Browser\Application\brave.exe"
) | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $edge) { throw "Kein Chrome, Edge oder Brave gefunden." }

$Out = [IO.Path]::GetFullPath($Out)
if (Test-Path $Out) { Remove-Item -LiteralPath $Out -Confirm:$false }
# Eigenes Profil im Temp-Ordner, damit das normale Browser-Profil unberuehrt bleibt.
$profile = Join-Path $env:TEMP "milo-page-shot"
$arguments = @(
    "--headless=new", "--disable-gpu", "--hide-scrollbars", "--no-first-run", "--force-device-scale-factor=1",
    "--user-data-dir=`"$profile`"", "--window-size=$Width,$Height", "--virtual-time-budget=$Budget",
    "--screenshot=`"$Out`"", "`"$Url`""
)
$process = Start-Process -FilePath $edge -ArgumentList $arguments -PassThru -WindowStyle Hidden
if (-not $process.WaitForExit(30000)) { $process.Kill(); throw "Der Browser hat nicht rechtzeitig geantwortet." }
if (Test-Path $Out) { "gespeichert: $Out" } else { throw "Es wurde kein Bild geschrieben." }
