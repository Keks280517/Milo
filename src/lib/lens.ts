// Gleitende Linse: ein einzelnes Glasstueck, das in einer Leiste oder einem Umschalter zur
// gewaehlten Stelle gleitet (statt dass jede Stelle ihre eigene Markierung ein- und ausblendet).
//
//   <div class="segments" use:lens={{ selector: "button.on" }}>
//     <span class="lens-thumb"></span> ... Knoepfe ...
//   </div>
//
// Die Aktion misst das gewaehlte Kind und schreibt Lage und Groesse als CSS-Variablen an den
// Behaelter (--lens-x/-y/-w/-h/-on). Das Aussehen der Linse bestimmt die Ansicht (.lens-thumb).

export interface LensOptions {
  /** Welches Kind ist gewaehlt (CSS-Selektor relativ zum Behaelter). */
  selector: string;
  /** Innerhalb des gewaehlten Kindes das Stueck, das die Linse umschliesst (z. B. nur das Symbol). */
  inner?: string;
  /** Die Linse ragt um so viele Pixel ueber das Ziel hinaus. */
  grow?: number;
}

export function lens(host: HTMLElement, options: LensOptions) {
  let opts = options;
  let first = true;
  let frame = 0;
  host.classList.add("lens-host");

  function measure() {
    frame = 0;
    const picked = host.querySelector<HTMLElement>(opts.selector);
    const target = picked && opts.inner ? picked.querySelector<HTMLElement>(opts.inner) ?? picked : picked;
    if (!target || !target.offsetWidth) {
      host.style.setProperty("--lens-on", "0");
      return;
    }
    // Lage ueber offset-Werte statt getBoundingClientRect: unabhaengig von Skalierung und
    // laufenden Verschiebungen (die gewaehlte Stelle hebt sich z. B. an).
    let x = 0, y = 0;
    for (let el: HTMLElement | null = target; el && el !== host; el = el.offsetParent as HTMLElement | null) {
      x += el.offsetLeft;
      y += el.offsetTop;
      if (el.offsetParent && !host.contains(el.offsetParent)) break;
    }
    const grow = opts.grow ?? 0;
    if (first) host.dataset.lensStill = "1";
    host.style.setProperty("--lens-x", `${x - grow}px`);
    host.style.setProperty("--lens-y", `${y - grow}px`);
    host.style.setProperty("--lens-w", `${target.offsetWidth + grow * 2}px`);
    host.style.setProperty("--lens-h", `${target.offsetHeight + grow * 2}px`);
    host.style.setProperty("--lens-on", "1");
    if (first) {
      first = false;
      // Beim ersten Mal ohne Fahrt an die Stelle setzen, danach gleiten.
      requestAnimationFrame(() => requestAnimationFrame(() => delete host.dataset.lensStill));
    }
  }
  const queue = () => {
    if (!frame) frame = requestAnimationFrame(measure);
  };

  const mutations = new MutationObserver(queue);
  mutations.observe(host, { subtree: true, childList: true, attributes: true, attributeFilter: ["class", "aria-selected", "aria-current", "aria-pressed", "hidden"] });
  const sizes = new ResizeObserver(queue);
  sizes.observe(host);
  queue();

  return {
    update(next: LensOptions) {
      opts = next;
      queue();
    },
    destroy() {
      mutations.disconnect();
      sizes.disconnect();
      if (frame) cancelAnimationFrame(frame);
    },
  };
}
