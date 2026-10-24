//! Entry point: stores, actions, playback loop, shell behaviour, panel mounting.

import "./styles/index.css";
import {
  assignColors,
  bestRun,
  type Actions,
  type App,
  type AppState,
  type ClockState,
} from "./app";
import { clear, h } from "./lib/dom";
import { icon } from "./lib/icons";
import { initWasm, parseReplayBytes } from "./lib/parse";
import { saveRecent } from "./lib/persist";
import { createStore } from "./lib/store";
import type { RunData } from "./lib/types";
import { mountCompare } from "./panels/compare";
import { mountInputs } from "./panels/pianoroll";
import { mountLive } from "./panels/live";
import { mountOverview } from "./panels/overview";
import { mountPath3d } from "./panels/path3d";
import { mountRuns } from "./panels/runs";
import { mountSplits } from "./panels/splits";
import { mountSpeed } from "./panels/speed";
import { mountTas } from "./panels/tas";
import { mountTransport } from "./panels/transport";

const state = createStore<AppState>({
  runs: [],
  selectedId: null,
  compareIds: [],
  recentsVersion: 0,
  status: "",
  error: null,
});

const clock = createStore<ClockState>({ timeMs: 0, playing: false, rate: 1 });

function selectedDuration(): number {
  const { runs, selectedId } = state.get();
  const run = runs.find((r) => r.id === selectedId);
  return run ? Math.max(run.durationMs, 1) : 1;
}

const actions: Actions = {
  async loadFiles(files) {
    const list = [...files].filter((file) => /\.gbx$/i.test(file.name));
    if (!list.length) {
      state.set({ status: "", error: "No .Gbx files were selected." });
      return;
    }
    for (const file of list) {
      try {
        const bytes = new Uint8Array(await file.arrayBuffer());
        await actions.loadBytes(file.name, bytes, { persist: true });
      } catch (err) {
        state.set({ status: "", error: `Could not read ${file.name}: ${describe(err)}` });
      }
    }
  },

  async loadBytes(fileName, bytes, options) {
    state.set({ status: `Parsing ${fileName}…`, error: null });
    // Let the browser paint the status before the (synchronous) parse blocks.
    await new Promise((resolve) => setTimeout(resolve, 0));
    let runs: RunData[];
    try {
      await initWasm();
      runs = parseReplayBytes(bytes, fileName);
    } catch (err) {
      state.set({ status: "", error: `Could not read ${fileName}: ${describe(err)}` });
      return;
    }
    if (!runs.length) {
      state.set({ status: "", error: `${fileName} holds no ghosts.` });
      return;
    }
    const current = state.get();
    const kept = current.runs.filter((run) => !runs.some((fresh) => fresh.id === run.id));
    const colors = assignColors(kept, runs.length);
    const added = runs.map((run, index) => ({ ...run, color: colors[index] }));
    const selected = bestRun(added) ?? added[0];
    state.set({
      runs: [...kept, ...added],
      selectedId: selected.id,
      compareIds: current.compareIds.filter(
        (id) => !added.some((run) => run.id === id) && id !== selected.id && kept.some((run) => run.id === id),
      ),
      status: `Loaded ${fileName}${added.length > 1 ? ` · ${added.length} runs` : ""}`,
      error: null,
    });
    clock.set({ timeMs: 0, playing: false });
    window.dispatchEvent(new Event("trackmaniare:run-selected"));
    window.dispatchEvent(new Event("resize"));
    if (options?.persist !== false) {
      // The write finishes after this call; bump the version so the runs panel
      // picks the new entry up instead of showing the list from before it.
      void saveRecent(fileName, fileName, bytes.length, bytes, runs.length).then(() => {
        state.set({ recentsVersion: state.get().recentsVersion + 1 });
      });
    }
  },

  select(id) {
    const current = state.get();
    if (current.selectedId === id) {
      window.dispatchEvent(new Event("trackmaniare:run-selected"));
      return;
    }
    // Inspecting a comparison run should keep the old selection visible as a
    // comparison in the same slot. Otherwise clicking a chart legend would
    // silently drop the run you were comparing against.
    const wasComparing = current.compareIds.includes(id);
    let compareIds = current.compareIds.filter((entry) => entry !== id);
    if (wasComparing && current.selectedId) {
      const slot = current.compareIds.indexOf(id);
      compareIds = [...compareIds];
      compareIds.splice(Math.max(0, slot), 0, current.selectedId);
    }
    compareIds = compareIds.filter((entry) => entry !== id);
    state.set({ selectedId: id, compareIds });
    clock.set({ timeMs: 0, playing: false });
    window.dispatchEvent(new Event("trackmaniare:run-selected"));
  },

  toggleCompare(id) {
    const current = state.get();
    if (id === current.selectedId) return;
    if (!current.runs.some((run) => run.id === id)) return;
    const next = current.compareIds.includes(id)
      ? current.compareIds.filter((entry) => entry !== id)
      : [...current.compareIds, id];
    state.set({ compareIds: next });
  },

  clearCompare() {
    if (!state.get().compareIds.length) return;
    state.set({ compareIds: [] });
  },

  makeReference(id) {
    const current = state.get();
    if (id === current.selectedId || !current.compareIds.includes(id)) return;
    state.set({ compareIds: [id, ...current.compareIds.filter((entry) => entry !== id)] });
  },

  removeRun(id) {
    const current = state.get();
    if (!current.runs.some((run) => run.id === id)) return;
    const runs = current.runs.filter((run) => run.id !== id);
    const selectedId =
      current.selectedId === id ? (bestRun(runs)?.id ?? null) : current.selectedId;
    const compareIds = current.compareIds.filter(
      (entry) => entry !== id && entry !== selectedId && runs.some((run) => run.id === entry),
    );
    state.set({ runs, selectedId, compareIds });
    clock.set({ timeMs: 0, playing: false });
    window.dispatchEvent(new Event("resize"));
  },

  removeFile(fileName) {
    const current = state.get();
    const doomed = new Set(current.runs.filter((run) => run.fileName === fileName).map((run) => run.id));
    if (!doomed.size) return;
    const runs = current.runs.filter((run) => !doomed.has(run.id));
    const selectedId =
      current.selectedId && doomed.has(current.selectedId) ? (bestRun(runs)?.id ?? null) : current.selectedId;
    const compareIds = current.compareIds.filter(
      (entry) => !doomed.has(entry) && entry !== selectedId && runs.some((run) => run.id === entry),
    );
    state.set({
      runs,
      selectedId,
      compareIds,
      status: `Closed ${fileName}`,
      error: null,
    });
    clock.set({ timeMs: 0, playing: false });
    window.dispatchEvent(new Event("resize"));
  },

  seek(ms) {
    const clamped = Math.max(0, Math.min(ms, selectedDuration()));
    clock.set({ timeMs: clamped });
  },

  nudge(deltaMs) {
    clock.set({ playing: false, timeMs: Math.max(0, Math.min(clock.get().timeMs + deltaMs, selectedDuration())) });
  },

  setPlaying(playing) {
    if (playing && clock.get().timeMs >= selectedDuration() - 1) clock.set({ timeMs: 0 });
    clock.set({ playing });
  },

  togglePlay() {
    actions.setPlaying(!clock.get().playing);
  },

  setRate(rate) {
    const clamped = Math.max(0.1, Math.min(4, Number.isFinite(rate) ? rate : 1));
    clock.set({ rate: clamped });
  },

  setStatus(status, error = null) {
    state.set({ status, error });
  },
};

const app: App = { state, clock, actions };

export { app, actions, clock, state };

function mount(id: string, fn: (root: HTMLElement, app: App) => void): void {
  const root = document.getElementById(id);
  if (root) fn(root, app);
}

// --- Header, drawer, empty state ------------------------------------------

const fileInput = document.getElementById("file-input") as HTMLInputElement | null;
const workspaceEmpty = document.getElementById("workspace-empty");
const workspacePanels = document.getElementById("workspace-panels");
const tabsBar = document.querySelector<HTMLElement>(".tabs");
const runsCount = document.getElementById("runs-count");
const runsToggle = document.getElementById("runs-toggle");
const runsDrawer = document.getElementById("runs-drawer");
const drawerBackdrop = document.getElementById("drawer-backdrop");
const runsClose = document.getElementById("runs-close");
const shortcutsOpen = document.getElementById("shortcuts-open");
const shortcutsDialog = document.getElementById("shortcuts-dialog") as HTMLDialogElement | null;
const shortcutsClose = document.getElementById("shortcuts-close");

function openFilePicker(): void {
  fileInput?.click();
}

document.getElementById("open-files")?.addEventListener("click", openFilePicker);
document.getElementById("empty-open")?.addEventListener("click", openFilePicker);
fileInput?.addEventListener("change", () => {
  if (fileInput.files?.length) void actions.loadFiles(fileInput.files);
  fileInput.value = "";
});

function updateShell(current: AppState): void {
  const hasRun = Boolean(current.selectedId && current.runs.some((run) => run.id === current.selectedId));
  if (workspaceEmpty) workspaceEmpty.hidden = hasRun;
  if (workspacePanels) workspacePanels.hidden = !hasRun;
  if (tabsBar) tabsBar.hidden = !hasRun;
  if (runsCount) runsCount.textContent = String(current.runs.length);
  if (runsToggle) {
    runsToggle.setAttribute("aria-label", current.runs.length ? `Runs, ${current.runs.length} loaded` : "Runs");
  }
  if (!hasRun) document.getElementById("compare-bar")?.setAttribute("hidden", "");
}

function syncDrawerAccessibility(): void {
  if (!runsDrawer || !runsToggle) return;
  // The same element is a static rail above 900 px and a drawer below it. Only
  // a closed drawer has to leave the tab order; the static rail stays live.
  const drawerMode = runsToggle.offsetParent !== null;
  const hidden = drawerMode && !runsDrawer.classList.contains("open");
  if (hidden) {
    if (runsDrawer.contains(document.activeElement)) runsToggle.focus();
    runsDrawer.setAttribute("inert", "");
    runsDrawer.setAttribute("aria-hidden", "true");
  } else {
    runsDrawer.removeAttribute("inert");
    runsDrawer.removeAttribute("aria-hidden");
  }
}

function setDrawer(open: boolean, returnFocus = false): void {
  if (!runsDrawer || !drawerBackdrop || !runsToggle) return;
  // Move focus out before making a closing drawer inert.
  if (!open && returnFocus) runsToggle.focus();
  runsDrawer.classList.toggle("open", open);
  drawerBackdrop.hidden = !open;
  runsToggle.setAttribute("aria-expanded", String(open));
  document.body.classList.toggle("drawer-open", open);
  syncDrawerAccessibility();
  if (open) (runsClose ?? runsDrawer).focus();
}

syncDrawerAccessibility();
window.addEventListener("resize", syncDrawerAccessibility);

runsToggle?.addEventListener("click", () => {
  setDrawer(!runsDrawer?.classList.contains("open"));
});
runsClose?.addEventListener("click", () => setDrawer(false, true));
drawerBackdrop?.addEventListener("click", () => setDrawer(false, true));
window.addEventListener("trackmaniare:run-selected", () => {
  if (runsDrawer?.classList.contains("open")) setDrawer(false, true);
});

function openShortcuts(): void {
  if (!shortcutsDialog) return;
  if (shortcutsDialog.open) return;
  if (typeof shortcutsDialog.showModal === "function") {
    try {
      shortcutsDialog.showModal();
    } catch {
      shortcutsDialog.setAttribute("open", "");
    }
  } else {
    shortcutsDialog.setAttribute("open", "");
  }
}

function closeShortcuts(): void {
  if (!shortcutsDialog) return;
  if (typeof shortcutsDialog.close === "function" && shortcutsDialog.open) shortcutsDialog.close();
  else shortcutsDialog.removeAttribute("open");
}

shortcutsOpen?.addEventListener("click", openShortcuts);
shortcutsClose?.addEventListener("click", closeShortcuts);
shortcutsDialog?.addEventListener("click", (event) => {
  if (event.target === shortcutsDialog) closeShortcuts();
});

// --- Status toast ----------------------------------------------------------

const statusEl = document.getElementById("status");
let statusTimer: ReturnType<typeof setTimeout> | null = null;

function renderStatus(current: AppState): void {
  if (!statusEl) return;
  if (statusTimer) {
    clearTimeout(statusTimer);
    statusTimer = null;
  }
  clear(statusEl);
  const message = current.error ?? current.status;
  if (!message) {
    statusEl.hidden = true;
    return;
  }
  const isError = Boolean(current.error);
  const toast = h(
    "div",
    { class: `toast${isError ? " toast-error" : " toast-info"}`, role: isError ? "alert" : "status" },
    icon(isError ? "warning" : "info", 15),
    h("span", { class: "toast-message", text: message }),
    h(
      "button",
      {
        class: "btn btn-icon btn-ghost toast-close",
        type: "button",
        "aria-label": "Dismiss message",
        title: "Dismiss",
        onclick: () => actions.setStatus("", null),
      },
      icon("close", 14),
    ),
  );
  statusEl.hidden = false;
  statusEl.append(toast);
  if (!isError) {
    statusTimer = setTimeout(() => {
      toast.classList.add("toast-leaving");
      setTimeout(() => {
        if (statusEl.contains(toast)) toast.remove();
        statusEl.hidden = statusEl.childElementCount === 0;
      }, 160);
    }, 4200);
  }
}

// --- Panel mounting --------------------------------------------------------

mount("panel-runs", mountRuns);
mount("compare-bar", mountCompare);
mount("panel-overview", mountOverview);
mount("panel-tas", mountTas);
mount("panel-transport", mountTransport);
mount("panel-live", mountLive);
mount("panel-inputs", mountInputs);
mount("panel-speed", mountSpeed);
mount("panel-splits", mountSplits);
mount("panel-path", mountPath3d);

state.subscribe(updateShell);
state.subscribe(renderStatus);
updateShell(state.get());
renderStatus(state.get());

// Tabs. Playback and the runs rail sit outside them, so only the workspace
// panels hide and show; playback keeps running across tabs.
const TAB_KEY = "trackmaniare.tab.v2";
const LEGACY_TAB_KEY = "trackmaniare.tab";
const tabButtons = [...document.querySelectorAll<HTMLButtonElement>(".tab")];
const tabPages = tabButtons.map((button) => document.getElementById(button.getAttribute("aria-controls") ?? ""));

function activateTab(index: number): void {
  tabButtons.forEach((button, i) => {
    const on = i === index;
    button.setAttribute("aria-selected", String(on));
    button.tabIndex = on ? 0 : -1;
    const page = tabPages[i];
    if (page) page.hidden = !on;
  });
  try {
    localStorage.setItem(TAB_KEY, tabPages[index]?.id ?? String(index));
  } catch {
    // Private mode: the tab just does not stick between reloads.
  }
  // Charts and canvases measured a zero-size box while hidden; let them remeasure.
  window.dispatchEvent(new Event("resize"));
}

if (tabButtons.length) {
  let initial = 0;
  try {
    const stored = localStorage.getItem(TAB_KEY) ?? localStorage.getItem(LEGACY_TAB_KEY);
    const legacy: Record<string, string> = {
      "tab-stats": "tab-overview",
      "tab-3d": "tab-3d",
      "tab-inputs": "tab-inputs",
    };
    const wanted = stored ? (legacy[stored] ?? stored) : "";
    const found = wanted ? tabPages.findIndex((page) => page?.id === wanted) : -1;
    if (found >= 0) initial = found;
  } catch {
    // Ignore storage that is unavailable.
  }
  tabButtons.forEach((button, index) => {
    button.addEventListener("click", () => activateTab(index));
    button.addEventListener("keydown", (event) => {
      if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;
      event.preventDefault();
      event.stopPropagation();
      let next = index;
      if (event.key === "ArrowLeft") next = (index + tabButtons.length - 1) % tabButtons.length;
      if (event.key === "ArrowRight") next = (index + 1) % tabButtons.length;
      if (event.key === "Home") next = 0;
      if (event.key === "End") next = tabButtons.length - 1;
      activateTab(next);
      tabButtons[next]?.focus();
    });
  });
  activateTab(initial);
}

// Playback clock.
let lastFrame = performance.now();
function frame(now: number): void {
  const delta = now - lastFrame;
  lastFrame = now;
  const current = clock.get();
  if (current.playing && delta > 0 && delta < 500) {
    const next = current.timeMs + delta * current.rate;
    const duration = selectedDuration();
    if (next >= duration) clock.set({ timeMs: duration, playing: false });
    else clock.set({ timeMs: next });
  }
  requestAnimationFrame(frame);
}
requestAnimationFrame(frame);

// Drop files anywhere on the page.
let dragDepth = 0;
function hasFiles(event: DragEvent): boolean {
  return Boolean(event.dataTransfer && [...event.dataTransfer.types].includes("Files"));
}
document.addEventListener("dragenter", (event) => {
  if (!hasFiles(event)) return;
  dragDepth += 1;
  document.body.classList.add("dragging-file");
});
document.addEventListener("dragleave", (event) => {
  if (!hasFiles(event)) return;
  dragDepth = Math.max(0, dragDepth - 1);
  if (!dragDepth) document.body.classList.remove("dragging-file");
});
document.addEventListener("dragover", (event) => event.preventDefault());
document.addEventListener("drop", (event) => {
  event.preventDefault();
  dragDepth = 0;
  document.body.classList.remove("dragging-file");
  const files = [...(event.dataTransfer?.files ?? [])].filter((file) => /\.gbx$/i.test(file.name));
  if (!files.length) {
    actions.setStatus("", "No .Gbx files were dropped.");
    return;
  }
  void actions.loadFiles(files);
});

// Keyboard shortcuts. Ignore keys aimed at controls, fields, an open dialog,
// or a canvas that manages its own keyboard interaction.
document.addEventListener("keydown", (event) => {
  if (event.key === "Escape" && runsDrawer?.classList.contains("open")) {
    event.preventDefault();
    setDrawer(false, true);
    return;
  }
  if (event.defaultPrevented || event.ctrlKey || event.metaKey || event.altKey) return;
  if (shortcutsDialog?.open) return;
  const target = event.target as HTMLElement | null;
  if (
    target?.closest(
      "input, textarea, select, button, a, [contenteditable='true'], [data-local-shortcuts], dialog",
    )
  ) {
    return;
  }
  switch (event.key) {
    case " ":
      event.preventDefault();
      actions.togglePlay();
      break;
    case "ArrowLeft":
      event.preventDefault();
      actions.nudge(event.shiftKey ? -1000 : -100);
      break;
    case "ArrowRight":
      event.preventDefault();
      actions.nudge(event.shiftKey ? 1000 : 100);
      break;
    case "Home":
      event.preventDefault();
      actions.seek(0);
      break;
    case "End":
      event.preventDefault();
      actions.seek(selectedDuration());
      break;
    case "?":
      event.preventDefault();
      openShortcuts();
      break;
    default:
      break;
  }
});

void initWasm().catch((err) => {
  state.set({
    status: "",
    error: `The WebAssembly module could not be loaded (${describe(err)}). Build it with “npm run wasm”.`,
  });
});

function describe(err: unknown): string {
  if (err instanceof Error) return err.message;
  return String(err);
}
