//! Checkpoint splits, optionally against the reference run.

import { referenceRun, runDisplayName, selectedRun, type App } from "../app";
import { splitsFor } from "../lib/derive";
import { clear, h } from "../lib/dom";
import { fmtDelta, fmtTime } from "../lib/format";

export function mountSplits(root: HTMLElement, app: App): void {
  let rowEls: { timeMs: number; el: HTMLTableRowElement }[] = [];

  function updateCurrent(): void {
    const time = app.clock.get().timeMs;
    let active = -1;
    rowEls.forEach((row, index) => {
      if (row.timeMs <= time + 0.5) active = index;
    });
    rowEls.forEach((row, index) => row.el.classList.toggle("current", index === active));
  }

  function render(): void {
    clear(root);
    rowEls = [];
    const run = selectedRun(app);
    if (!run) {
      root.append(h("div", { class: "muted small", text: "Select a run to see its splits." }));
      return;
    }
    const reference = referenceRun(app);
    const compare = reference && reference.id !== run.id ? reference : null;
    const rows = splitsFor(run, compare);
    if (!rows.length) {
      root.append(h("div", { class: "muted small", text: "This run has no recorded checkpoints or finish time." }));
      return;
    }
    const byDistance = new Set(rows.map((r) => r.timeMs));
    if (byDistance.size === 1 && rows.length > 1) {
      // Platform-style ghosts can record every checkpoint at the same time.
      root.append(h("div", { class: "muted small", text: "Checkpoints carry no usable times in this ghost; only the finish is shown." }));
    }
    const sameMap = !compare || !run.mapUid || !compare.mapUid || run.mapUid === compare.mapUid;
    const tbody = h("tbody");
    for (const row of rows) {
      const tr = h(
        "tr",
        { "data-time": String(row.timeMs) },
        h("td", {}, row.label),
        h("td", { class: "num" }, fmtTime(row.timeMs)),
        compare
          ? h(
              "td",
              {
                class: `num ${row.deltaMs === null ? "muted" : row.deltaMs > 0 ? "delta-behind" : "delta-ahead"}`,
              },
              row.deltaMs === null ? "–" : fmtDelta(row.deltaMs),
            )
          : null,
      ) as HTMLTableRowElement;
      tbody.append(tr);
      rowEls.push({ timeMs: row.timeMs, el: tr });
    }
    root.append(
      h(
        "div",
        { class: "splits-scroll" },
        h(
          "table",
          { class: "data" },
          h("caption", { class: "sr-only", text: "Checkpoint splits" }),
          h(
            "thead",
            {},
            h(
              "tr",
              {},
              h("th", { scope: "col" }, "Split"),
              h("th", { scope: "col", class: "num" }, "Time"),
              compare ? h("th", { scope: "col", class: "num" }, `Δ vs ${runDisplayName(compare)}`) : null,
            ),
          ),
          tbody,
        ),
      ),
    );
    if (compare && !sameMap) {
      root.append(
        h(
          "p",
          { class: "small map-warning" },
          h("span", { class: "badge badge-warn", text: "different maps" }),
          h("span", { class: "muted", text: " checkpoint indexes are not comparable" }),
        ),
      );
    }
    updateCurrent();
  }

  app.state.subscribe(render);
  app.clock.subscribe(updateCurrent);
  render();
}
