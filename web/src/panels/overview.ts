//! Run details: identity, race numbers, input stream shape, map download.
//!
//! The numbers live in four labelled columns instead of one long label/value
//! table and instead of nested cards. Values stay on one line and ellipsize;
//! whatever is clipped is in the row's `title`.

import { referenceRun, runDisplayName, selectedRun, type App } from "../app";
import { lastCheckpointIsFinish, SLIP_SLIDE_DEG } from "../lib/derive";
import { clear, h, type Child } from "../lib/dom";
import { downloadBytes, fmtBytes, fmtCount, fmtMeters, fmtTime, fileNameStem } from "../lib/format";
import type { RunData } from "../lib/types";

function sanitize(name: string): string {
  return name.replace(/[^A-Za-z0-9\-_ ]/g, "_");
}

/** Checkpoint count without the finish line, which TM2020 keeps as the last entry. */
function formatCheckpoints(run: RunData): string {
  if (!run.checkpoints.length) return "none recorded";
  const intermediates = run.checkpoints.length - (lastCheckpointIsFinish(run) ? 1 : 0);
  return intermediates ? `${fmtCount(intermediates)} + finish` : "finish only";
}

/** One label/value row; `title` exposes the full string when it can clip. */
function fact(label: string, value: Child, title?: string): HTMLElement {
  const dd = h("dd", {}, value);
  if (title) dd.title = title;
  return h("div", { class: "fact" }, h("dt", { text: label }), dd);
}

function group(title: string, facts: Child[]): HTMLElement {
  return h("section", { class: "fact-group" }, h("h3", { text: title }), h("dl", { class: "facts" }, ...facts));
}

/** A large, scannable number above the detailed fact groups. */
function metric(label: string, value: Child, title?: string): HTMLElement {
  const el = h(
    "div",
    { class: "metric" },
    h("span", { class: "metric-label", text: label }),
    h("span", { class: "metric-value" }, value),
  );
  if (title) el.title = title;
  return el;
}

export function mountOverview(root: HTMLElement, app: App): void {
  function render(): void {
    clear(root);
    const run = selectedRun(app);
    if (!run) {
      root.append(h("div", { class: "muted small", text: "Select a run to see its details." }));
      return;
    }
    const derived = run.derived;
    const reference = referenceRun(app);

    const finish =
      run.raceTimeMs !== null
        ? h("span", { class: "time mono", text: fmtTime(run.raceTimeMs) })
        : h("span", { class: "muted small", text: "not finished" });
    const finishBlock = h("div", { class: "run-finish" }, h("span", { class: "label", text: "Finish" }), finish);
    if (reference && reference.id !== run.id && run.raceTimeMs !== null && reference.raceTimeMs !== null) {
      const diff = run.raceTimeMs - reference.raceTimeMs;
      finishBlock.append(
        h(
          "div",
          { class: "run-vs" },
          h("span", { class: "muted", text: `vs ${runDisplayName(reference)} ` }),
          h("strong", { class: diff > 0 ? "delta-behind" : "delta-ahead", text: `${diff > 0 ? "+" : "−"}${fmtTime(Math.abs(diff))}` }),
        ),
      );
    }

    const header = h(
      "div",
      { class: "run-head" },
      h(
        "div",
        { class: "run-who" },
        h(
          "div",
          { class: "run-name" },
          h("span", {
            class: "swatch",
            style: `background:${run.color || "#3a4a5a"}`,
            title: "Run colour: used in the charts, split deltas and the 3D path",
          }),
          h("span", { class: "nick", text: runDisplayName(run) }),
        ),
        run.login ? h("div", { class: "mono small muted", text: run.login }) : null,
        run.mapName ? h("div", { class: "small muted", text: run.mapName }) : null,
      ),
      finishBlock,
    );

    const runFacts: Child[] = [fact("Checkpoints", formatCheckpoints(run))];
    if (run.respawns !== null) runFacts.push(fact("Respawns (recorded)", fmtCount(run.respawns)));
    if (derived?.respawnTimes.length) runFacts.push(fact("Respawns (input events)", fmtCount(derived.respawnTimes.length)));
    if (derived) {
      runFacts.push(
        fact(
          "Wall hits (guess)",
          derived.wallHits.length ? fmtCount(derived.wallHits.length) : h("span", { class: "muted", text: "none" }),
          derived.wallHits.length ? "Guessed from speed drops; orange points in the 3D path" : undefined,
        ),
      );
    }

    const motionFacts: Child[] = [fact("Samples (20 Hz)", run.sampleCount ? fmtCount(run.sampleCount) : "none")];
    if (run.samples) {
      const first = run.sampleStartMs ?? run.samples.times[0];
      const last = run.sampleEndMs ?? run.samples.times[run.samples.times.length - 1];
      motionFacts.push(fact("Sample range", `${fmtTime(first)} – ${fmtTime(last)}`));
    }
    if (derived) {
      motionFacts.push(
        fact(
          "Sliding",
          derived.slideMs > 0
            ? `${fmtTime(derived.slideMs)} · up to ${derived.maxSlipDeg.toFixed(0)}°`
            : h("span", { class: "muted", text: `none over ${SLIP_SLIDE_DEG}°` }),
        ),
      );
    }

    const inputFacts: Child[] = [];
    if (run.inputs) {
      const inputs = run.inputs;
      inputFacts.push(fact("Stream", `${inputs.format} v${inputs.version}`));
      inputFacts.push(fact("Ticks", `${fmtCount(inputs.ticks)} ticks`));
      inputFacts.push(fact("Events", `${fmtCount(inputs.events.length)} events`));
      if (inputs.startOffsetMs !== null) inputFacts.push(fact("Start offset", `${inputs.startOffsetMs} ms`));
    } else {
      inputFacts.push(fact("Input stream", h("span", { class: "muted", text: "none" })));
    }
    if (run.tas) {
      const lines = run.tas.split("\n").length - 1;
      inputFacts.push(fact("TAS export", `${fmtCount(lines)} lines${run.tasDropped ? ` · ${fmtCount(run.tasDropped)} dropped` : ""}`));
    }

    const mapFacts: Child[] = [
      fact("Name", run.mapName ?? h("span", { class: "muted", text: "unknown" }), run.mapName ?? undefined),
    ];
    if (run.mapUid) mapFacts.push(fact("UID", h("span", { class: "mono", text: run.mapUid }), run.mapUid));
    if (run.mapEnvironment) mapFacts.push(fact("Environment", run.mapEnvironment));
    if (run.mapAuthor) mapFacts.push(fact("Author", run.mapAuthor));
    mapFacts.push(fact("File", run.fileName, run.fileName));
    mapFacts.push(fact("Size", fmtBytes(run.fileSize)));
    if (run.map) {
      const map = run.map;
      mapFacts.push(
        h(
          "div",
          { class: "map-download" },
          h("button", {
            class: "btn",
            text: `Download map (${fmtBytes(map.bytes.length)})`,
            onclick: () =>
              downloadBytes(
                `${sanitize(run.mapName ?? fileNameStem(run.fileName))}.Challenge.Gbx`,
                map.bytes,
                "application/octet-stream",
              ),
          }),
          h("div", { class: "muted small", text: "raw embedded .Challenge.Gbx" }),
        ),
      );
    }

    const warnings = run.warnings.length
      ? h(
          "div",
          { class: "warnings-box" },
          h("strong", { text: "Warnings" }),
          h("ul", { class: "warnings" }, ...run.warnings.map((warning) => h("li", { text: warning }))),
        )
      : null;

    const metrics = derived
      ? h(
          "div",
          { class: "metric-grid" },
          metric("Distance", fmtMeters(derived.totalDistance), "Distance travelled along the sampled path"),
          metric("Top speed", `${derived.topSpeedKmh.toFixed(0)} km/h`),
          metric("Average speed", `${derived.avgSpeedKmh.toFixed(0)} km/h`),
          metric("Airborne", fmtTime(derived.airborneMs)),
        )
      : null;

    root.append(
      h(
        "div",
        { class: "overview" },
        header,
        warnings,
        metrics,
        h(
          "div",
          { class: "fact-grid" },
          group("Run", runFacts),
          group("Motion", motionFacts),
          group("Input", inputFacts),
          group("Map & file", mapFacts),
        ),
      ),
    );
  }

  app.state.subscribe(render);
  render();
}
