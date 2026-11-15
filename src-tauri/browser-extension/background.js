// Milo für den Browser: verbindet sich über Native Messaging (nur lokal) mit Milo und führt dessen
// Aufträge im gerade aktiven Tab aus: anklickbare Elemente auflisten, etwas anklicken, Text lesen,
// tippen, blättern, eine Adresse öffnen. Kein Netzwerk, keine Daten an Dritte.
//
// Sicherheitsregeln (zusätzlich zu denen in Milo): Kaufen, Bezahlen, Löschen, Senden, Konto und
// Ähnliches wird nie angeklickt, in Passwortfelder wird nie getippt.

const HOST = "de.milo.bridge";
let port = null;
let retry = 2000;

function connect() {
  try {
    port = chrome.runtime.connectNative(HOST);
  } catch {
    port = null;
    return later();
  }
  port.onMessage.addListener(async (msg) => {
    if (!msg || typeof msg.id !== "number") return;
    try {
      const result = await handle(msg);
      port?.postMessage({ id: msg.id, ok: true, result });
    } catch (err) {
      port?.postMessage({ id: msg.id, ok: false, error: String(err?.message ?? err) });
    }
  });
  port.onDisconnect.addListener(() => {
    port = null;
    later();
  });
  retry = 2000;
  port.postMessage({ event: "hello", browser: navigator.userAgent, version: chrome.runtime.getManifest().version });
}

function later() {
  // Laeuft Milo nicht, nicht dauernd neu versuchen: langsam bis auf eine Minute strecken.
  setTimeout(connect, retry);
  retry = Math.min(60000, retry * 2);
}

chrome.runtime.onStartup.addListener(() => port || connect());
chrome.runtime.onInstalled.addListener(() => port || connect());
connect();

async function activeTab() {
  const [tab] = await chrome.tabs.query({ active: true, lastFocusedWindow: true });
  if (!tab?.id) throw new Error("Kein aktiver Tab.");
  return tab;
}

async function run(tab, func, args = []) {
  const [res] = await chrome.scripting.executeScript({ target: { tabId: tab.id }, func, args });
  return res?.result;
}

/** Wartet, bis der Tab fertig geladen ist (höchstens `ms`). */
function waitLoaded(tabId, ms) {
  return new Promise((resolve) => {
    const done = () => {
      chrome.tabs.onUpdated.removeListener(listener);
      clearTimeout(timer);
      resolve();
    };
    const listener = (id, info) => id === tabId && info.status === "complete" && done();
    const timer = setTimeout(done, ms);
    chrome.tabs.onUpdated.addListener(listener);
  });
}

async function handle(msg) {
  const tab = await activeTab();
  const info = { title: tab.title ?? "", url: tab.url ?? "" };
  switch (msg.action) {
    case "info":
      return info;
    case "list":
      return { ...info, items: await run(tab, pageList, [msg.limit ?? 80]) };
    case "read":
      return { ...info, text: await run(tab, pageRead, [msg.max ?? 12000]) };
    case "click": {
      const result = await run(tab, pageClick, [msg.name ?? "", msg.forbidden ?? [], !!msg.dryRun]);
      // Nach einem echten Klick kurz warten: Seiten laden nach oder oeffnen eine Vorschau.
      if (result?.clicked) {
        await new Promise((r) => setTimeout(r, 700));
        await waitLoaded(tab.id, 4000);
      }
      const now = await chrome.tabs.get(tab.id).catch(() => tab);
      return { title: now.title ?? info.title, url: now.url ?? info.url, ...result };
    }
    case "type":
      return { ...info, ...(await run(tab, pageType, [msg.text ?? "", !!msg.enter])) };
    case "scroll":
      return { ...info, ...(await run(tab, pageScroll, [!!msg.down, msg.steps ?? 5])) };
    case "navigate": {
      await chrome.tabs.update(tab.id, { url: msg.url });
      await waitLoaded(tab.id, 15000);
      // Seiten wie Netflix laden ihre Treffer nach dem eigentlichen Laden nach.
      await new Promise((r) => setTimeout(r, 1200));
      const now = await chrome.tabs.get(tab.id);
      return { title: now.title ?? "", url: now.url ?? "" };
    }
    default:
      throw new Error(`Unbekannter Auftrag: ${msg.action}`);
  }
}

// ---------------------------------------------------------------------------------------------
// Die folgenden Funktionen laufen IN der Seite (chrome.scripting) und müssen für sich stehen.

function pageList(limit) {
  const SEL = 'a[href], button, [role="button"], [role="link"], [role="menuitem"], [role="option"], [role="tab"], [role="checkbox"], input[type="submit"], input[type="button"], summary, [aria-label][tabindex]';
  const nameOf = (el) => {
    const label = el.getAttribute("aria-label") || el.getAttribute("title") || "";
    const text = (el.innerText || el.value || "").replace(/\s+/g, " ").trim();
    const img = el.querySelector?.("img[alt]")?.getAttribute("alt") || "";
    return (label || text || img).slice(0, 120);
  };
  const openModal = () => {
    const list = [...document.querySelectorAll('[role="dialog"], [aria-modal="true"], dialog[open], .previewModal--container, .detail-modal')];
    return list.reverse().find((m) => {
      const r = m.getBoundingClientRect();
      return r.width > 100 && r.height > 100 && getComputedStyle(m).visibility !== "hidden" && m.getAttribute("aria-hidden") !== "true";
    }) || null;
  };
  const covered = (el) => {
    const r = el.getBoundingClientRect();
    const hit = document.elementFromPoint(Math.min(innerWidth - 1, Math.max(0, r.left + r.width / 2)), Math.min(innerHeight - 1, Math.max(0, r.top + r.height / 2)));
    return !!hit && hit !== el && !el.contains(hit) && !hit.contains(el);
  };
  // Ein offenes Fenster (Vorschau, Dialog) zuerst, dann was sichtbar und nicht verdeckt ist, dann der Rest.
  const modal = openModal();
  const rank = (el) => {
    if (modal && modal.contains(el)) return 0;
    const r = el.getBoundingClientRect();
    const inView = r.bottom > 0 && r.top < innerHeight && r.right > 0 && r.left < innerWidth;
    if (inView && !covered(el)) return 1;
    return inView ? 3 : 2;
  };
  const all = [];
  for (const el of document.querySelectorAll(SEL)) {
    const r = el.getBoundingClientRect();
    if (r.width < 2 || r.height < 2) continue;
    const style = getComputedStyle(el);
    if (style.visibility === "hidden" || style.display === "none") continue;
    const name = nameOf(el);
    if (name) all.push({ name, rank: rank(el) });
  }
  all.sort((x, y) => x.rank - y.rank);
  const seen = new Set();
  const out = [];
  for (const item of all) {
    if (seen.has(item.name)) continue;
    seen.add(item.name);
    out.push(item.name);
    if (out.length >= limit) break;
  }
  return out;
}

function pageRead(max) {
  const root = document.querySelector("main, [role=main], article") || document.body;
  return (root?.innerText || "").replace(/\n{3,}/g, "\n\n").slice(0, max);
}

function pageClick(wanted, forbidden, dryRun) {
  const SEL = 'a[href], button, [role="button"], [role="link"], [role="menuitem"], [role="option"], [role="tab"], [role="checkbox"], input[type="submit"], input[type="button"], summary, [aria-label], [tabindex]';
  const norm = (s) => s.toLowerCase().normalize("NFD").replace(/[̀-ͯ]/g, "").replace(/[^a-z0-9äöüß ]+/g, " ").replace(/\s+/g, " ").trim();
  // Woerter, die nichts unterscheiden: nach ihnen allein wird nie geklickt.
  const STOP = new Set(["der", "die", "das", "den", "dem", "the", "a", "an", "ein", "eine", "und", "and", "of", "von", "auf", "in", "im", "zu", "to", "mit"]);
  const nameOf = (el) => {
    const label = el.getAttribute("aria-label") || el.getAttribute("title") || "";
    const text = (el.innerText || el.value || "").replace(/\s+/g, " ").trim();
    const img = el.querySelector?.("img[alt]")?.getAttribute("alt") || "";
    return (label || text || img).slice(0, 160);
  };
  // Klang statt Schreibweise (Spracherkennung): Filip = Philipp, Mittel = Middle.
  const RULES = [["sch", "s"], ["ch", "k"], ["ck", "k"], ["ph", "f"], ["pf", "f"], ["th", "d"], ["qu", "kw"], ["tz", "s"], ["ie", "i"], ["ee", "i"], ["ea", "i"], ["oo", "u"], ["ou", "u"], ["y", "i"], ["c", "k"], ["z", "s"], ["v", "f"], ["w", "f"], ["t", "d"], ["p", "b"], ["g", "k"], ["x", "ks"], ["h", ""]];
  const soundKey = (text) => {
    const words = norm(text).replace(/ä/g, "a").replace(/ö/g, "o").replace(/ü/g, "u").replace(/ß/g, "s").split(" ").filter(Boolean);
    const kept = words.filter((w) => !STOP.has(w));
    return (kept.length ? kept : words)
      .map((w) => {
        if (w.length > 3 && w.endsWith("le") && !w.endsWith("ele")) w = w.slice(0, -2) + "el";
        for (const [f, t] of RULES) w = w.split(f).join(t);
        return w.replace(/(.)\1+/g, "$1");
      })
      .filter(Boolean)
      .join(" ");
  };
  const dist = (a, b) => {
    let prev = Array.from({ length: b.length + 1 }, (_, i) => i);
    for (let i = 0; i < a.length; i++) {
      const cur = [i + 1];
      for (let j = 0; j < b.length; j++) cur.push(Math.min(prev[j] + (a[i] === b[j] ? 0 : 1), prev[j + 1] + 1, cur[j] + 1));
      prev = cur;
    }
    return prev[b.length];
  };
  const similar = (found, want) => {
    const f = soundKey(found), w = soundKey(want);
    if (!f || !w) return 0;
    const score = (a, b) => 1 - dist(a, b) / Math.max(a.length, b.length);
    let best = score(f, w);
    const fw = f.split(" "), size = w.split(" ").length;
    if (fw.length > size) for (let i = 0; i + size <= fw.length; i++) best = Math.max(best, score(fw.slice(i, i + size).join(" "), w) - 0.03);
    return best;
  };
  const modal = (() => {
    const list = [...document.querySelectorAll('[role="dialog"], [aria-modal="true"], dialog[open], .previewModal--container, .detail-modal')];
    return list.reverse().find((m) => {
      const r = m.getBoundingClientRect();
      return r.width > 100 && r.height > 100 && getComputedStyle(m).visibility !== "hidden" && m.getAttribute("aria-hidden") !== "true";
    }) || null;
  })();
  const covered = (el) => {
    const r = el.getBoundingClientRect();
    if (r.bottom <= 0 || r.top >= innerHeight || r.right <= 0 || r.left >= innerWidth) return false; // ausserhalb: nicht pruefbar
    const hit = document.elementFromPoint(Math.min(innerWidth - 1, Math.max(0, r.left + r.width / 2)), Math.min(innerHeight - 1, Math.max(0, r.top + r.height / 2)));
    return !!hit && hit !== el && !el.contains(hit) && !hit.contains(el);
  };

  const want = norm(wanted);
  if (!want) return { error: "Leerer Name." };
  const words = want.split(" ").filter((w) => w.length > 1 && !STOP.has(w));
  const viewArea = innerWidth * innerHeight;
  let best = null;
  const names = [];
  for (const el of document.querySelectorAll(SEL)) {
    const r = el.getBoundingClientRect();
    if (r.width < 2 || r.height < 2) continue;
    const style = getComputedStyle(el);
    if (style.visibility === "hidden" || style.display === "none") continue;
    const name = nameOf(el);
    const n = norm(name);
    if (!n) continue;
    names.push(name);
    // Nur Fuellwoerter gesucht („the“): nur ein exakt gleicher Name zaehlt.
    if (!words.length && n !== want) continue;
    let score = 0;
    if (n === want) score = 100;
    else if (n.startsWith(want)) score = 82;
    else if (want.length >= 3 && n.includes(want)) score = 64;
    else {
      // Einzelne Woerter zaehlen nur, wenn die meisten bedeutsamen Woerter vorkommen.
      const parts = n.split(" ");
      const hit = words.filter((w) => parts.some((x) => x === w || (w.length > 3 && x.startsWith(w)))).length;
      if (words.length && hit / words.length >= 0.6) score = 30 + (25 * hit) / words.length;
      // Anders geschrieben, aber gleich klingend.
      if (n.length < 80) {
        const sim = similar(name, wanted);
        if (sim >= 0.74) score = Math.max(score, 40 + sim * 30);
      }
    }
    if (score <= 0) continue;
    // Kuerzere Namen sind genauer (Titel statt Beschreibung); riesige Flaechen (Banner oben) sind selten gemeint.
    score -= Math.min(15, n.length / 30);
    if ((r.width * r.height) / viewArea > 0.3) score -= 15;
    // Ein offenes Fenster (z. B. die Vorschau einer Serie) gewinnt vor der Seite dahinter.
    if (modal) score += modal.contains(el) ? 25 : -25;
    // Verdeckt (von einem Fenster oder Banner ueberlagert): kaum klickbar.
    if (covered(el)) score -= 30;
    if (score > 0 && (!best || score > best.score)) best = { el, name, score };
  }
  if (!best || best.score < 20) {
    const near = names.filter((x) => words.some((w) => w.length > 2 && norm(x).includes(w)) || (x.length < 80 && similar(x, wanted) >= 0.6)).slice(0, 12);
    return { error: `„${wanted}“ finde ich auf der Seite nicht.`, similar: near };
  }
  const lower = norm(best.name);
  if (forbidden.some((f) => lower.includes(norm(f)))) return { error: `„${best.name}“ klicke ich aus Sicherheitsgründen nicht.`, refused: true };
  if (dryRun) return { name: best.name };
  const el = best.el;
  el.scrollIntoView({ block: "center", inline: "center" });
  el.focus?.({ preventScroll: true });
  // Wie eine echte Maus: manche Seiten (React) reagieren erst auf Zeiger- und Maus-Ereignisse.
  const box = el.getBoundingClientRect();
  const at = { bubbles: true, cancelable: true, view: window, clientX: box.left + box.width / 2, clientY: box.top + box.height / 2, button: 0 };
  for (const type of ["pointerover", "mouseover", "pointerdown", "mousedown", "pointerup", "mouseup"]) {
    el.dispatchEvent(type.startsWith("pointer") ? new PointerEvent(type, { ...at, pointerType: "mouse", isPrimary: true }) : new MouseEvent(type, at));
  }
  el.click();
  return { name: best.name, clicked: true };
}

function pageType(text, enter) {
  let el = document.activeElement;
  const editable = (e) => e && (e.isContentEditable || ((e.tagName === "INPUT" || e.tagName === "TEXTAREA") && !e.disabled && !e.readOnly));
  if (!editable(el)) {
    // Kein Feld im Fokus: das erste sichtbare Suchfeld nehmen.
    el = [...document.querySelectorAll('input[type="search"], input[name*="search" i], input[placeholder*="such" i], input[placeholder*="search" i], input[type="text"]')].find((e) => e.getBoundingClientRect().width > 0) || null;
    el?.focus();
  }
  if (!editable(el)) return { error: "Ich finde kein Eingabefeld." };
  if (el.type === "password") return { error: "In Passwortfelder tippe ich nicht.", refused: true };
  if (el.isContentEditable) document.execCommand("insertText", false, text);
  else {
    const setter = Object.getOwnPropertyDescriptor(Object.getPrototypeOf(el), "value")?.set;
    setter ? setter.call(el, text) : (el.value = text);
    el.dispatchEvent(new Event("input", { bubbles: true }));
    el.dispatchEvent(new Event("change", { bubbles: true }));
  }
  if (enter) {
    for (const type of ["keydown", "keypress", "keyup"]) el.dispatchEvent(new KeyboardEvent(type, { key: "Enter", code: "Enter", keyCode: 13, which: 13, bubbles: true }));
    el.form?.requestSubmit?.();
  }
  return { typed: true, field: el.getAttribute("aria-label") || el.placeholder || el.name || "" };
}

function pageScroll(down, steps) {
  window.scrollBy({ top: (down ? 1 : -1) * steps * 160, behavior: "smooth" });
  return { scrolled: true };
}
