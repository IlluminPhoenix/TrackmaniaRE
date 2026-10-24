//! Live input display: the on-screen keys, gear and speed at the playback
//! time, in the style of the TICK overlay. Everything is driven by the same
//! clock as the transport, so scrubbing animates it.

import { selectedRun, type App } from "../app";
import { cachedRoll, rollAt, samplesAt, type RollData } from "../lib/derive";
import { h } from "../lib/dom";
import { fmtSteerPercent, fmtTime, STEER_FULL } from "../lib/format";
import type { RunData } from "../lib/types";

const SVG_NS = "http://www.w3.org/2000/svg";
const SEGMENTS = 24;
const FLASH_MS = 400;
const FILL_COLOR = "#f72585";
const IDLE_COLOR = "#1a2431";
const OUTLINE_COLOR = "#3a4a5a";

/**
 * Arrow key outlines. The pedals are square, the steering keys are wider so a
 * partial fill is readable.
 */
const ARROW: Record<string, { box: [number, number]; path: string }> = {
  up: { box: [24, 24], path: "M12 2 L21 11 H16 V22 H8 V11 H3 Z" },
  down: { box: [24, 24], path: "M12 22 L3 13 H8 V2 H16 V13 H21 Z" },
  left: { box: [30, 20], path: "M2 10 L11 1 L11 6 H28 V14 H11 V19 Z" },
  right: { box: [30, 20], path: "M28 10 L19 1 L19 6 H2 V14 H19 V19 Z" },
};

interface Arrow {
  el: SVGSVGElement;
  /** Fills from the base of the arrow, 0..1. */
  set(value: number): void;
}

function svgElement<K extends keyof SVGElementTagNameMap>(
  tag: K,
  attrs: Record<string, string>,
): SVGElementTagNameMap[K] {
  const el = document.createElementNS(SVG_NS, tag);
  for (const [key, value] of Object.entries(attrs)) el.setAttribute(key, value);
  return el;
}

function makeArrow(direction: keyof typeof ARROW): Arrow {
  const vertical = direction === "up" || direction === "down";
  const [boxWidth, boxHeight] = ARROW[direction].box;
  const id = `live-fill-${direction}`;
  const rect = svgElement("rect", {
    width: String(boxWidth),
    height: String(boxHeight),
    fill: FILL_COLOR,
  });
  const clip = svgElement("clipPath", { id });
  clip.append(rect);

  const path = ARROW[direction].path;
  // The unlit part of the key stays visible as a dark shape, so how much of the
  // arrow is filled is readable at a glance.
  const body = svgElement("path", { d: path, fill: IDLE_COLOR });
  const fill = svgElement("path", { d: path, fill: FILL_COLOR, "clip-path": `url(#${id})` });
  const outline = svgElement("path", {
    d: path,
    fill: "none",
    stroke: OUTLINE_COLOR,
    "stroke-width": "1.3",
    "stroke-linejoin": "round",
  });
  const el = svgElement("svg", {
    viewBox: `0 0 ${boxWidth} ${boxHeight}`,
    class: `live-arrow ${direction}`,
    role: "img",
    "aria-label": direction,
  });
  el.append(clip, body, fill, outline);

  return {
    el,
    set(value: number) {
      const fraction = Math.max(0, Math.min(1, value));
      if (vertical) {
        const height = boxHeight * fraction;
        rect.setAttribute("y", direction === "up" ? String(boxHeight - height) : "0");
        rect.setAttribute("height", String(height));
      } else {
        const width = boxWidth * fraction;
        rect.setAttribute("x", direction === "right" ? "0" : String(boxWidth - width));
        rect.setAttribute("width", String(width));
      }
    },
  };
}

/** Tachometer ramp: yellow at the bottom, green in the middle, red at the top. */
function segmentColor(index: number): string {
  const t = SEGMENTS <= 1 ? 0 : index / (SEGMENTS - 1);
  const hue = t < 0.5 ? 55 + (t / 0.5) * 65 : 120 - ((t - 0.5) / 0.5) * 120;
  return `hsl(${Math.round(hue)} 70% 45%)`;
}

/** True when a respawn happened within the last `FLASH_MS` of playback. */
function recentRespawn(times: number[], t: number): boolean {
  for (let i = times.length - 1; i >= 0; i--) {
    const dt = t - times[i];
    if (dt >= 0 && dt <= FLASH_MS) return true;
    if (dt > FLASH_MS) break;
  }
  return false;
}

export function mountLive(root: HTMLElement, app: App): void {
  const arrows = {
    up: makeArrow("up"),
    down: makeArrow("down"),
    left: makeArrow("left"),
    right: makeArrow("right"),
  };
  const pad = h(
    "div",
    { class: "live-pad" },
    h("div", { class: "live-slot up" }, arrows.up.el),
    h("div", { class: "live-slot left" }, arrows.left.el),
    h("div", { class: "live-slot down" }, arrows.down.el),
    h("div", { class: "live-slot right" }, arrows.right.el),
  );

  const gearValue = h("span", { class: "gear-value", text: "–" });
  const speedValue = h("span", { class: "speed-value", text: "0" });
  const speedScale = h("span", { class: "speed-scale", text: "0 – 100 km/h" });
  const steerValue = h("span", { class: "steer-value", text: "0%" });
  const steerReadout = h(
    "div",
    { class: "steer-readout" },
    h("span", { class: "steer-label", text: "steer" }),
    steerValue,
  );
  const segments = Array.from({ length: SEGMENTS }, (_, index) =>
    h("span", { class: "seg", style: `background:${segmentColor(index)}` }),
  );
  const panel = h(
    "div",
    { class: "live" },
    h("div", { class: "live-column" }, pad, steerReadout),
    h(
      "div",
      { class: "live-right" },
      h("div", { class: "gear" }, gearValue, h("span", { class: "gear-label", text: "gear" })),
      h(
        "div",
        { class: "speed" },
        h("div", { class: "speed-bar" }, ...segments),
        h("div", { class: "speed-line" }, speedValue, h("span", { class: "muted", text: " km/h" })),
        speedScale,
      ),
    ),
  );
  const stateLine = h("div", { class: "live-state mono small" });
  const hint = h("div", { class: "live-hint muted small" });
  root.append(hint, panel, stateLine);

  let run: RunData | null = null;
  let roll: RollData | null = null;
  let fullScaleKmh = 100;
  let litSegments = -1;

  function select(next: RunData | null): void {
    if (next?.id === run?.id) return;
    run = next;
    roll = next ? cachedRoll(next) : null;
    fullScaleKmh = Math.max(next?.derived?.topSpeedKmh ?? 0, 30);
    speedScale.textContent = `0 – ${fullScaleKmh.toFixed(0)} km/h`;
    litSegments = -1;
  }

  function update(): void {
    select(selectedRun(app));
    const ready = Boolean(run && roll);
    panel.hidden = !ready;
    stateLine.hidden = !ready;
    if (!ready) {
      hint.hidden = false;
      hint.textContent = run
        ? "This run has neither an input stream nor car samples to display."
        : "Select a run to see its live inputs.";
      return;
    }
    hint.hidden = Boolean(run?.samples);
    hint.textContent = run?.samples
      ? ""
      : "This run has inputs but no vehicle samples; speed and gear stay unavailable.";
    const time = app.clock.get().timeMs;
    const current = rollAt(roll!, time);

    arrows.up.set(current.gas ? 1 : 0);
    arrows.down.set(current.brake ? 1 : 0);
    const steer = current.inRange ? current.steer / STEER_FULL : 0;
    arrows.left.set(Math.max(0, -steer));
    arrows.right.set(Math.max(0, steer));
    const steerPercent = current.inRange ? current.steer : 0;
    steerValue.textContent = fmtSteerPercent(steerPercent);
    steerValue.classList.toggle("on", steerPercent !== 0);

    const sample = samplesAt(run!, time);
    const kmh = sample.speedKmh ?? 0;
    const lit = Math.round(Math.min(1, kmh / fullScaleKmh) * SEGMENTS);
    if (lit !== litSegments) {
      litSegments = lit;
      segments.forEach((segment, index) => segment.classList.toggle("on", index < lit));
    }
    speedValue.textContent = kmh.toFixed(0);
    gearValue.textContent = sample.gear === null ? "–" : String(Math.round(sample.gear));

    stateLine.textContent = current.inRange
      ? `${fmtTime(time)} · steer ${fmtSteerPercent(current.steer)} · accel ${current.gas ? 1 : 0} · brake ${current.brake ? 1 : 0} · ${roll!.fromInputs ? "100 Hz" : "20 Hz"}`
      : `${fmtTime(time)} · before the input stream starts`;
    const respawns = run!.derived?.respawnTimes ?? [];
    root.classList.toggle("flash", recentRespawn(respawns, time));
  }

  app.state.subscribe(update);
  app.clock.subscribe(update);
  update();
}
