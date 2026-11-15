// Hilfen fuer die Wetter-Ansicht: Wettercode -> Symbol und Farbstimmung.
import { t } from "./i18n";

export type Sky = "clear" | "partly" | "cloudy" | "fog" | "drizzle" | "rain" | "snow" | "storm";

/** Ordnet die WMO-Wettercodes von Open-Meteo einer Symbolart zu. */
export function sky(code: number): Sky {
  if (code === 0) return "clear";
  if (code === 1 || code === 2) return "partly";
  if (code === 3) return "cloudy";
  if (code === 45 || code === 48) return "fog";
  if (code >= 51 && code <= 57) return "drizzle";
  if ((code >= 61 && code <= 67) || (code >= 80 && code <= 82)) return "rain";
  if ((code >= 71 && code <= 77) || code === 85 || code === 86) return "snow";
  if (code >= 95) return "storm";
  return "cloudy";
}

/** Wetter fuer die Kulisse der Insel (Regen, Schnee, Wolken ...). */
export function sceneryWeather(code: number): "clear" | "cloudy" | "rain" | "snow" | "storm" | "fog" {
  const kind = sky(code);
  if (kind === "partly") return "clear";
  if (kind === "drizzle") return "rain";
  return kind;
}

/** Hintergrundverlauf der Wetterkarte, passend zu Himmel und Tageszeit. */
export function skyGradient(code: number, isDay: boolean): string {
  const kind = sky(code);
  if (!isDay) {
    if (kind === "clear" || kind === "partly") return "linear-gradient(165deg, #101a3c, #24336b)";
    if (kind === "storm") return "linear-gradient(165deg, #15172b, #33365a)";
    return "linear-gradient(165deg, #171c29, #323a4d)";
  }
  switch (kind) {
    case "clear": return "linear-gradient(165deg, #2a78d0, #62b0ee)";
    case "partly": return "linear-gradient(165deg, #3a7dc4, #7fb4e0)";
    case "fog": return "linear-gradient(165deg, #5b6470, #8e98a4)";
    case "drizzle":
    case "rain": return "linear-gradient(165deg, #34495f, #56728f)";
    case "snow": return "linear-gradient(165deg, #6c85a1, #a3b8cf)";
    case "storm": return "linear-gradient(165deg, #2a2d44, #4c5070)";
    default: return "linear-gradient(165deg, #48546a, #74819a)";
  }
}

/** Farbe fuer eine Temperatur: kalt blau, mild gruen-gelb, warm orange-rot. */
export function tempColor(temp: number): string {
  const stops: [number, [number, number, number]][] = [
    [-10, [110, 160, 255]], [0, [100, 200, 255]], [10, [110, 220, 190]], [18, [200, 225, 110]], [25, [255, 190, 80]], [34, [255, 110, 80]],
  ];
  if (temp <= stops[0][0]) return `rgb(${stops[0][1].join(",")})`;
  for (let i = 1; i < stops.length; i++) {
    const [t1, c1] = stops[i];
    const [t0, c0] = stops[i - 1];
    if (temp <= t1) {
      const u = (temp - t0) / (t1 - t0);
      return `rgb(${c0.map((v, k) => Math.round(v + (c1[k] - v) * u)).join(",")})`;
    }
  }
  return `rgb(${stops[stops.length - 1][1].join(",")})`;
}

export const round = (n: number) => Math.round(n);

/** Kurze Beschreibung der Wetterlage fuer Karten ("Klar", "Leichter Regen" ...). */
export function conditionText(code: number, isDay = true): string {
  if (code === 0) return isDay ? t("Sonnig", "Sunny") : t("Klar", "Clear");
  if (code === 1) return isDay ? t("Überwiegend sonnig", "Mostly sunny") : t("Überwiegend klar", "Mostly clear");
  if (code === 2) return t("Teils bewölkt", "Partly cloudy");
  if (code === 3) return t("Bedeckt", "Overcast");
  if (code === 45 || code === 48) return t("Nebel", "Fog");
  if (code >= 51 && code <= 57) return t("Nieselregen", "Drizzle");
  if (code === 61 || code === 80) return t("Leichter Regen", "Light rain");
  if (code === 63 || code === 81) return t("Regen", "Rain");
  if (code === 65 || code === 82) return t("Starker Regen", "Heavy rain");
  if (code === 66 || code === 67) return t("Gefrierender Regen", "Freezing rain");
  if (code === 71 || code === 85) return t("Leichter Schnee", "Light snow");
  if (code === 73 || code === 75 || code === 86) return t("Schnee", "Snow");
  if (code === 77) return t("Schneegriesel", "Snow grains");
  if (code >= 95) return t("Gewitter", "Thunderstorm");
  return t("Bewölkt", "Cloudy");
}
