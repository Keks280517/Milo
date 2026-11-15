// Allgemeiner Debug-Zugriff für Tests (Browser-Testprofil):
//   node scripts/cdp-any.mjs <port> <url-teil> eval "<js>"
//   node scripts/cdp-any.mjs <port> <url-teil> shot <datei.png>
//   node scripts/cdp-any.mjs <port> <url-teil> nav <url>
//   node scripts/cdp-any.mjs <port> - list
import { writeFileSync } from "node:fs";

const [port, part, command, arg] = process.argv.slice(2);
const targets = await (await fetch(`http://127.0.0.1:${port}/json`)).json();
if (command === "list") {
  for (const t of targets) console.log(t.type, t.url);
  process.exit(0);
}
const t = targets.find((x) => x.url.endsWith(part) && x.type === "page") ?? targets.find((x) => x.url.includes(part) && (x.type === "page" || x.type === "service_worker" || x.type === "iframe" || x.type === "other"));
if (!t) throw new Error("Ziel nicht gefunden: " + targets.map((x) => x.type + " " + x.url).join(" | "));
const ws = new WebSocket(t.webSocketDebuggerUrl);
await new Promise((res, rej) => {
  ws.onopen = res;
  ws.onerror = () => rej(new Error("keine Verbindung"));
});
let id = 0;
const pending = new Map();
ws.onmessage = ({ data }) => {
  const m = JSON.parse(data);
  if (m.id && pending.has(m.id)) {
    const { res, rej } = pending.get(m.id);
    pending.delete(m.id);
    m.error ? rej(new Error(m.error.message)) : res(m.result);
  }
};
const send = (method, params = {}) => new Promise((res, rej) => { pending.set(++id, { res, rej }); ws.send(JSON.stringify({ id, method, params })); });
try {
  if (command === "eval") {
    const { result, exceptionDetails } = await send("Runtime.evaluate", { expression: `(async () => (${arg}))()`, awaitPromise: true, returnByValue: true, userGesture: true });
    if (exceptionDetails) throw new Error(exceptionDetails.exception?.description ?? exceptionDetails.text);
    console.log(JSON.stringify(result.value, null, 2));
  } else if (command === "shot") {
    await send("Page.bringToFront").catch(() => {});
    const { data } = await send("Page.captureScreenshot", { format: "png" });
    writeFileSync(arg, Buffer.from(data, "base64"));
    console.log("gespeichert:", arg);
  } else if (command === "nav") {
    await send("Page.navigate", { url: arg });
    console.log("ok");
  } else if (command === "click") {
    // echter Mausklick an Position "x,y" (isTrusted = true)
    const [x, y] = arg.split(",").map(Number);
    for (const type of ["mousePressed", "mouseReleased"]) await send("Input.dispatchMouseEvent", { type, x, y, button: "left", clickCount: 1 });
    console.log("geklickt");
  } else if (command === "type") {
    await send("Input.insertText", { text: arg });
    console.log("getippt");
  } else if (command === "key") {
    for (const type of ["keyDown", "keyUp"]) await send("Input.dispatchKeyEvent", { type, key: arg, code: arg, windowsVirtualKeyCode: arg === "Enter" ? 13 : 0 });
    console.log("Taste");
  }
} finally {
  ws.close();
}
