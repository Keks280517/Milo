// Zeilenweiser Vergleich fuer die Claude-Arbeitsansicht: welche Zeilen Claude ersetzt und welche
// es neu schreibt. Klassisches LCS, fuer die kurzen Ausschnitte aus den Hooks (hoechstens ein paar
// hundert Zeilen) schnell genug; laengere Texte werden ohne Vergleich als "alt weg, neu dazu" gezeigt.

export interface DiffLine {
  kind: "same" | "add" | "del";
  text: string;
}

const MAX_LINES = 300;

export function diffLines(before: string, after: string): DiffLine[] {
  const a = before ? before.split("\n") : [];
  const b = after ? after.split("\n") : [];
  if (a.length === 0) return b.map((text) => ({ kind: "add", text }));
  if (a.length > MAX_LINES || b.length > MAX_LINES) {
    return [...a.map((text) => ({ kind: "del" as const, text })), ...b.map((text) => ({ kind: "add" as const, text }))];
  }
  // lcs[i][j] = Laenge der laengsten gemeinsamen Folge von a[i..] und b[j..].
  const lcs: number[][] = Array.from({ length: a.length + 1 }, () => new Array<number>(b.length + 1).fill(0));
  for (let i = a.length - 1; i >= 0; i--) {
    for (let j = b.length - 1; j >= 0; j--) {
      lcs[i][j] = a[i] === b[j] ? lcs[i + 1][j + 1] + 1 : Math.max(lcs[i + 1][j], lcs[i][j + 1]);
    }
  }
  const out: DiffLine[] = [];
  let i = 0, j = 0;
  while (i < a.length && j < b.length) {
    if (a[i] === b[j]) {
      out.push({ kind: "same", text: a[i] });
      i++;
      j++;
    } else if (lcs[i + 1][j] >= lcs[i][j + 1]) {
      out.push({ kind: "del", text: a[i++] });
    } else {
      out.push({ kind: "add", text: b[j++] });
    }
  }
  while (i < a.length) out.push({ kind: "del", text: a[i++] });
  while (j < b.length) out.push({ kind: "add", text: b[j++] });
  return out;
}
