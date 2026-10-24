//! uPlot wrappers: a time axis in seconds, a playback playhead, click to seek.
//!
//! Charts are rebuilt when the set of series changes (rare) and only redrawn
//! when the playhead moves (every frame during playback).

// Must come before uplot: it formats legend numbers with `navigator.language`
// at import time and throws when the browser reports an unusable locale tag.
import "./locale-guard";

import uPlot from "uplot";
import type { AlignedData, Axis, Options, Plugin } from "uplot";
import "uplot/dist/uPlot.min.css";

export interface PlotSeries {
  label: string;
  color: string;
  values: (number | null)[];
  width?: number;
  /** Draw only points, for short comparison runs. */
  pointsOnly?: boolean;
}

export interface PlotMarker {
  timeMs: number;
  label?: string;
  kind?: "checkpoint" | "finish";
}

export interface PlotData {
  /** Time grid in seconds. */
  xs: number[];
  series: PlotSeries[];
  /** Vertical guide lines for the selected run. */
  markers?: PlotMarker[];
}

export interface ChartSpec {
  height?: number;
  yLabel: string;
  /** Value at or below which a zero line is drawn (delta charts). */
  zeroLine?: boolean;
  /** uPlot's own legend duplicates the compact legend in the panel. */
  showLegend?: boolean;
  getTime: () => number;
  onSeek?: (ms: number) => void;
}

const AXIS_STROKE = "#8fa3b8";
const GRID_STROKE = "#1b2531";
const TICK_STROKE = "#22303d";

function timeAxis(): Axis {
  return {
    stroke: AXIS_STROKE,
    grid: { stroke: GRID_STROKE, width: 1 },
    ticks: { stroke: TICK_STROKE, width: 1 },
    values: (_u, splits) =>
      splits.map((seconds) => {
        const ms = seconds * 1000;
        const total = Math.round(ms);
        const m = Math.floor(total / 60_000);
        const s = Math.floor((total % 60_000) / 1000);
        const frac = total % 1000;
        if (m === 0) return frac === 0 ? `${s}s` : `${s}.${String(Math.round(frac / 100))}`;
        return `${m}:${String(s).padStart(2, "0")}`;
      }),
  };
}

export class TimeChart {
  private plot: uPlot | null = null;
  private readonly spec: ChartSpec;
  private readonly host: HTMLElement;
  private readonly resizeObserver: ResizeObserver;
  private timeMs = 0;
  private markers: PlotMarker[] = [];
  /** True once the chart has been drawn into a host that had a real width. */
  private sized = false;

  constructor(host: HTMLElement, spec: ChartSpec) {
    this.host = host;
    this.spec = spec;
    this.resizeObserver = new ResizeObserver(() => this.resize());
    this.resizeObserver.observe(host);
  }

  render(data: PlotData): void {
    const width = this.host.clientWidth || 640;
    const height = this.spec.height ?? 160;
    this.markers = data.markers ?? [];
    const series: uPlot.Series[] = [{ value: (_u, v) => (v === null || v === undefined ? "–" : `${v.toFixed(2)}s`) }];
    for (const s of data.series) {
      series.push({
        label: s.label,
        stroke: s.color,
        width: s.width ?? 1.5,
        points: { show: Boolean(s.pointsOnly), size: 6 },
        value: (_u, v) => (v === null || v === undefined ? "–" : v.toFixed(1)),
      });
    }
    const yAxis: Axis = {
      stroke: AXIS_STROKE,
      grid: { stroke: GRID_STROKE, width: 1 },
      ticks: { stroke: TICK_STROKE, width: 1 },
      label: this.spec.yLabel,
      labelSize: 22,
      labelFont: "12px system-ui",
      font: "11px system-ui",
    };
    const options: Options = {
      width,
      height,
      padding: [10, 10, 0, 0],
      cursor: { drag: { x: true, y: false, setScale: true }, points: { show: false } },
      legend: { show: this.spec.showLegend ?? true, live: true },
      scales: { x: { time: false } },
      axes: [timeAxis(), yAxis],
      series,
      plugins: [
        this.playheadPlugin(),
        this.markersPlugin(),
        ...(this.spec.zeroLine ? [zeroLinePlugin] : []),
      ],
    };
    const aligned: AlignedData = [data.xs, ...data.series.map((s) => s.values)] as unknown as AlignedData;
    this.plot?.destroy();
    this.plot = new uPlot(options, aligned, this.host);
    // A host inside a hidden tab has no width: uPlot then falls back to the
    // nominal 640 px and its scales never see a real x range. `resize()` picks
    // that up once the tab becomes visible and re-ranges the data.
    this.sized = this.host.clientWidth > 0;
    if (this.spec.onSeek) {
      const seek = this.spec.onSeek;
      this.plot.over.addEventListener("click", (event) => {
        const rect = this.plot?.over.getBoundingClientRect();
        if (!rect || !this.plot) return;
        const x = event.clientX - rect.left;
        if (x < 0 || x > rect.width) return;
        seek(Math.round(this.plot.posToVal(x, "x") * 1000));
      });
    }
  }

  setTime(ms: number): void {
    this.timeMs = ms;
    // Redrawing a chart whose tab is hidden would range its scales against a
    // zero-width plot; skip it and let `resize()` catch up on show.
    if (this.host.clientWidth > 0) this.plot?.redraw();
  }

  resize(): void {
    if (!this.plot) return;
    const width = this.host.clientWidth;
    if (width <= 0) return;
    const needsRescale = !this.sized;
    this.sized = true;
    this.plot.setSize({ width, height: this.plot.height });
    // `setSize` keeps the existing scales, so a chart that was first built
    // while hidden needs its x range computed again from the data.
    if (needsRescale && this.plot.data[0]?.length) this.plot.setData(this.plot.data);
  }

  destroy(): void {
    this.resizeObserver.disconnect();
    this.plot?.destroy();
    this.plot = null;
  }

  /** Checkpoint and finish guides for the selected run. */
  private markersPlugin(): Plugin {
    return {
      hooks: {
        draw: (u) => {
          const min = u.scales.x?.min;
          const max = u.scales.x?.max;
          if (min === undefined || max === undefined) return;
          const { ctx } = u;
          for (const marker of this.markers) {
            const seconds = marker.timeMs / 1000;
            if (!Number.isFinite(seconds) || seconds < min || seconds > max) continue;
            const x = Math.round(u.valToPos(seconds, "x", true));
            if (!Number.isFinite(x)) continue;
            const finish = marker.kind === "finish";
            ctx.save();
            ctx.strokeStyle = finish ? "rgba(249, 199, 79, 0.72)" : "rgba(143, 163, 184, 0.3)";
            ctx.lineWidth = finish ? 1.5 : 1;
            ctx.setLineDash(finish ? [5, 3] : [2, 4]);
            ctx.beginPath();
            ctx.moveTo(x + 0.5, u.bbox.top);
            ctx.lineTo(x + 0.5, u.bbox.top + u.bbox.height);
            ctx.stroke();
            if (finish) {
              ctx.setLineDash([]);
              ctx.fillStyle = "#f9c74f";
              ctx.beginPath();
              ctx.moveTo(x - 3, u.bbox.top);
              ctx.lineTo(x + 3, u.bbox.top);
              ctx.lineTo(x, u.bbox.top + 5);
              ctx.closePath();
              ctx.fill();
            }
            ctx.restore();
          }
        },
      },
    };
  }

  /** Vertical playback line, clamped to the visible x range. */
  private playheadPlugin(): Plugin {
    return {
      hooks: {
        draw: (u) => {
          const seconds = this.timeMs / 1000;
          const min = u.scales.x?.min;
          const max = u.scales.x?.max;
          if (min === undefined || max === undefined || seconds < min || seconds > max) return;
          const x = Math.round(u.valToPos(seconds, "x", true));
          if (!Number.isFinite(x)) return;
          const { ctx } = u;
          ctx.save();
          ctx.strokeStyle = "rgba(219, 228, 238, 0.55)";
          ctx.lineWidth = 1;
          ctx.beginPath();
          ctx.moveTo(x + 0.5, u.bbox.top);
          ctx.lineTo(x + 0.5, u.bbox.top + u.bbox.height);
          ctx.stroke();
          ctx.restore();
        },
      },
    };
  }
}

const zeroLinePlugin: Plugin = {
  hooks: {
    draw: (u) => {
      const y = Math.round(u.valToPos(0, "y", true));
      if (!Number.isFinite(y)) return;
      const { ctx } = u;
      ctx.save();
      ctx.strokeStyle = "rgba(143, 163, 184, 0.6)";
      ctx.beginPath();
      ctx.moveTo(u.bbox.left, y + 0.5);
      ctx.lineTo(u.bbox.left + u.bbox.width, y + 0.5);
      ctx.stroke();
      ctx.restore();
    },
  },
};
