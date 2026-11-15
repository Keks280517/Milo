// Verbindung zur Debug-Schnittstelle der laufenden Milo-App (nur Entwicklungsmodus).

const PORT = 9223;

export async function connect() {
  const targets = await (await fetch(`http://127.0.0.1:${PORT}/json`)).json();
  const page = targets.find((t) => t.type === "page" && /localhost:1420\/?$|tauri\.localhost/.test(t.url));
  if (!page) throw new Error("Insel nicht gefunden. Seiten: " + targets.map((t) => t.url).join(", "));
  const ws = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => {
    ws.onopen = resolve;
    ws.onerror = () => reject(new Error("Verbindung zur Debug-Schnittstelle fehlgeschlagen"));
  });
  let id = 0;
  const pending = new Map();
  ws.onmessage = ({ data }) => {
    const msg = JSON.parse(data);
    if (msg.id && pending.has(msg.id)) {
      const { resolve, reject } = pending.get(msg.id);
      pending.delete(msg.id);
      msg.error ? reject(new Error(msg.error.message)) : resolve(msg.result);
    }
  };
  const send = (method, params = {}) =>
    new Promise((resolve, reject) => {
      pending.set(++id, { resolve, reject });
      ws.send(JSON.stringify({ id, method, params }));
    });
  return { send, close: () => ws.close() };
}

/** Wertet einen Ausdruck in der Insel aus (await erlaubt) und liefert seinen Wert. */
export async function evaluate(cdp, expression) {
  const { result, exceptionDetails } = await cdp.send("Runtime.evaluate", {
    expression: `(async () => (${expression}))()`,
    awaitPromise: true,
    returnByValue: true,
  });
  if (exceptionDetails) throw new Error(exceptionDetails.exception?.description ?? exceptionDetails.text);
  return result.value;
}

/** Wartet, bis der Ausdruck wahr wird; liefert false, wenn die Zeit ablaeuft. */
export async function waitFor(cdp, expression, seconds = 60) {
  const deadline = Date.now() + seconds * 1000;
  while (Date.now() < deadline) {
    if (await evaluate(cdp, expression)) return true;
    await new Promise((r) => setTimeout(r, 250));
  }
  return false;
}
