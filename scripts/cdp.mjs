// Prueft die laufende Milo-App von aussen ueber die Debug-Schnittstelle von WebView2,
// ohne Maus oder Tastatur des Nutzers zu benutzen. Nur fuer den Entwicklungsmodus:
//
//   $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = "--remote-debugging-port=9223"; npm run tauri dev
//
//   node scripts/cdp.mjs eval "<javascript>"     Ausdruck in der Insel auswerten (await erlaubt)
//   node scripts/cdp.mjs shot <datei.png>        Bild der Insel speichern
//   node scripts/cdp.mjs wait "<javascript>" [sekunden]   warten, bis der Ausdruck wahr ist

import { writeFileSync } from "node:fs";
import { connect, evaluate, waitFor } from "./lib/cdp-client.mjs";

const [command, arg, extra] = process.argv.slice(2);

const cdp = await connect();
try {
  if (command === "eval") {
    console.log(JSON.stringify(await evaluate(cdp, arg), null, 2));
  } else if (command === "shot") {
    const { data } = await cdp.send("Page.captureScreenshot", { format: "png" });
    writeFileSync(arg, Buffer.from(data, "base64"));
    console.log("gespeichert:", arg);
  } else if (command === "wait") {
    const ok = await waitFor(cdp, arg, Number(extra ?? 60));
    console.log(ok ? "erfüllt" : "Zeit abgelaufen");
    process.exitCode = ok ? 0 : 1;
  } else {
    console.error("Befehle: eval <js> | shot <datei> | wait <js> [sekunden]");
    process.exitCode = 2;
  }
} finally {
  cdp.close();
}
