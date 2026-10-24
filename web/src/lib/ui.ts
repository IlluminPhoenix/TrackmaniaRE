//! Small UI primitives shared by the panels. Keep button variants, icon-only
//! buttons and badges on these helpers so the CSS has one contract.

import { h } from "./dom";
import { icon, type IconName } from "./icons";

export type ButtonVariant = "default" | "primary" | "ghost" | "danger";

export interface ButtonOptions {
  label: string;
  variant?: ButtonVariant;
  icon?: IconName;
  title?: string;
  className?: string;
  disabled?: boolean;
  ariaLabel?: string;
  ariaPressed?: boolean;
  onClick?: (event: MouseEvent) => void;
}

function variantClass(variant: ButtonVariant | undefined): string {
  return variant && variant !== "default" ? ` btn-${variant}` : "";
}

export function button(options: ButtonOptions): HTMLButtonElement {
  const classes = `btn${variantClass(options.variant)}${options.className ? ` ${options.className}` : ""}`;
  const content = [
    options.icon ? icon(options.icon) : null,
    h("span", { class: "btn-label", text: options.label }),
  ];
  const el = h(
    "button",
    {
      type: "button",
      class: classes,
      title: options.title ?? options.label,
      "aria-label": options.ariaLabel,
      "aria-pressed": options.ariaPressed === undefined ? undefined : String(options.ariaPressed),
      disabled: options.disabled,
    },
    ...content,
  ) as HTMLButtonElement;
  if (options.onClick) el.addEventListener("click", options.onClick);
  return el;
}

export function iconButton(name: IconName, label: string, options: Omit<ButtonOptions, "label" | "icon"> = {}): HTMLButtonElement {
  return button({
    ...options,
    label: "",
    icon: name,
    className: `btn-icon${options.className ? ` ${options.className}` : ""}`,
    ariaLabel: options.ariaLabel ?? label,
    title: options.title ?? label,
  });
}

export type BadgeTone = "neutral" | "ok" | "warn" | "danger" | "accent";

export function badge(text: string, tone: BadgeTone = "neutral", title?: string): HTMLElement {
  const el = h("span", { class: `badge badge-${tone}`, text });
  if (title) el.title = title;
  return el;
}

export function emptyState(title: string, detail: string, action?: HTMLElement): HTMLElement {
  return h(
    "div",
    { class: "empty-state" },
    h("div", { class: "empty-state-title", text: title }),
    h("p", { class: "empty-state-detail", text: detail }),
    action ?? null,
  );
}
