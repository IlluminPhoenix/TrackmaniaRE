//! Runs panel: files grouped by source, the active run, comparison toggles and
//! recent files stored in this browser.

import { runDisplayName, selectedRun, type App } from "../app";
import { clear, h } from "../lib/dom";
import { fileNameStem, fmtBytes, fmtCount, fmtTime } from "../lib/format";
import { icon } from "../lib/icons";
import {
  clearRecents,
  deleteRecent,
  listRecents,
  loadRecent,
  type RecentRun,
} from "../lib/persist";
import type { RunData } from "../lib/types";
import { badge, iconButton } from "../lib/ui";

export function mountRuns(root: HTMLElement, app: App): void {
  const input = document.getElementById("file-input") as HTMLInputElement | null;

  /** Restore focus after a re-render replaces the control that had it. */
  function restoreFocus(runId: string, selector: string): void {
    queueMicrotask(() => {
      if (document.activeElement && document.activeElement !== document.body) return;
      const row = [...root.querySelectorAll<HTMLElement>(".run-row")].find((entry) => entry.dataset.runId === runId);
      row?.querySelector<HTMLElement>(selector)?.focus();
    });
  }

  const dropzone = h(
    "button",
    {
      class: "dropzone",
      type: "button",
      title: "Open .Replay.Gbx or .Ghost.Gbx files",
      onclick: () => input?.click(),
    },
    icon("open", 18),
    h(
      "span",
      { class: "dropzone-text" },
      h("strong", { text: "Open files" }),
      h("span", { text: "Drop .Replay.Gbx / .Ghost.Gbx" }),
    ),
  );
  dropzone.addEventListener("dragover", (event) => {
    event.preventDefault();
    dropzone.classList.add("dragging");
  });
  dropzone.addEventListener("dragleave", () => dropzone.classList.remove("dragging"));
  dropzone.addEventListener("drop", () => dropzone.classList.remove("dragging"));

  const list = h("div", { class: "run-list" });
  const recentDetails = h("details", { class: "recent-block" }) as HTMLDetailsElement;
  const recentSummary = h("summary", {}, h("span", { text: "Recent files" }), h("span", { class: "recent-count" }));
  const recentBody = h("div", { class: "recent-body" });
  recentDetails.append(recentSummary, recentBody);

  root.append(dropzone, list, recentDetails);

  let recents: RecentRun[] = [];
  let firstRecents = true;

  async function refreshRecents(): Promise<void> {
    recents = await listRecents();
    renderRecents();
  }

  function renderRecents(): void {
    recentSummary.querySelector(".recent-count")!.textContent = String(recents.length);
    clear(recentBody);
    if (recents.length) {
      const total = recents.reduce((sum, entry) => sum + entry.fileSize, 0);
      recentBody.append(
        h(
          "div",
          { class: "recent-head" },
          h("span", { class: "muted small", text: `${fmtBytes(total)} stored in this browser` }),
          h("button", {
            class: "link-button",
            type: "button",
            text: "Clear all",
            title: "Delete every stored replay",
            onclick: async () => {
              const ok =
                typeof window.confirm !== "function" ||
                window.confirm("Delete every replay stored in this browser?");
              if (!ok) return;
              await clearRecents();
              await refreshRecents();
            },
          }),
        ),
      );
    } else {
      recentBody.append(h("p", { class: "recent-empty", text: "Nothing stored yet." }));
    }

    for (const entry of recents) {
      const load = async (): Promise<void> => {
        app.actions.setStatus(`Loading ${entry.fileName}…`);
        const bytes = await loadRecent(entry.id);
        if (!bytes) {
          await refreshRecents();
          app.actions.setStatus("", `The stored copy of ${entry.fileName} is gone.`);
          return;
        }
        await app.actions.loadBytes(entry.fileName, bytes, { persist: false });
      };
      const remove = async (): Promise<void> => {
        await deleteRecent(entry.id);
        await refreshRecents();
      };
      recentBody.append(
        h(
          "div",
          { class: "recent-row" },
          h(
            "button",
            {
              class: "recent-load",
              type: "button",
              title: entry.fileName,
              "aria-label": `Load ${entry.fileName}`,
              onclick: () => void load(),
            },
            h(
              "span",
              { class: "recent-text" },
              h("span", { class: "recent-name", text: fileNameStem(entry.fileName) }),
              h(
                "span",
                { class: "recent-meta" },
                `${fmtBytes(entry.fileSize)} · ${fmtCount(entry.runCount)} ${entry.runCount === 1 ? "run" : "runs"}`,
              ),
            ),
          ),
          iconButton("close", `Remove ${entry.fileName} from storage`, {
            variant: "ghost",
            className: "recent-remove",
            onClick: () => void remove(),
          }),
        ),
      );
    }

    if (firstRecents && !app.state.get().runs.length && recents.length) {
      firstRecents = false;
      recentDetails.open = true;
    }
  }

  function renderList(): void {
    clear(list);
    const { runs, compareIds, selectedId } = app.state.get();
    if (!runs.length) {
      list.append(
        h(
          "div",
          { class: "sidebar-empty" },
          icon("runs", 20),
          h("strong", { text: "No runs loaded" }),
          h("span", { text: "Open a replay or ghost to start inspecting." }),
        ),
      );
      return;
    }

    const groups = new Map<string, RunData[]>();
    for (const run of runs) {
      const group = groups.get(run.fileName);
      if (group) group.push(run);
      else groups.set(run.fileName, [run]);
    }

    const current = selectedRun(app);
    for (const [fileName, fileRuns] of groups) {
      const maps = new Set(fileRuns.map((run) => run.mapName).filter((name): name is string => Boolean(name)));
      const mapLabel = maps.size === 1 ? [...maps][0] : maps.size > 1 ? "mixed maps" : null;
      const meta = [
        `${fileRuns.length} ${fileRuns.length === 1 ? "run" : "runs"}`,
        fmtBytes(fileRuns[0].fileSize),
        mapLabel,
      ]
        .filter(Boolean)
        .join(" · ");
      const removeFile = iconButton("trash", `Close all runs from ${fileName}`, {
        variant: "ghost",
        className: "file-remove",
        onClick: () => app.actions.removeFile(fileName),
      });
      const group = h(
        "div",
        { class: "file-group", "data-file": fileName },
        h(
          "div",
          { class: "file-head" },
          h(
            "div",
            { class: "file-copy" },
            h("div", { class: "file-name", text: fileNameStem(fileName), title: fileName }),
            h("div", { class: "file-meta", text: meta }),
          ),
          removeFile,
        ),
        h(
          "div",
          { class: "file-runs" },
          ...fileRuns.map((run) => runRow(run, run.id === selectedId, compareIds.includes(run.id), current)),
        ),
      );
      list.append(group);
    }
  }

  function runRow(
    run: RunData,
    selected: boolean,
    comparing: boolean,
    current: RunData | null,
  ): HTMLElement {
    const title = runDisplayName(run);
    const time = run.raceTimeMs !== null ? fmtTime(run.raceTimeMs) : "no finish";
    const sampleText = run.sampleCount
      ? `${fmtCount(run.sampleCount)} samples`
      : run.inputs
        ? `${fmtCount(run.inputs.ticks)} ticks`
        : "no data";
    const metaTitle = [
      title,
      `Finish: ${run.raceTimeMs !== null ? fmtTime(run.raceTimeMs) : "not recorded"}`,
      `Samples: ${run.sampleCount ? fmtCount(run.sampleCount) : "none"}`,
      `Inputs: ${run.inputs ? `${fmtCount(run.inputs.ticks)} ticks` : "none"}`,
      run.warnings.length ? `Warnings:\n${run.warnings.join("\n")}` : "",
    ]
      .filter(Boolean)
      .join("\n");

    const info = h(
      "div",
      { class: "run-copy" },
      h(
        "div",
        { class: "run-title-line" },
        h("span", {
          class: "run-swatch",
          style: `background:${run.color || "#3a4a5a"}`,
          title: "Run colour in charts, splits and the 3D path",
        }),
        h("span", { class: "run-title", text: title }),
      ),
      h(
        "div",
        { class: "run-meta" },
        h("span", { class: "run-time", text: time }),
        h("span", { text: "·" }),
        h("span", { text: sampleText }),
      ),
      (run.warnings.length || !run.samples || !run.inputs) && h(
        "div",
        { class: "run-tags" },
        !run.samples ? badge("no samples", "warn") : null,
        !run.inputs ? badge("no inputs") : null,
        run.warnings.length
          ? badge(`${run.warnings.length} warning${run.warnings.length === 1 ? "" : "s"}`, "warn", run.warnings.join("\n"))
          : null,
      ),
    );

    const selectButton = h(
      "button",
      {
        class: "run-select",
        type: "button",
        "aria-pressed": String(selected),
        title: metaTitle,
        onclick: () => {
          app.actions.select(run.id);
          restoreFocus(run.id, ".run-select");
        },
      },
      info,
    );

    const trailing = selected
      ? h("span", { class: "run-current", text: "Current" })
      : h(
          "label",
          {
            class: "compare-toggle",
            title: `Compare ${runDisplayName(run)} with ${current ? runDisplayName(current) : "the current run"}`,
          },
          h("input", {
            type: "checkbox",
            checked: comparing,
            "aria-label": `Compare ${runDisplayName(run)} with ${current ? runDisplayName(current) : "the current run"}`,
            onchange: () => {
              app.actions.toggleCompare(run.id);
              restoreFocus(run.id, "input[type=checkbox]");
            },
          }),
          h("span", { text: "Compare" }),
        );

    return h(
      "div",
      { class: `run-row${selected ? " selected" : ""}${comparing ? " comparing" : ""}`, "data-run-id": run.id },
      selectButton,
      trailing,
      iconButton("close", `Close ${runDisplayName(run)}`, {
        variant: "ghost",
        className: "run-remove",
        onClick: () => app.actions.removeRun(run.id),
      }),
    );
  }

  let lastRunCount = -1;
  let lastRecentsVersion = -1;
  app.state.subscribe((current) => {
    renderList();
    if (current.runs.length !== lastRunCount || current.recentsVersion !== lastRecentsVersion) {
      lastRunCount = current.runs.length;
      lastRecentsVersion = current.recentsVersion;
      void refreshRecents();
    }
  });
  renderList();
  void refreshRecents();
}
