// @vitest-environment jsdom
//
// Boots the real application against the real `index.html` shell, with the wasm
// parser doing the actual parsing of the fixture replays. `lib/chart` is mocked
// because uPlot needs a canvas 2D implementation that jsdom does not have; the
// chart wrapper itself holds no logic worth testing here.

import { readFileSync } from "node:fs";
import { join } from "node:path";
import { beforeAll, describe, expect, it, vi } from "vitest";

vi.mock("../src/lib/chart", () => {
  const renders: { xs: number[]; labels: string[] }[] = [];
  class TimeChart {
    render(data: { xs: number[]; series: { label: string }[] }): void {
      renders.push({ xs: data.xs, labels: data.series.map((series) => series.label) });
    }
    setTime(): void {}
    resize(): void {}
    destroy(): void {}
  }
  return { TimeChart, __renders: renders };
});

type AppModule = typeof import("../src/main");

// vitest runs with the `web/` directory as its root.
const here = process.cwd();
const fixtures = join(here, "..", "crates/trackmaniare/tests/fixtures");

let app: AppModule["app"];

function context2d(): CanvasRenderingContext2D {
  const noop = (): void => {};
  return {
    canvas: null,
    fillStyle: "",
    strokeStyle: "",
    lineWidth: 1,
    lineCap: "butt",
    font: "",
    textAlign: "left",
    globalAlpha: 1,
    setTransform: noop,
    setLineDash: noop,
    clearRect: noop,
    fillRect: noop,
    strokeRect: noop,
    beginPath: noop,
    closePath: noop,
    moveTo: noop,
    lineTo: noop,
    arc: noop,
    stroke: noop,
    fill: noop,
    fillText: noop,
    drawImage: noop,
    save: noop,
    restore: noop,
  } as unknown as CanvasRenderingContext2D;
}

beforeAll(async () => {
  const html = readFileSync(join(here, "index.html"), "utf8");
  document.documentElement.innerHTML = html.replace(/<script[\s\S]*?<\/script>/g, "");

  class ResizeObserverStub {
    observe(): void {}
    unobserve(): void {}
    disconnect(): void {}
  }
  (globalThis as Record<string, unknown>).ResizeObserver = ResizeObserverStub;
  // The playback loop must not run: frames never fire in this test.
  (globalThis as Record<string, unknown>).requestAnimationFrame = () => 1;
  (globalThis as Record<string, unknown>).cancelAnimationFrame = () => {};

  HTMLCanvasElement.prototype.getContext = function (id: string): unknown {
    if (id === "2d") return context2d();
    return null; // WebGL: the 3D panel must degrade instead of throwing
  } as typeof HTMLCanvasElement.prototype.getContext;

  const { initWasm } = await import("../src/lib/parse");
  const wasm = new Uint8Array(readFileSync(join(here, "src/lib/wasm/trackmaniare_wasm_bg.wasm")));
  await initWasm(wasm);

  ({ app } = await import("../src/main"));
});

function text(id: string): string {
  return document.getElementById(id)?.textContent ?? "";
}

function fixture(name: string): Uint8Array {
  return new Uint8Array(readFileSync(join(fixtures, name)));
}

describe("application shell", () => {
  it("starts with an empty workspace instead of a fake run", async () => {
    expect(document.getElementById("status")?.hidden).toBe(true);
    expect(text("status")).toBe("");
    expect(document.getElementById("open-files")?.getAttribute("aria-label")).toBe("Open files");
    expect(document.getElementById("open-files")?.querySelector(".btn-label")?.textContent).toBe("Open files");
    expect(document.getElementById("workspace-empty")?.hidden).toBe(false);
    expect(document.getElementById("workspace-panels")?.hidden).toBe(true);
    expect(document.querySelector<HTMLElement>(".tabs")?.hidden).toBe(true);
    expect(text("panel-runs")).toContain("No runs loaded");
    expect(text("panel-overview")).toContain("Select a run");
    expect(text("panel-tas")).toContain("Select a run");
    expect(text("panel-splits")).toContain("Select a run");
    expect(text("panel-speed")).toContain("Select a run");
    expect(text("panel-live")).toContain("Select a run");
    await vi.waitFor(() => expect(text("panel-runs")).toContain("Nothing stored yet"));
  });

  it("auto-selects the run with samples and a finish, then exposes the input-only ghost", async () => {
    await app.actions.loadBytes("tm2020-merged-record.Replay.Gbx", fixture("tm2020-merged-record.Replay.Gbx"), {
      persist: false,
    });
    const state = app.state.get();
    expect(state.runs).toHaveLength(2);
    const selected = state.runs.find((run) => run.id === state.selectedId);
    // The replay-level record has samples; the first ghost is input-only. Picking
    // the record is the whole point of auto-selection.
    expect(selected?.samples).toBeTruthy();
    expect(selected?.raceTimeMs).not.toBeNull();
    expect(state.selectedId).not.toBe(state.runs[0].id);
    expect(text("status")).toContain("Loaded tm2020-merged-record.Replay.Gbx");
    expect(document.getElementById("workspace-empty")?.hidden).toBe(true);
    expect(document.getElementById("workspace-panels")?.hidden).toBe(false);
    expect(document.querySelector<HTMLElement>(".tabs")?.hidden).toBe(false);

    // Runs list: one file group, both entries, exactly one selected.
    const list = document.getElementById("panel-runs");
    expect(list?.querySelectorAll(".file-group")).toHaveLength(1);
    expect(list?.querySelectorAll(".run-row")).toHaveLength(2);
    expect(list?.querySelectorAll(".run-row.selected")).toHaveLength(1);
    expect(text("panel-runs")).toContain("391 samples");
    expect(text("panel-runs")).toContain("no inputs");
    expect(text("panel-runs")).toContain("no samples");
    expect(text("panel-runs")).toContain("· record");
    expect(text("panel-runs")).not.toContain("ghost 2");

    // The selected record can drive every sampled panel.
    expect(text("panel-overview")).toContain("391");
    expect(document.querySelector("#panel-overview .metric-grid")).not.toBeNull();
    expect(text("panel-tas")).toContain("no input stream");
    const groups = [...document.querySelectorAll("#panel-overview .fact-group h3")].map((el) => el.textContent);
    expect(groups).toEqual(["Run", "Motion", "Input", "Map & file"]);
    expect(document.querySelector("#panel-overview .run-name .swatch")?.getAttribute("title")).toContain("Run colour");
    expect(text("panel-transport")).toContain("recorded");
    expect(document.querySelector("#panel-transport .transport-sub")?.textContent).toBe(
      "0:07.603 finish · 391 samples",
    );

    // Selecting the input-only ghost keeps all its features available.
    const ghost = state.runs.find((run) => run.inputs?.ticks);
    expect(ghost).toBeDefined();
    app.actions.select(ghost!.id);
    expect(app.state.get().selectedId).toBe(ghost!.id);
    expect(app.state.get().compareIds).toEqual([]);
    expect(text("panel-overview")).toContain("1,955 ticks");
    expect(text("panel-overview")).toContain("TAS export");
    const tas = document.querySelector<HTMLTextAreaElement>("#panel-tas textarea");
    expect(tas?.value.split("\n").length).toBeGreaterThan(100);
    expect(tas?.value).toMatch(/^\d+ steer -?\d+$/m);

    // Transport stays live for a ghost with inputs but no vehicle samples.
    expect(text("panel-transport")).toContain("only the input stream is available");
    expect(document.querySelector("#panel-transport .transport-sub")?.textContent).toBe(
      "0:07.603 finish · 1,955 ticks",
    );

    // The live display runs off the input stream alone.
    expect(text("panel-live")).toContain("steer 0% · accel 0 · brake 0 · 100 Hz");
    expect(document.querySelector("#panel-live .live-arrow.up rect")?.getAttribute("height")).toBe("0");
    expect(document.querySelector("#panel-live .steer-value")?.textContent).toBe("0%");
    expect(document.querySelector("#panel-live .gear-value")?.textContent).toBe("–");
    expect(document.querySelector("#panel-live .speed-value")?.textContent).toBe("0");

    app.actions.seek(600);
    expect(text("panel-transport")).toContain("0:00.600");
    expect(text("panel-live")).toContain("0:00.600 · steer +63% · accel 1 · brake 0 · 100 Hz");
    expect(document.querySelector("#panel-live .steer-value")?.textContent).toBe("+63%");

    // The 3D panel has no WebGL here and must say so instead of throwing.
    await vi.waitFor(() => expect(text("panel-path")).toContain("could not start"));
  });

  it("compares multiple runs, swaps the reference and keeps the old selection", async () => {
    await app.actions.loadBytes("tm2020-short.Replay.Gbx", fixture("tm2020-short.Replay.Gbx"), { persist: false });
    const state = app.state.get();
    expect(state.runs).toHaveLength(3);
    const short = state.runs[2];
    const record = state.runs[1];
    const ghost = state.runs[0];
    expect(short.sampleCount).toBe(112);
    expect(state.selectedId).toBe(short.id);

    app.actions.toggleCompare(record.id);
    expect(app.state.get().compareIds).toEqual([record.id]);
    expect(text("panel-speed")).toContain("Delta against TestDriver0001 · record");
    expect(document.querySelector("#panel-speed .chart-legend")?.textContent).toContain("(reference)");
    expect(document.querySelector("#compare-bar")?.textContent).toContain("TestDriver0001 · record");
    expect(text("panel-splits")).toContain("Finish");
    expect(document.querySelector("#panel-splits .splits-scroll table.data")).not.toBeNull();
    expect(document.getElementById("compare-bar")?.hidden).toBe(false);

    // A second comparison is supported; the radio group makes the reference explicit.
    app.actions.toggleCompare(ghost.id);
    expect(app.state.get().compareIds).toEqual([record.id, ghost.id]);
    let radios = [...document.querySelectorAll<HTMLInputElement>("#compare-bar input[type=radio]")];
    expect(radios).toHaveLength(2);
    expect(radios[0].checked).toBe(true);
    radios[1].click();
    expect(app.state.get().compareIds[0]).toBe(ghost.id);
    expect(text("panel-speed")).toContain(`Delta against ${ghost.nickname}`);
    radios = [...document.querySelectorAll<HTMLInputElement>("#compare-bar input[type=radio]")];
    expect(radios[0].checked).toBe(true);

    // Make the sampled record the reference again, then remove the other overlay.
    radios[1].click();
    expect(app.state.get().compareIds[0]).toBe(record.id);
    const remove = document.querySelectorAll<HTMLButtonElement>("#compare-bar .compare-remove");
    remove[1].click();
    expect(app.state.get().compareIds).toEqual([record.id]);

    const chart = (await import("../src/lib/chart")) as unknown as {
      __renders: { xs: number[]; labels: string[] }[];
    };
    expect(chart.__renders.some((entry) => entry.labels.length === 2)).toBe(true);
    expect(chart.__renders.some((entry) => entry.labels.some((label) => label.startsWith("Δ vs")))).toBe(true);

    // Selecting a comparison run puts the previous selection into its slot.
    app.actions.select(record.id);
    expect(app.state.get().selectedId).toBe(record.id);
    expect(app.state.get().compareIds).toEqual([short.id]);
    app.actions.select(short.id);
    expect(app.state.get().selectedId).toBe(short.id);
    expect(app.state.get().compareIds).toEqual([record.id]);

    document.querySelector<HTMLButtonElement>("#compare-bar .compare-clear")?.click();
    expect(app.state.get().compareIds).toEqual([]);
    expect(document.getElementById("compare-bar")?.hidden).toBe(true);

    // Live display: arrows follow the input stream, gear and speed follow samples.
    app.actions.seek(0);
    expect(document.querySelector("#panel-live .live-arrow.up rect")?.getAttribute("height")).toBe("24");
    expect(document.querySelector("#panel-live .live-arrow.down rect")?.getAttribute("height")).toBe("0");
    expect(document.querySelector("#panel-live .live-arrow.left rect")?.getAttribute("width")).toBe("0");
    expect(document.querySelector("#panel-live .gear-value")?.textContent).toBe("1");
    expect(document.querySelector("#panel-live .speed-value")?.textContent).toBe("1");
    expect(document.querySelectorAll("#panel-live .speed-bar .seg.on")).toHaveLength(0);

    app.actions.seek(1000);
    expect(text("panel-live")).toContain("57");
    expect(text("panel-live")).toContain("steer 0% · accel 1 · brake 0 · 100 Hz");
    expect(document.querySelectorAll("#panel-live .speed-bar .seg.on").length).toBeGreaterThan(0);
  });

  it("keeps focus on run and comparison controls when the lists re-render", async () => {
    const rowById = (id: string): HTMLElement | null =>
      [...document.querySelectorAll<HTMLElement>(".run-row")].find((row) => row.dataset.runId === id) ?? null;
    const rows = [...document.querySelectorAll<HTMLElement>(".run-row")];
    const ghost = rows.find((row) => row.textContent?.includes("1,955 ticks"));
    const record = rows.find((row) => row.textContent?.includes("TestDriver0001 · record"));
    const short = rows.find((row) => row.textContent?.includes("112 samples"));
    expect(ghost && record && short).toBeTruthy();
    const ghostId = ghost!.dataset.runId!;
    const recordId = record!.dataset.runId!;
    const shortId = short!.dataset.runId!;

    const ghostSelect = rowById(ghostId)?.querySelector<HTMLButtonElement>(".run-select");
    ghostSelect?.focus();
    ghostSelect?.click();
    await Promise.resolve();
    expect(document.activeElement).toBe(rowById(ghostId)?.querySelector(".run-select"));

    const recordBox = rowById(recordId)?.querySelector<HTMLInputElement>("input[type=checkbox]");
    recordBox?.focus();
    recordBox?.click();
    await Promise.resolve();
    const focusedRecordBox = rowById(recordId)?.querySelector<HTMLInputElement>("input[type=checkbox]");
    expect(focusedRecordBox?.checked).toBe(true);
    expect(document.activeElement).toBe(focusedRecordBox);

    const shortSelect = rowById(shortId)?.querySelector<HTMLButtonElement>(".run-select");
    shortSelect?.focus();
    shortSelect?.click();
    await Promise.resolve();

    const ghostBox = rowById(ghostId)?.querySelector<HTMLInputElement>("input[type=checkbox]");
    ghostBox?.focus();
    ghostBox?.click();
    await Promise.resolve();
    const radios = [...document.querySelectorAll<HTMLInputElement>("#compare-bar input[type=radio]")];
    expect(radios).toHaveLength(2);
    radios[1].focus();
    radios[1].click();
    await Promise.resolve();
    expect(app.state.get().compareIds[0]).toBe(ghostId);
    const focusedRadio = document.activeElement as HTMLInputElement;
    expect(focusedRadio.type).toBe("radio");
    expect(focusedRadio.value).toBe(ghostId);
    expect(focusedRadio.checked).toBe(true);

    app.actions.clearCompare();
  });

  it("drops a run and reports a broken file as a dismissible toast", async () => {
    const record = app.state.get().runs.find((run) => run.samples && run.fileName.includes("merged"));
    expect(record).toBeDefined();
    app.actions.removeRun(record!.id);
    expect(app.state.get().runs).toHaveLength(2);
    expect(app.state.get().compareIds).toEqual([]);

    const before = app.state.get().runs.length;
    await app.actions.loadBytes("broken.Replay.Gbx", new Uint8Array([1, 2, 3, 4, 5]));
    expect(app.state.get().error).toContain("Could not read broken.Replay.Gbx");
    expect(app.state.get().runs).toHaveLength(before);
    expect(text("status")).toContain("Could not read broken.Replay.Gbx");
    expect(document.querySelector("#status .toast-error")).not.toBeNull();

    document.querySelector<HTMLButtonElement>("#status .toast-close")?.click();
    expect(app.state.get().error).toBeNull();
    expect(document.getElementById("status")?.hidden).toBe(true);
  });

  it("switches between the three tab pages and remembers the new key", () => {
    const overview = document.getElementById("tab-overview");
    const three = document.getElementById("tab-3d");
    const inputs = document.getElementById("tab-inputs");
    const buttons = [...document.querySelectorAll<HTMLButtonElement>(".tab")];
    expect(buttons).toHaveLength(3);
    expect(buttons.map((button) => button.getAttribute("aria-controls"))).toEqual([
      "tab-overview",
      "tab-3d",
      "tab-inputs",
    ]);
    expect(buttons[0].getAttribute("aria-selected")).toBe("true");
    expect(overview?.hidden).toBe(false);
    expect(three?.hidden).toBe(true);
    expect(inputs?.hidden).toBe(true);

    // Runs and playback stay outside the tabs, on every tab page.
    expect(document.getElementById("panel-runs")?.closest(".tab-page")).toBeNull();
    expect(three?.contains(document.getElementById("panel-live"))).toBe(true);
    expect(document.getElementById("panel-transport")?.closest(".tab-page")).toBeNull();

    buttons[1].click();
    expect(three?.hidden).toBe(false);
    expect(overview?.hidden).toBe(true);
    expect(localStorage.getItem("trackmaniare.tab.v2")).toBe("tab-3d");

    buttons[2].click();
    expect(inputs?.hidden).toBe(false);
    expect(three?.hidden).toBe(true);
    expect(localStorage.getItem("trackmaniare.tab.v2")).toBe("tab-inputs");

    buttons[0].click();
    expect(overview?.hidden).toBe(false);
    expect(inputs?.hidden).toBe(true);
    expect(localStorage.getItem("trackmaniare.tab.v2")).toBe("tab-overview");

    const before = app.clock.get().timeMs;
    document.body.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true }));
    expect(app.clock.get().timeMs).toBeCloseTo(before + 100, 5);
  });

  it("opens and closes the runs drawer without leaving focus stranded", () => {
    const toggle = document.getElementById("runs-toggle") as HTMLButtonElement;
    const drawer = document.getElementById("runs-drawer")!;
    const backdrop = document.getElementById("drawer-backdrop")!;
    expect(drawer.classList.contains("open")).toBe(false);
    toggle.click();
    expect(drawer.classList.contains("open")).toBe(true);
    expect(toggle.getAttribute("aria-expanded")).toBe("true");
    expect(backdrop.hidden).toBe(false);
    document.getElementById("runs-close")?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(drawer.classList.contains("open")).toBe(false);
    expect(toggle.getAttribute("aria-expanded")).toBe("false");
    expect(backdrop.hidden).toBe(true);
    expect(document.activeElement).toBe(toggle);

    // Escape works from a focused run button, not just from the page body.
    toggle.click();
    const runButton = drawer.querySelector<HTMLButtonElement>(".run-select");
    expect(runButton).not.toBeNull();
    runButton?.focus();
    runButton?.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    expect(drawer.classList.contains("open")).toBe(false);
    expect(backdrop.hidden).toBe(true);
    expect(document.activeElement).toBe(toggle);
  });

  it("opens the shortcut list with the keyboard and closes it", () => {
    const dialog = document.getElementById("shortcuts-dialog")!;
    expect(dialog.hasAttribute("open")).toBe(false);
    document.body.dispatchEvent(new KeyboardEvent("keydown", { key: "?", bubbles: true }));
    expect(dialog.hasAttribute("open")).toBe(true);
    expect(text("shortcuts-dialog")).toContain("Open this shortcut list");
    document.getElementById("shortcuts-close")?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    expect(dialog.hasAttribute("open")).toBe(false);
  });
});
