//! Inline SVG icons. Strokes follow `currentColor`, so pressed states,
//! disabled buttons and focus styles restyle them for free. No external
//! assets, no icon font, no platform-dependent Unicode glyphs.

export type IconName =
  | "play"
  | "pause"
  | "restart"
  | "skip-start"
  | "skip-end"
  | "step-back"
  | "step-forward"
  | "fit"
  | "follow"
  | "fullscreen"
  | "exit-fullscreen"
  | "compare"
  | "close"
  | "help"
  | "open"
  | "download"
  | "copy"
  | "warning"
  | "check"
  | "trash"
  | "chevron-down"
  | "runs"
  | "info";

type IconPart = { d: string; fill?: boolean } | { circle: [number, number, number] };

const PARTS: Record<IconName, IconPart[]> = {
  play: [{ d: "M7 4.5 18.5 12 7 19.5Z", fill: true }],
  pause: [
    { d: "M7.5 4.5h3v15h-3z", fill: true },
    { d: "M13.5 4.5h3v15h-3z", fill: true },
  ],
  restart: [{ d: "M6 5v14" }, { d: "M19 5 9.5 12 19 19Z", fill: true }],
  "skip-start": [{ d: "M5 5v14" }, { d: "M19 5 9 12l10 7Z", fill: true }],
  "skip-end": [{ d: "M19 5v14" }, { d: "M5 5l10 7-10 7Z", fill: true }],
  "step-back": [{ d: "M15 6l-6 6 6 6" }],
  "step-forward": [{ d: "M9 6l6 6-6 6" }],
  fit: [
    { d: "M3 9V3h6" },
    { d: "M15 3h6v6" },
    { d: "M21 15v6h-6" },
    { d: "M9 21H3v-6" },
    { d: "M10 10h4v4h-4Z" },
  ],
  follow: [
    { circle: [12, 12, 7] },
    { d: "M12 2v3" },
    { d: "M12 19v3" },
    { d: "M2 12h3" },
    { d: "M19 12h3" },
  ],
  fullscreen: [
    { d: "M9 3H3v6" },
    { d: "M15 3h6v6" },
    { d: "M21 15v6h-6" },
    { d: "M3 15v6h6" },
  ],
  "exit-fullscreen": [
    { d: "M9 3v6H3" },
    { d: "M15 3v6h6" },
    { d: "M21 15h-6v6" },
    { d: "M3 15h6v6" },
  ],
  compare: [
    { d: "M4 8h13" },
    { d: "M14 4l4 4-4 4" },
    { d: "M20 16H7" },
    { d: "M10 12l-4 4 4 4" },
  ],
  close: [
    { d: "M6 6l12 12" },
    { d: "M18 6 6 18" },
  ],
  help: [
    { circle: [12, 12, 9] },
    { d: "M9.4 9a2.7 2.7 0 1 1 3.7 2.5c-.8.35-1.1.9-1.1 1.8" },
    { d: "M12 17.2h.01" },
  ],
  open: [
    { d: "M3 7.5h6l2 2h10v9.5a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3 19Z" },
    { d: "M3 7.5V5a1.5 1.5 0 0 1 1.5-1.5H9L11 5.5" },
  ],
  download: [
    { d: "M12 3v12" },
    { d: "M7 10l5 5 5-5" },
    { d: "M4 20h16" },
  ],
  copy: [
    { d: "M8 8h11v12H8Z" },
    { d: "M16 8V4H5v12h3" },
  ],
  warning: [
    { d: "M12 3 2.8 20h18.4Z" },
    { d: "M12 9v5" },
    { d: "M12 17.3h.01" },
  ],
  check: [{ d: "M4 12.5 9.5 18 20 6.5" }],
  trash: [
    { d: "M4 7h16" },
    { d: "M9 7V4h6v3" },
    { d: "M6 7l1 13h10l1-13" },
    { d: "M10 11v5" },
    { d: "M14 11v5" },
  ],
  "chevron-down": [{ d: "M6 9l6 6 6-6" }],
  runs: [
    { d: "M4 6h16" },
    { d: "M4 12h16" },
    { d: "M4 18h16" },
  ],
  info: [
    { circle: [12, 12, 9] },
    { d: "M12 11v5" },
    { d: "M12 8h.01" },
  ],
};

const NS = "http://www.w3.org/2000/svg";

export function icon(name: IconName, size = 16): SVGSVGElement {
  const svg = document.createElementNS(NS, "svg");
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("width", String(size));
  svg.setAttribute("height", String(size));
  svg.setAttribute("fill", "none");
  svg.setAttribute("stroke", "currentColor");
  svg.setAttribute("stroke-width", "2");
  svg.setAttribute("stroke-linecap", "round");
  svg.setAttribute("stroke-linejoin", "round");
  svg.setAttribute("aria-hidden", "true");
  for (const part of PARTS[name]) {
    if ("circle" in part) {
      const circle = document.createElementNS(NS, "circle");
      circle.setAttribute("cx", String(part.circle[0]));
      circle.setAttribute("cy", String(part.circle[1]));
      circle.setAttribute("r", String(part.circle[2]));
      svg.append(circle);
      continue;
    }
    const path = document.createElementNS(NS, "path");
    path.setAttribute("d", part.d);
    if (part.fill) {
      path.setAttribute("fill", "currentColor");
      path.setAttribute("stroke", "none");
    }
    svg.append(path);
  }
  return svg;
}
