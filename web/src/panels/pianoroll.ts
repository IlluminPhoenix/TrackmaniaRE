//! Input piano roll: steering waveform, pedal bars, respawn and wall-hit
//! markers, all on one time axis with a playhead. Wheel zooms, shift+drag
//! pans, click or drag scrubs.

import { selectedRun, type App } from "../app";
import { cachedRoll, lastCheckpointIsFinish, rollAt, rollKey, sampleIndexAt, type RollData } from "../lib/derive";
import { clear, h } from "../lib/dom";
import { fmtSteerPercent, fmtTime } from "../lib/format";
import { icon } from "../lib/icons";

const STEER_LEFT = "#4cc9f0";
const STEER_RIGHT = "#f72585";
const GAS_COLOR = "#43aa8b";
const BRAKE_COLOR = "#ef476f";
const RESPAWN_COLOR = "rgba(239, 71, 111, 0.55)";
const WALL_COLOR = "#f8961e";
const CHECKPOINT_COLOR = "rgba(143, 163, 184, 0.35)";
const FINISH_COLOR = "#f9c74f";

export function mountInputs(root: HTMLElement, app: App): void {
  const canvas = h("canvas", {
    class: "pianoroll",
    tabindex: "0",
    role: "application",
    "data-local-shortcuts": "true",
    "aria-label": "Input piano roll. Click or drag to seek. Arrow keys seek, plus and minus zoom, zero fits the view.",
  }) as HTMLCanvasElement;
  const readout = h("div", { class: "row small muted roll-readout" });
  const legend = h("div", { class: "legend roll-legend" });
  const fitButton = h(
    "button",
    { class: "btn btn-ghost", type: "button", title: "Fit the whole input stream", onclick: () => fitView() },
    icon("fit", 14),
    h("span", { text: "Fit" }),
  );
  const zoomOut = h("button", {
    class: "btn btn-ghost roll-zoom",
    type: "button",
    text: "−",
    title: "Zoom out",
    "aria-label": "Zoom out",
    onclick: () => zoomView(1.4),
  });
  const zoomIn = h("button", {
    class: "btn btn-ghost roll-zoom",
    type: "button",
    text: "+",
    title: "Zoom in",
    "aria-label": "Zoom in",
    onclick: () => zoomView(0.7),
  });
  const toolbar = h(
    "div",
    { class: "roll-toolbar" },
    h("div", { class: "roll-actions" }, fitButton, zoomOut, zoomIn),
    h(
      "span",
      { class: "roll-hint" },
      h("span", {
        class: "fine-pointer",
        text: "Click or drag to seek · wheel to zoom · Shift-drag to pan · double-click to fit",
      }),
      h("span", { class: "coarse-pointer", text: "Drag to seek · use Fit or ± to zoom" }),
    ),
  );
  root.append(toolbar, canvas, readout, legend);

  const ctx = canvas.getContext("2d");
  let view = { start: 0, end: 1000 };
  let data: RollData | null = null;
  let runKey = "";
  let cache: HTMLCanvasElement | null = null;
  let cacheKey = "";
  let dragging: "seek" | "pan" | null = null;
  let dragStartX = 0;
  let dragStartView = { start: 0, end: 0 };

  function currentData(): RollData | null {
    const run = selectedRun(app);
    if (!run) return null;
    const key = rollKey(run);
    if (key !== runKey) {
      runKey = key;
      data = cachedRoll(run);
      cache = null;
      cacheKey = "";
      // TM2020 records inputs during the pre-start hold, so the stream starts
      // before the timer does and the view begins at its first tick.
      view = { start: minTime(), end: Math.max(run.durationMs, 1000) };
    }
    return data;
  }

  function width(): number {
    return canvas.clientWidth || 800;
  }

  function height(): number {
    return canvas.clientHeight || 150;
  }

  function timeToX(t: number): number {
    const span = Math.max(view.end - view.start, 1);
    return ((t - view.start) / span) * width();
  }

  function xToTime(x: number): number {
    const span = Math.max(view.end - view.start, 1);
    return view.start + (x / width()) * span;
  }

  /** Earliest time the view may show: the start of the recorded stream. */
  function minTime(): number {
    return Math.min(0, data?.times[0] ?? 0);
  }

  function indexAt(t: number): number {
    return data ? sampleIndexAt(data.times, t) : -1;
  }

  function niceStep(spanMs: number, cssWidth: number): number {
    const candidates = [100, 250, 500, 1000, 2000, 5000, 10_000, 15_000, 30_000, 60_000, 120_000, 300_000, 600_000];
    const target = spanMs / Math.max(4, Math.floor(cssWidth / 110));
    for (const c of candidates) if (c >= target) return c;
    return candidates[candidates.length - 1];
  }

  /**
   * Draws the whole roll in device pixels, one column per device pixel.
   *
   * Drawing on the device grid instead of CSS pixels scaled by the context is
   * what keeps the roll sharp: on a 150% display a 1 CSS pixel bar covers 1.5
   * device pixels, so the rasteriser softens its edges and the gap between
   * columns turns into a moiré of stripes. Device columns are exactly one pixel
   * wide, snap to the grid, and tile the axis without gaps.
   */
  function drawWaveform(target: CanvasRenderingContext2D, w: number, hh: number, dpr: number): void {
    const dataNow = data;
    const hairline = Math.max(1, Math.round(dpr));
    const steerCenter = Math.round(hh * 0.29);
    const steerHalf = Math.round(hh * 0.23);
    const gasTop = Math.round(hh * 0.58);
    const gasHeight = Math.round(hh * 0.14);
    const brakeTop = gasTop + gasHeight + hairline;
    const span = Math.max(view.end - view.start, 1);
    const xFor = (t: number): number => ((t - view.start) / span) * w;
    const tFor = (x: number): number => view.start + (x / w) * span;

    target.clearRect(0, 0, w, hh);
    target.font = `${Math.round(11 * dpr)}px system-ui`;

    // Checkpoints and the finish line are the first guides behind the data.
    const run = selectedRun(app);
    if (run) {
      const finishIsLast = lastCheckpointIsFinish(run);
      const drawn = new Set<number>();
      const guide = (timeMs: number, finish: boolean): void => {
        if (drawn.has(timeMs)) return;
        drawn.add(timeMs);
        const x = Math.round(xFor(timeMs));
        if (x < 0 || x > w) return;
        target.save();
        target.strokeStyle = finish ? FINISH_COLOR : CHECKPOINT_COLOR;
        target.lineWidth = Math.max(hairline, finish ? Math.round(1.5 * dpr) : hairline);
        target.setLineDash?.(finish ? [Math.round(5 * dpr), Math.round(3 * dpr)] : [Math.round(2 * dpr), Math.round(4 * dpr)]);
        target.beginPath();
        target.moveTo(x + 0.5, 0);
        target.lineTo(x + 0.5, hh);
        target.stroke();
        target.restore();
        if (finish) {
          target.save();
          target.fillStyle = FINISH_COLOR;
          target.font = `${Math.round(9 * dpr)}px system-ui`;
          target.textAlign = x > w - Math.round(52 * dpr) ? "right" : "left";
          target.fillText("FINISH", x + (target.textAlign === "right" ? -4 : 4) * dpr, Math.round(12 * dpr));
          target.restore();
        }
      };
      run.checkpoints.forEach((checkpoint, index) => {
        guide(checkpoint.timeMs, finishIsLast && index === run.checkpoints.length - 1);
      });
      if (run.raceTimeMs !== null) guide(run.raceTimeMs, true);
    }

    // Respawn and wall-hit markers behind everything.
    if (run?.derived) {
      target.fillStyle = RESPAWN_COLOR;
      for (const t of run.derived.respawnTimes) {
        const x = Math.round(xFor(t));
        if (x >= 0 && x <= w) target.fillRect(x, 0, hairline * 2, hh);
      }
      const half = Math.round(4 * dpr);
      const tall = Math.round(9 * dpr);
      target.fillStyle = WALL_COLOR;
      for (const hit of run.derived.wallHits) {
        const x = Math.round(xFor(hit.timeMs));
        if (x < 0 || x > w) continue;
        target.beginPath();
        target.moveTo(x - half, 0);
        target.lineTo(x + half, 0);
        target.lineTo(x, tall);
        target.closePath();
        target.fill();
      }
    }

    target.fillStyle = "#22303d";
    target.fillRect(0, steerCenter, w, hairline);

    // The pre-start hold: TM2020 already records inputs before the timer runs.
    if (view.start < 0) {
      const zero = Math.max(0, Math.min(w, Math.round(xFor(0))));
      target.fillStyle = "rgba(143, 163, 184, 0.07)";
      target.fillRect(0, 0, zero, hh);
      target.fillStyle = "#3b4c5e";
      target.fillRect(Math.max(0, zero - hairline), 0, hairline, hh);
    }

    if (!dataNow || dataNow.times.length === 0) {
      target.fillStyle = "#8fa3b8";
      target.fillText(
        "No steering data in this run.",
        Math.round(10 * dpr),
        steerCenter + Math.round(4 * dpr),
      );
    } else {
      const last = dataNow.times.length - 1;
      const scale = steerHalf / 127;
      for (let x = 0; x < w; x++) {
        let i0 = indexAt(tFor(x));
        if (i0 < 0) i0 = last + 1; // column is before the stream starts
        let i1 = indexAt(tFor(x + 1)) + 1;
        if (i1 > last) i1 = last;
        if (i0 > last || i1 < 0 || i0 > i1) continue;
        let low = 0;
        let high = 0;
        let gas = 0;
        let brake = 0;
        for (let i = i0; i <= i1; i++) {
          const v = dataNow.steer[i];
          if (v < low) low = v;
          if (v > high) high = v;
          if (dataNow.gas[i] >= 0.5) gas = 1;
          if (dataNow.brake[i] >= 0.5) brake = 1;
        }
        if (high > 0) {
          const bar = Math.min(steerCenter, Math.max(hairline, Math.round(high * scale)));
          target.fillStyle = STEER_RIGHT;
          target.fillRect(x, steerCenter - bar, 1, bar);
        }
        if (low < 0) {
          const bar = Math.max(hairline, Math.round(-low * scale));
          target.fillStyle = STEER_LEFT;
          target.fillRect(x, steerCenter, 1, Math.min(hh - steerCenter, bar));
        }
        if (gas) {
          target.fillStyle = GAS_COLOR;
          target.fillRect(x, gasTop, 1, gasHeight);
        }
        if (brake) {
          target.fillStyle = BRAKE_COLOR;
          target.fillRect(x, brakeTop, 1, gasHeight);
        }
      }
    }

    // Ruler: ticks in the lower strip, labels above the bottom edge.
    const stepMs = niceStep(span, w / dpr);
    const tickTop = hh - Math.round(11 * dpr);
    for (let t = Math.ceil(view.start / stepMs) * stepMs; t <= view.end; t += stepMs) {
      const x = Math.round(xFor(t));
      if (x < 0 || x > w) continue;
      target.fillStyle = "#1b2531";
      target.fillRect(x, tickTop, hairline, hh - tickTop);
      target.fillStyle = "#6b7f94";
      target.fillText(fmtTime(t), x + Math.round(4 * dpr), hh - Math.round(4 * dpr));
    }
  }

  function render(): void {
    if (!ctx) return;
    const dpr = window.devicePixelRatio || 1;
    const w = Math.max(1, Math.round(width() * dpr));
    const hh = Math.max(1, Math.round(height() * dpr));
    if (canvas.width !== w || canvas.height !== hh) {
      canvas.width = w;
      canvas.height = hh;
      cache = null;
      cacheKey = "";
    }
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    if (!data) {
      ctx.clearRect(0, 0, w, hh);
      ctx.fillStyle = "#8fa3b8";
      ctx.font = `${Math.round(12 * dpr)}px system-ui`;
      const run = selectedRun(app);
      ctx.fillText(
        run ? "This run has no input or vehicle samples." : "Select a run with an input stream.",
        Math.round(10 * dpr),
        Math.round(20 * dpr),
      );
      return;
    }
    const key = `${runKey}|${view.start.toFixed(1)}|${view.end.toFixed(1)}|${w}x${hh}`;
    if (!cache || key !== cacheKey) {
      cache = cache ?? document.createElement("canvas");
      cache.width = w;
      cache.height = hh;
      cacheKey = key;
      const cctx = cache.getContext("2d");
      if (cctx) drawWaveform(cctx, w, hh, dpr);
    }
    ctx.clearRect(0, 0, w, hh);
    ctx.drawImage(cache, 0, 0);
    const x = Math.round(timeToX(app.clock.get().timeMs) * dpr);
    if (x >= 0 && x <= w) {
      ctx.fillStyle = "rgba(219, 228, 238, 0.75)";
      ctx.fillRect(x, 0, Math.max(2, Math.round(dpr)), hh);
    }
  }

  function updateReadout(t: number): void {
    if (!data) {
      readout.textContent = "";
      return;
    }
    const state = rollAt(data, t);
    clear(readout);
    readout.append(
      h("span", { class: "mono", text: fmtTime(t) }),
      h("span", { text: `steer ${fmtSteerPercent(state.steer)}` }),
      h("span", { class: state.gas ? "" : "muted", text: state.gas ? "gas" : "gas off" }),
      h("span", { class: state.brake ? "" : "muted", text: state.brake ? "brake" : "brake off" }),
      h("span", { class: "muted", text: data.fromInputs ? "100 Hz input stream" : "20 Hz samples" }),
    );
  }

  canvas.addEventListener("pointerdown", (event) => {
    canvas.setPointerCapture(event.pointerId);
    if (event.shiftKey) {
      dragging = "pan";
      dragStartX = event.offsetX;
      dragStartView = { ...view };
    } else {
      dragging = "seek";
      app.actions.setPlaying(false);
      app.actions.seek(Math.round(xToTime(event.offsetX)));
    }
  });
  canvas.addEventListener("pointermove", (event) => {
    if (dragging === "seek") {
      app.actions.seek(Math.max(0, Math.round(xToTime(event.offsetX))));
    } else if (dragging === "pan") {
      const span = dragStartView.end - dragStartView.start;
      const shift = ((event.offsetX - dragStartX) / width()) * span;
      view = { start: dragStartView.start - shift, end: dragStartView.end - shift };
      clampView();
      render();
    }
    updateReadout(Math.max(0, xToTime(event.offsetX)));
  });
  canvas.addEventListener("pointerup", () => (dragging = null));
  canvas.addEventListener("pointerleave", () => (dragging = null));
  canvas.addEventListener("wheel", (event) => {
    event.preventDefault();
    const focus = xToTime(event.offsetX);
    const factor = Math.exp(event.deltaY / 400);
    const span = Math.max(200, view.end - view.start);
    const next = Math.max(200, Math.min(span * factor, totalSpan() - minTime()));
    const alpha = (focus - view.start) / span;
    view = { start: focus - alpha * next, end: focus + (1 - alpha) * next };
    clampView();
    render();
  });
  canvas.addEventListener("dblclick", () => fitView());
  canvas.addEventListener("keydown", (event) => {
    const keys = ["ArrowLeft", "ArrowRight", "Home", "End", "+", "=", "-", "_", "0"];
    if (!keys.includes(event.key)) return;
    event.preventDefault();
    event.stopPropagation();
    if (event.key === "0") {
      fitView();
      return;
    }
    if (event.key === "+" || event.key === "=") {
      zoomView(0.7);
      return;
    }
    if (event.key === "-" || event.key === "_") {
      zoomView(1.4);
      return;
    }
    if (event.key === "Home" || event.key === "End") {
      app.actions.setPlaying(false);
      app.actions.seek(event.key === "Home" ? minTime() : totalSpan());
      return;
    }
    app.actions.setPlaying(false);
    app.actions.nudge(event.shiftKey ? (event.key === "ArrowLeft" ? -1000 : 1000) : (event.key === "ArrowLeft" ? -100 : 100));
  });

  function zoomView(factor: number): void {
    if (!data) return;
    const span = Math.max(200, view.end - view.start);
    const focus = Math.max(view.start, Math.min(view.end, app.clock.get().timeMs));
    const next = Math.max(200, Math.min(span * factor, totalSpan() - minTime()));
    const alpha = (focus - view.start) / span;
    view = { start: focus - alpha * next, end: focus + (1 - alpha) * next };
    clampView();
    render();
  }

  function fitView(): void {
    if (!data) return;
    view = { start: minTime(), end: totalSpan() };
    render();
  }

  function totalSpan(): number {
    const run = selectedRun(app);
    return Math.max(run?.durationMs ?? 1000, 1000);
  }

  function clampView(): void {
    const total = totalSpan();
    const min = minTime();
    const span = Math.min(view.end - view.start, total - min);
    let start = view.start;
    if (start + span > total) start = total - span;
    if (start < min) start = min;
    view = { start, end: start + span };
  }

  function renderLegend(): void {
    clear(legend);
    legend.append(
      h("span", { class: "key" }, h("span", { class: "dot", style: `background:${STEER_RIGHT}` }), "steer right"),
      h("span", { class: "key" }, h("span", { class: "dot", style: `background:${STEER_LEFT}` }), "steer left"),
      h("span", { class: "key" }, h("span", { class: "dot", style: `background:${GAS_COLOR}` }), "gas"),
      h("span", { class: "key" }, h("span", { class: "dot", style: `background:${BRAKE_COLOR}` }), "brake"),
      h("span", { class: "key" }, h("span", { class: "dot", style: `background:${RESPAWN_COLOR}` }), "respawn"),
      h("span", { class: "key" }, h("span", { class: "dot", style: `background:${WALL_COLOR}` }), "wall hit (guess)"),
      h("span", { class: "key" }, h("span", { class: "dot", style: `background:${CHECKPOINT_COLOR}` }), "checkpoint"),
      h("span", { class: "key" }, h("span", { class: "dot", style: `background:${FINISH_COLOR}` }), "finish"),
    );
  }

  app.state.subscribe(() => {
    currentData();
    render();
  });
  app.clock.subscribe((clock) => {
    render();
    updateReadout(clock.timeMs);
  });
  new ResizeObserver(() => render()).observe(canvas);
  currentData();
  renderLegend();
  render();
}
