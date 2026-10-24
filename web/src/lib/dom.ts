//! Tiny DOM helpers. No framework, no innerHTML.

export type Child = Node | string | number | null | undefined | false;

type Attrs = Record<string, unknown>;

/**
 * Creates an element. `class`, `text`, `value`, `checked`, `disabled` and
 * `on<event>` are handled directly, everything else becomes an attribute.
 */
export function h(tag: string, attrs: Attrs = {}, ...children: Child[]): HTMLElement {
  const el = document.createElement(tag);
  for (const [key, value] of Object.entries(attrs)) {
    if (value === undefined || value === null || value === false) continue;
    if (key === "class") el.className = String(value);
    else if (key === "text") el.textContent = String(value);
    else if (key === "value") (el as HTMLInputElement).value = String(value);
    else if (key === "checked") (el as HTMLInputElement).checked = Boolean(value);
    else if (key === "disabled") (el as HTMLButtonElement).disabled = Boolean(value);
    else if (key.startsWith("on") && typeof value === "function") {
      el.addEventListener(key.slice(2).toLowerCase(), value as EventListener);
    } else {
      el.setAttribute(key, String(value));
    }
  }
  append(el, children);
  return el;
}

export function append(parent: Node, children: Child[]): void {
  for (const child of children) {
    if (child === null || child === undefined || child === false) continue;
    if (Array.isArray(child)) append(parent, child as Child[]);
    else if (typeof child === "object") parent.appendChild(child);
    else parent.appendChild(document.createTextNode(String(child)));
  }
}

export function clear(el: Element): void {
  el.replaceChildren();
}

export function text(tag: string, value: string, cls?: string): HTMLElement {
  return h(tag, cls ? { class: cls } : {}, value);
}

/** A labelled value row for detail lists. */
export function pair(label: string, value: Child): HTMLElement {
  return h("tr", {}, h("th", {}, label), h("td", {}, value));
}

export function on<K extends keyof HTMLElementEventMap>(
  el: HTMLElement,
  type: K,
  handler: (ev: HTMLElementEventMap[K]) => void,
): void {
  el.addEventListener(type, handler as EventListener);
}
