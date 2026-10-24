//! Speed chart (all visible runs) and distance delta (selection vs reference),
//! with compact legends and checkpoint/finish guides.

import { referenceRun, runDisplayName, runRoleLabel, selectedRun, visibleRuns, type App } from "../app";
import { TimeChart, type PlotMarker } from "../lib/chart";
import { buildDeltaSeries, buildSpeedSeries, lastCheckpointIsFinish } from "../lib/derive";
import { clear, h } from "../lib/dom";
import type { RunData } from "../lib/types";

function markersFor(run: RunData | null): PlotMarker[] {
  if (!run) return [];
  const finishIsLast = lastCheckpointIsFinish(run);
  const markers: PlotMarker[] = run.checkpoints.map((checkpoint, index) => ({
    timeMs: checkpoint.timeMs,
    label: finishIsLast && index === run.checkpoints.length - 1 ? "Finish" : `Checkpoint ${index + 1}`,
    kind: finishIsLast && index === run.checkpoints.length - 1 ? "finish" : "checkpoint",
  }));
  if (run.raceTimeMs !== null) {
    const last = run.checkpoints[run.checkpoints.length - 1]?.timeMs;
    if (last === undefined || Math.abs(last - run.raceTimeMs) > 1) {
      markers.push({ timeMs: run.raceTimeMs, label: "Finish", kind: "finish" });
    }
  }
  return markers;
}

export function mountSpeed(root: HTMLElement, app: App): void {
  const speedHost = h("div", { class: "chart-host" });
  const deltaHost = h("div", { class: "chart-host" });
  const speedLegend = h("div", { class: "chart-legend" });
  const deltaLegend = h("div", { class: "chart-legend" });
  const speedMessage = h("div", { class: "chart-message" });
  const deltaMessage = h("div", { class: "chart-message" });
  const note = h("p", { class: "chart-note" });
  const markerNote = h("p", { class: "chart-note" });

  const speedSection = h(
    "section",
    { class: "chart-section" },
    h("div", { class: "chart-head" }, h("h3", { text: "Speed" }), speedLegend),
    speedHost,
    speedMessage,
    markerNote,
  );
  const deltaSection = h(
    "section",
    { class: "chart-section" },
    h("div", { class: "chart-head" }, h("h3", { text: "Distance delta" }), deltaLegend),
    note,
    deltaHost,
    deltaMessage,
  );
  root.append(speedSection, deltaSection);

  const speedChart = new TimeChart(speedHost, {
    height: 180,
    yLabel: "km/h",
    showLegend: false,
    getTime: () => app.clock.get().timeMs,
    onSeek: (ms) => app.actions.seek(ms),
  });
  const deltaChart = new TimeChart(deltaHost, {
    height: 150,
    yLabel: "Δ seconds",
    zeroLine: true,
    showLegend: false,
    getTime: () => app.clock.get().timeMs,
    onSeek: (ms) => app.actions.seek(ms),
  });

  function renderLegend(legend: HTMLElement, entries: { label: string; color: string; title?: string }[]): void {
    clear(legend);
    for (const entry of entries) {
      const key = h(
        "span",
        { class: "legend-key", title: entry.title ?? entry.label },
        h("span", { class: "legend-dot", style: `background:${entry.color}` }),
        h("span", { class: "legend-label", text: entry.label }),
      );
      legend.append(key);
    }
  }

  function render(): void {
    const runs = visibleRuns(app).map((entry) => entry.run);
    const selected = selectedRun(app);
    const reference = referenceRun(app);

    const withSamples = runs.filter((run) => run.samples && run.samples.times.length > 0);
    const data = withSamples.length ? buildSpeedSeries(runs) : null;
    const drawable = Boolean(data && data.xs.length > 1);
    speedHost.hidden = !drawable;
    speedMessage.hidden = drawable;
    if (drawable && data) {
      speedChart.render({
        xs: data.xs,
        series: data.series.map(({ run, values }) => ({
          label: runRoleLabel(run, app),
          color: run.color || "#4cc9f0",
          values,
        })),
        markers: markersFor(selected),
      });
      renderLegend(
        speedLegend,
        data.series.map(({ run }) => ({
          label: runRoleLabel(run, app),
          color: run.color || "#4cc9f0",
          title: [runDisplayName(run), run.mapName, run.fileName].filter(Boolean).join("\n"),
        })),
      );
    } else {
      clear(speedLegend);
      speedMessage.textContent = selected
        ? withSamples.length
          ? "Not enough samples to draw a speed chart."
          : "This run has no vehicle samples."
        : "Select a run to draw its speed.";
    }
    markerNote.hidden = !drawable || !selected || (!selected.checkpoints.length && selected.raceTimeMs === null);
    markerNote.textContent = "Dashed guides mark checkpoints; the amber guide marks the finish.";

    clear(note);
    clear(deltaLegend);
    if (selected && reference && reference.id !== selected.id) {
      const referenceName = runDisplayName(reference);
      note.append(`Delta against ${referenceName}: time at the same distance (below the zero line means ahead).`);
      const sameMap = !selected.mapUid || !reference.mapUid || selected.mapUid === reference.mapUid;
      if (!sameMap) {
        note.append(" ");
        note.append(
          h("span", {
            class: "badge badge-warn",
            text: "different maps",
            title: "Distance-based deltas compare positions on different tracks.",
          }),
        );
      }
      const delta = buildDeltaSeries(selected, reference);
      const deltaDrawable = Boolean(delta && delta.xs.length > 1);
      deltaHost.hidden = !deltaDrawable;
      deltaMessage.hidden = deltaDrawable;
      if (deltaDrawable && delta) {
        deltaChart.render({
          xs: delta.xs,
          series: [{ label: `Δ vs ${referenceName}`, color: reference.color || "#f72585", values: delta.values }],
          markers: markersFor(selected),
        });
        renderLegend(deltaLegend, [
          {
            label: `Δ vs ${referenceName}`,
            color: reference.color || "#f72585",
            title: "Positive is behind the reference at the same distance",
          },
        ]);
      } else {
        deltaMessage.textContent = "No comparable distance covered.";
      }
    } else {
      note.textContent = "Compare with another run from the runs panel to draw a distance delta.";
      deltaHost.hidden = true;
      deltaMessage.hidden = false;
      deltaMessage.textContent = reference
        ? "This run is the current selection; choose another comparison."
        : "No reference run selected.";
    }
  }

  app.state.subscribe(render);
  app.clock.subscribe((clock) => {
    speedChart.setTime(clock.timeMs);
    deltaChart.setTime(clock.timeMs);
  });
  render();
  speedChart.setTime(app.clock.get().timeMs);
  deltaChart.setTime(app.clock.get().timeMs);
}
