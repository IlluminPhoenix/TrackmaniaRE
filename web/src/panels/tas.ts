//! TAS panel: the TICK input script for the selected run, ready to paste.

import { selectedRun, type App } from "../app";
import { clear, h } from "../lib/dom";
import { copyText, downloadText, fileNameStem, fmtCount } from "../lib/format";
import { icon } from "../lib/icons";
import { badge } from "../lib/ui";

/** Above this size the script is not put into a textarea until asked for. */
const INLINE_LIMIT = 2 * 1024 * 1024;

export function mountTas(root: HTMLElement, app: App): void {
  function render(): void {
    clear(root);
    const run = selectedRun(app);
    if (!run) {
      root.append(h("div", { class: "muted small", text: "Select a run to export its inputs." }));
      return;
    }
    if (!run.tas) {
      root.append(
        h(
          "div",
          { class: "panel-note" },
          icon("info", 16),
          h("span", {
            text: run.inputs
              ? `This run holds ${run.inputs.format} inputs; TICK scripts are only generated for Trackmania 2020 input streams.`
              : "This run has no input stream, so there is no TAS script to export.",
          }),
        ),
      );
      return;
    }
    const text = run.tas;
    const lines = text.length ? text.split("\n").length - 1 : 0;
    const copy = h("button", { class: "btn btn-primary", type: "button", text: "Copy script" });
    copy.addEventListener("click", async () => {
      const ok = await copyText(text);
      copy.textContent = ok ? "Copied" : "Copy failed";
      setTimeout(() => (copy.textContent = "Copy script"), 1500);
    });

    root.append(
      h(
        "div",
        { class: "tas-layout" },
        h(
          "div",
          { class: "tas-toolbar" },
          copy,
          h(
            "button",
            {
              class: "btn",
              type: "button",
              text: "Download .txt",
              onclick: () => downloadText(`${fileNameStem(run.fileName)}.tas.txt`, text),
            },
          ),
          badge(`${fmtCount(lines)} lines`, "neutral"),
        ),
        h(
          "p",
          { class: "tas-note" },
          "Format: ",
          h("code", { text: "<time_ms> <action> <value>" }),
          ". Only verified TICK keywords are written. ",
          run.tasDropped
            ? `${fmtCount(run.tasDropped)} events this run carries (horn, action slots, mouse, camera) have no verified TICK keyword and are left out.`
            : "This run has no events that had to be left out.",
        ),
        text.length > INLINE_LIMIT
          ? scriptPreview(text)
          : h("textarea", {
              class: "tas",
              readonly: true,
              spellcheck: "false",
              rows: String(Math.max(6, Math.min(16, lines))),
              "aria-label": "TAS input script",
              text,
            }),
      ),
    );
  }

  function scriptPreview(text: string): HTMLElement {
    const box = h("div", { class: "stack" });
    const show = h("button", {
      class: "btn",
      type: "button",
      text: `Show script (${fmtCount(text.length)} characters)`,
    });
    show.addEventListener("click", () => {
      show.remove();
      box.append(
        h("textarea", {
          class: "tas",
          readonly: true,
          spellcheck: "false",
          rows: "16",
          "aria-label": "TAS input script",
          text,
        }),
      );
    });
    box.append(show);
    return box;
  }

  app.state.subscribe(render);
  render();
}
