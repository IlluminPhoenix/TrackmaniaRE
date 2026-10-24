//! Comparison bar: names the selected run, lists every comparison run and
//! makes the reference explicit. Multiple overlays stay supported; the radio
//! picks which one drives deltas and splits.

import { compareRuns, referenceRun, runDisplayName, selectedRun, type App } from "../app";
import { clear, h } from "../lib/dom";
import { fmtTime } from "../lib/format";
import { iconButton } from "../lib/ui";

export function mountCompare(root: HTMLElement, app: App): void {
  root.setAttribute("role", "region");
  root.setAttribute("aria-label", "Run comparison");

  function render(): void {
    clear(root);
    const selected = selectedRun(app);
    const comparisons = compareRuns(app);
    if (!selected || !comparisons.length) {
      root.hidden = true;
      return;
    }
    const reference = referenceRun(app);
    const differentMaps = comparisons.some(
      (run) => run.mapUid && selected.mapUid && run.mapUid !== selected.mapUid,
    );

    const options = comparisons.map((run) => {
      const isReference = reference?.id === run.id;
      const radio = h("input", {
        type: "radio",
        name: "compare-reference",
        value: run.id,
        checked: isReference,
        "aria-label": `Use ${runDisplayName(run)} as the reference`,
        onchange: () => {
          app.actions.makeReference(run.id);
          queueMicrotask(() => {
            if (document.activeElement && document.activeElement !== document.body) return;
            [...root.querySelectorAll<HTMLInputElement>("input[type=radio]")]
              .find((entry) => entry.value === run.id)
              ?.focus();
          });
        },
      });
      const name = runDisplayName(run);
      return h(
        "div",
        { class: `compare-option${isReference ? " is-reference" : ""}` },
        h(
          "label",
          { class: "compare-option-label" },
          radio,
          h("span", { class: "compare-dot", style: `background:${run.color || "#8fa3b8"}` }),
          h("span", { class: "compare-name", text: name, title: name }),
        ),
        h("span", { class: "compare-time mono", text: fmtTime(run.raceTimeMs) }),
        iconButton("close", `Remove ${name} from the comparison`, {
          variant: "ghost",
          className: "compare-remove",
          onClick: () => app.actions.toggleCompare(run.id),
        }),
      );
    });

    root.append(
      h(
        "div",
        { class: "compare-inner" },
        h(
          "div",
          { class: "compare-current" },
          h("span", { class: "compare-kicker", text: "Compare" }),
          h("span", { class: "compare-dot", style: `background:${selected.color || "#4cc9f0"}` }),
          h("strong", { text: runDisplayName(selected), title: runDisplayName(selected) }),
        ),
        h("span", { class: "compare-vs", text: "with" }),
        h(
          "fieldset",
          { class: "compare-options" },
          h("legend", { class: "sr-only", text: "Reference run" }),
          ...options,
        ),
        differentMaps
          ? h("span", {
              class: "badge badge-warn",
              text: "different maps",
              title: "Distance-based deltas compare positions on different tracks.",
            })
          : null,
        h("button", {
          class: "link-button compare-clear",
          type: "button",
          text: "Clear",
          title: "Remove every comparison run",
          onclick: () => app.actions.clearCompare(),
        }),
      ),
    );
    root.hidden = false;
  }

  app.state.subscribe(render);
  render();
}
