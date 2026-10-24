//! Persistent playback transport: transport controls, checkpoint markers,
//! finish versus recorded duration, and a compact speed picker.

import { runDisplayName, selectedRun, type App } from "../app";
import { lastCheckpointIsFinish } from "../lib/derive";
import { clear, h } from "../lib/dom";
import { fmtCount, fmtTime } from "../lib/format";
import { icon } from "../lib/icons";
import { PLAYBACK_RATES, formatRate } from "../lib/rates";
import { button, iconButton } from "../lib/ui";

export function mountTransport(root: HTMLElement, app: App): void {
  const runDot = h("span", { class: "transport-dot" });
  const runTitle = h("span", { class: "transport-title", text: "No run loaded" });
  const runSub = h("span", { class: "transport-sub" });
  const meta = h("div", { class: "transport-meta" }, h("div", { class: "transport-name" }, runDot, runTitle), runSub);

  const startButton = iconButton("skip-start", "Start (Home)", {
    variant: "ghost",
    className: "transport-step",
    onClick: () => app.actions.seek(0),
  });
  const back1 = button({
    label: "−1s",
    variant: "ghost",
    className: "transport-step transport-nudge",
    title: "Back 1 second (Shift+←)",
    onClick: () => app.actions.nudge(-1000),
  });
  const back01 = button({
    label: "−0.1s",
    variant: "ghost",
    className: "transport-step transport-nudge",
    title: "Back 0.1 seconds (←)",
    onClick: () => app.actions.nudge(-100),
  });
  const playButton = button({
    label: "Play",
    icon: "play",
    variant: "primary",
    className: "transport-play",
    title: "Play or pause (Space)",
    onClick: () => app.actions.togglePlay(),
  });
  const forward01 = button({
    label: "+0.1s",
    variant: "ghost",
    className: "transport-step transport-nudge",
    title: "Forward 0.1 seconds (→)",
    onClick: () => app.actions.nudge(100),
  });
  const forward1 = button({
    label: "+1s",
    variant: "ghost",
    className: "transport-step transport-nudge",
    title: "Forward 1 second (Shift+→)",
    onClick: () => app.actions.nudge(1000),
  });
  const endButton = iconButton("skip-end", "End (End)", {
    variant: "ghost",
    className: "transport-step",
    onClick: () => {
      const run = selectedRun(app);
      if (run) app.actions.seek(run.durationMs);
    },
  });
  const controls = h(
    "div",
    { class: "transport-controls", role: "group", "aria-label": "Playback controls" },
    startButton,
    back1,
    back01,
    playButton,
    forward01,
    forward1,
    endButton,
  );

  const timeLabel = h("span", { class: "mono transport-time", text: "0:00.000" });
  const totalLabel = h("span", { class: "mono muted", text: "/ 0:00.000" });
  const finishLabel = h("span", { class: "mono transport-finish" });
  const timeline = h("input", {
    type: "range",
    class: "slider timeline",
    min: "0",
    max: "1000",
    step: "10",
    value: "0",
    "aria-label": "Playback position",
  }) as HTMLInputElement;
  const markers = h("div", { class: "timeline-markers", "aria-hidden": "true" });
  const timelineWrap = h("div", { class: "timeline-wrap" }, markers, timeline);
  const timelineBlock = h(
    "div",
    { class: "transport-timeline" },
    h("div", { class: "timeline-labels" }, h("span", {}, timeLabel, h("span", { class: "timeline-sep", text: " / " }), totalLabel), finishLabel),
    timelineWrap,
  );
  timeline.addEventListener("input", () => app.actions.seek(Number(timeline.value)));

  const rateSelect = h(
    "select",
    {
      class: "rate-select",
      "aria-label": "Playback speed",
      title: "Playback speed",
      onchange: () => app.actions.setRate(Number(rateSelect.value)),
    },
    ...PLAYBACK_RATES.map((rate) => h("option", { value: String(rate), text: formatRate(rate) })),
  ) as HTMLSelectElement;
  rateSelect.value = "1";
  const rateControl = h(
    "label",
    { class: "rate-control" },
    h("span", { text: "Speed" }),
    rateSelect,
  );

  const hint = h("p", { class: "transport-hint" });
  root.append(
    h("div", { class: "transport-inner" }, meta, controls, timelineBlock, rateControl, hint),
  );

  function setIcon(target: HTMLButtonElement, name: Parameters<typeof icon>[0]): void {
    target.querySelector("svg")?.replaceWith(icon(name));
  }

  function renderMarkers(run: ReturnType<typeof selectedRun>): void {
    clear(markers);
    if (!run) return;
    const duration = Math.max(run.durationMs, 1);
    const finishIsLast = lastCheckpointIsFinish(run);
    const finishTime = run.checkpoints.length && finishIsLast
      ? run.checkpoints[run.checkpoints.length - 1].timeMs
      : run.raceTimeMs;
    run.checkpoints.forEach((checkpoint, index) => {
      const isFinish = finishIsLast && index === run.checkpoints.length - 1;
      const marker = h("span", {
        class: `timeline-marker${isFinish ? " is-finish" : ""}`,
        style: `left:${Math.max(0, Math.min(100, (checkpoint.timeMs / duration) * 100))}%`,
        title: isFinish ? `Finish · ${fmtTime(checkpoint.timeMs)}` : `Checkpoint ${index + 1} · ${fmtTime(checkpoint.timeMs)}`,
      });
      markers.append(marker);
    });
    if (
      finishTime !== null &&
      (!run.checkpoints.length || !finishIsLast || Math.abs((run.checkpoints[run.checkpoints.length - 1]?.timeMs ?? -1) - finishTime) > 1)
    ) {
      markers.append(
        h("span", {
          class: "timeline-marker is-finish",
          style: `left:${Math.max(0, Math.min(100, (finishTime / duration) * 100))}%`,
          title: `Finish · ${fmtTime(finishTime)}`,
        }),
      );
    }
  }

  function renderState(): void {
    const run = selectedRun(app);
    const duration = Math.max(run?.durationMs ?? 0, 1);
    const playable = Boolean(run?.samples) || Boolean(run?.inputs);
    timeline.max = String(Math.round(duration));
    timeline.disabled = !playable;
    playButton.disabled = !playable;
    startButton.disabled = !playable;
    endButton.disabled = !playable;
    back1.disabled = !playable;
    back01.disabled = !playable;
    forward01.disabled = !playable;
    forward1.disabled = !playable;
    rateSelect.disabled = !playable;
    runDot.style.background = run?.color || "var(--text-faint)";
    const name = run ? runDisplayName(run) : "No run loaded";
    runTitle.textContent = name;
    runTitle.title = name;
    if (run) {
      const finish = run.raceTimeMs !== null ? `${fmtTime(run.raceTimeMs)} finish` : "no finish";
      const detail = run.sampleCount
        ? `${fmtCount(run.sampleCount)} samples`
        : run.inputs
          ? `${fmtCount(run.inputs.ticks)} ticks`
          : "no samples or inputs";
      runSub.textContent = `${finish} · ${detail}`;
    } else {
      runSub.textContent = "Open a replay to start";
    }
    totalLabel.textContent = `/ ${fmtTime(run?.durationMs ?? 0)}`;
    finishLabel.textContent = run?.raceTimeMs != null ? `finish ${fmtTime(run.raceTimeMs)}` : "";
    finishLabel.hidden = !run?.raceTimeMs;
    hint.hidden = !run;
    hint.textContent = run
      ? run.samples
        ? "Playback follows the recorded 20 Hz samples; inputs are drawn at 100 Hz."
        : "No vehicle samples in this run; only the input stream is available."
      : "";
    if (run?.inputs?.truncated) hint.textContent += " The input stream is truncated in this file.";
    renderMarkers(run);
  }

  function renderClock(): void {
    const { timeMs, playing, rate } = app.clock.get();
    const duration = Math.max(selectedRun(app)?.durationMs ?? 0, 1);
    timeLabel.textContent = fmtTime(timeMs);
    playButton.querySelector(".btn-label")!.textContent = playing ? "Pause" : "Play";
    playButton.title = playing ? "Pause (Space)" : "Play (Space)";
    setIcon(playButton, playing ? "pause" : "play");
    if (document.activeElement !== timeline) timeline.value = String(Math.round(timeMs));
    timeline.style.setProperty("--pct", String(Math.min(1, timeMs / duration)));
    if (document.activeElement !== rateSelect) {
      const value = String(rate);
      if (![...rateSelect.options].some((option) => option.value === value)) {
        rateSelect.append(h("option", { value, text: formatRate(rate) }));
      }
      rateSelect.value = value;
    }
  }

  app.state.subscribe(renderState);
  app.clock.subscribe(renderClock);
  renderState();
  renderClock();
}
