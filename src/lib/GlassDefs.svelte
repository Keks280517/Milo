<script lang="ts">
  // Glas: jede Glasflaeche bekommt eine eigene Linse in ihrer echten Groesse und Rundung.
  //
  // Die Linse ist ein SVG-Filter im backdrop-filter (WebView2/Chromium kann das): eine
  // Verschiebungskarte, die den Inhalt hinter dem Glas NUR im Randband bricht (zur Mitte hin, an der
  // Kante am staerksten, in der Flaeche gar nicht), so wie eine gewoelbte Glaskante. Rot, Gruen und
  // Blau werden leicht unterschiedlich stark gebrochen, das gibt den feinen Farbsaum am Rand.
  // Die Karte wird je Form einmal per Canvas gerechnet (abgerundetes Rechteck als Abstandsfeld) und
  // nach Groesse zwischengespeichert. Dazu das Glanzlicht, das dem Mauszeiger folgt.
  import { onMount } from "svelte";

  const SVG = "http://www.w3.org/2000/svg";
  /** Die Karte wird in halber Aufloesung gerechnet (sie ist ohnehin weich). */
  const RES = 0.5;
  const MAX_FILTERS = 80;

  let defs = $state<SVGSVGElement>();

  /** Verschiebungskarte fuer ein abgerundetes Rechteck: Rot/Gruen = Richtung zur Mitte mal Staerke. */
  function lensMap(w: number, h: number, r: number, band: number): string {
    const cw = Math.max(2, Math.round(w * RES)), ch = Math.max(2, Math.round(h * RES));
    const canvas = document.createElement("canvas");
    canvas.width = cw;
    canvas.height = ch;
    const g = canvas.getContext("2d")!;
    const image = g.createImageData(cw, ch);
    const hw = w / 2, hh = h / 2;
    const rad = Math.min(r, hw, hh);
    const reach = Math.min(band, hw, hh);
    for (let j = 0; j < ch; j++) {
      for (let i = 0; i < cw; i++) {
        // Punkt relativ zur Mitte, in echten Pixeln.
        const px = ((i + 0.5) / cw) * w - hw, py = ((j + 0.5) / ch) * h - hh;
        // Abstandsfeld des abgerundeten Rechtecks: innen negativ, an der Kante 0.
        const qx = Math.abs(px) - (hw - rad), qy = Math.abs(py) - (hh - rad);
        const ox = Math.max(qx, 0), oy = Math.max(qy, 0);
        const outside = Math.hypot(ox, oy);
        const dist = outside + Math.min(Math.max(qx, qy), 0) - rad;
        const depth = -dist; // wie weit innen
        let nx = 0, ny = 0;
        if (depth < reach) {
          // Flaechennormale nach aussen: in der Ecke radial, an den Seiten quer zur Kante.
          if (outside > 0.0001) {
            nx = (ox / outside) * Math.sign(px);
            ny = (oy / outside) * Math.sign(py);
          } else if (qx > qy) nx = Math.sign(px);
          else ny = Math.sign(py);
          // Woelbung: an der Kante steil, zur Flaeche hin sanft auslaufend.
          const t = 1 - Math.max(depth, 0) / reach;
          const k = t * t * (3 - 2 * t) * t;
          nx *= -k;
          ny *= -k;
        }
        const o = (j * cw + i) * 4;
        image.data[o] = 128 + nx * 127;
        image.data[o + 1] = 128 + ny * 127;
        image.data[o + 2] = 128;
        image.data[o + 3] = 255;
      }
    }
    g.putImageData(image, 0, 0);
    return canvas.toDataURL("image/png");
  }

  function el(name: string, attrs: Record<string, string | number>): SVGElement {
    const node = document.createElementNS(SVG, name);
    for (const [key, value] of Object.entries(attrs)) node.setAttribute(key, String(value));
    return node;
  }

  const made = new Map<string, string>();
  let counter = 0;

  /** Filter fuer genau diese Form; gleiche Formen teilen sich einen. */
  function filterFor(w: number, h: number, r: number, band: number, bend: number, split: number): string {
    const key = `${w}x${h}r${r}b${band}s${bend}c${split}`;
    const known = made.get(key);
    if (known) return known;
    if (made.size >= MAX_FILTERS) {
      // Aelteste Form verwerfen (Elemente, die sie noch benutzen, bekommen beim naechsten Messen eine neue).
      const [oldKey, oldId] = made.entries().next().value as [string, string];
      made.delete(oldKey);
      defs?.querySelector(`#${oldId}`)?.remove();
    }
    const id = `milo-lens-${++counter}`;
    const filter = el("filter", { id, x: 0, y: 0, width: w, height: h, filterUnits: "userSpaceOnUse", primitiveUnits: "userSpaceOnUse", "color-interpolation-filters": "sRGB" });
    const image = el("feImage", { x: 0, y: 0, width: w, height: h, preserveAspectRatio: "none", result: "lens" });
    image.setAttribute("href", lensMap(w, h, r, band));
    filter.append(image);
    if (split > 0) {
      // Drei Durchgaenge mit leicht verschiedener Staerke, je einer pro Farbkanal, wieder addiert.
      const channels: [string, number, string][] = [
        ["r", 1 + 0.16 * split, "1 0 0 0 0  0 0 0 0 0  0 0 0 0 0  0 0 0 1 0"],
        ["g", 1, "0 0 0 0 0  0 1 0 0 0  0 0 0 0 0  0 0 0 1 0"],
        ["b", 1 - 0.16 * split, "0 0 0 0 0  0 0 0 0 0  0 0 1 0 0  0 0 0 1 0"],
      ];
      for (const [name, factor, matrix] of channels) {
        filter.append(
          el("feDisplacementMap", { in: "SourceGraphic", in2: "lens", scale: bend * factor, xChannelSelector: "R", yChannelSelector: "G", result: `d${name}` }),
          el("feColorMatrix", { in: `d${name}`, type: "matrix", values: matrix, result: name }),
        );
      }
      filter.append(
        el("feComposite", { in: "r", in2: "g", operator: "arithmetic", k1: 0, k2: 1, k3: 1, k4: 0, result: "rg" }),
        el("feComposite", { in: "rg", in2: "b", operator: "arithmetic", k1: 0, k2: 1, k3: 1, k4: 0 }),
      );
    } else {
      filter.append(el("feDisplacementMap", { in: "SourceGraphic", in2: "lens", scale: bend, xChannelSelector: "R", yChannelSelector: "G" }));
    }
    defs?.append(filter);
    made.set(key, id);
    return id;
  }

  onMount(() => {
    const root = document.documentElement;
    const pending = new Set<HTMLElement>();
    let timer = 0;

    function fit(glass: HTMLElement) {
      if (!glass.isConnected) return;
      const style = getComputedStyle(glass);
      const bend = Number(style.getPropertyValue("--glass-bend")) || 0;
      const w = Math.round(glass.offsetWidth), h = Math.round(glass.offsetHeight);
      if (!bend || w < 12 || h < 12) {
        glass.style.removeProperty("backdrop-filter");
        return;
      }
      const band = Number(style.getPropertyValue("--glass-band")) || 16;
      const split = Number(style.getPropertyValue("--glass-split")) || 0;
      const raw = style.borderTopLeftRadius;
      const r = Math.round(raw.endsWith("%") ? (Math.min(w, h) * parseFloat(raw)) / 100 : parseFloat(raw) || 0);
      // Kleine Teile haben einen schmaleren Rand und brechen schwaecher, sonst bliebe keine ruhige Mitte.
      const fitBand = Math.round(Math.min(band, Math.min(w, h) * 0.34));
      const fitBend = Math.round(bend * (fitBand / band));
      const id = filterFor(w, h, Math.min(r, Math.floor(Math.min(w, h) / 2)), fitBand, fitBend, split);
      // Erst weichzeichnen, dann brechen: so bleibt die gebrochene Kante scharf.
      glass.style.setProperty("backdrop-filter", `var(--glass-blur) url(#${id})`);
    }
    function flush() {
      timer = 0;
      for (const glass of pending) fit(glass);
      pending.clear();
    }
    /** Erst messen, wenn die Groesse kurz stillsteht (waehrend einer Animation waere jede Karte verloren). */
    function queue(glass: HTMLElement) {
      pending.add(glass);
      clearTimeout(timer);
      timer = window.setTimeout(flush, 70);
    }

    const sizes = new ResizeObserver((entries) => {
      for (const entry of entries) queue(entry.target as HTMLElement);
    });
    const seen = new WeakSet<Element>();
    function adopt(node: Node) {
      if (!(node instanceof HTMLElement)) return;
      const list = node.matches(".glass") ? [node, ...node.querySelectorAll<HTMLElement>(".glass")] : node.querySelectorAll<HTMLElement>(".glass");
      for (const glass of list) {
        if (seen.has(glass)) continue;
        seen.add(glass);
        sizes.observe(glass);
      }
    }
    adopt(document.body);
    const added = new MutationObserver((records) => {
      for (const record of records) {
        if (record.type === "attributes") adopt(record.target);
        else for (const node of record.addedNodes) adopt(node);
      }
    });
    added.observe(document.body, { subtree: true, childList: true, attributes: true, attributeFilter: ["class"] });
    // Wechselt die Glasstaerke in den Einstellungen, alle Flaechen neu rechnen.
    const strength = new MutationObserver(() => {
      for (const glass of document.querySelectorAll<HTMLElement>(".glass")) queue(glass);
    });
    strength.observe(root, { attributes: true, attributeFilter: ["data-glass"] });

    // Glanzlicht: die Position der Maus auf der Glasflaeche als CSS-Variablen.
    let current: HTMLElement | null = null;
    const move = (e: PointerEvent) => {
      const glass = (e.target as HTMLElement | null)?.closest?.<HTMLElement>(".glass") ?? null;
      if (current && current !== glass) current.style.removeProperty("--lit");
      current = glass;
      if (!glass) return;
      const r = glass.getBoundingClientRect();
      glass.style.setProperty("--mx", `${((e.clientX - r.left) / r.width) * 100}%`);
      glass.style.setProperty("--my", `${((e.clientY - r.top) / r.height) * 100}%`);
      glass.style.setProperty("--lit", "1");
    };
    window.addEventListener("pointermove", move, { passive: true });
    return () => {
      window.removeEventListener("pointermove", move);
      sizes.disconnect();
      added.disconnect();
      strength.disconnect();
      clearTimeout(timer);
    };
  });
</script>

<svg bind:this={defs} width="0" height="0" style="position: absolute" aria-hidden="true"></svg>
