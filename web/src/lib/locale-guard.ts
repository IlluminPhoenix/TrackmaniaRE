//! Guards against a `navigator.language` that `Intl` rejects.
//!
//! Chromium on a system whose locale is not a valid BCP-47 tag (a plain
//! `LANG=C.UTF-8` becomes `en-US@posix`) reports that broken tag through
//! `navigator.language`. Libraries that hand the tag straight to `Intl` then
//! throw while their module is being evaluated, which takes the whole page
//! down: uPlot does exactly that with its legend number formatter. Replace the
//! tag with `en` before those modules load. Import this first, ahead of any
//! dependency that formats numbers with the browser's own locale.

function intlAccepts(tag: string): boolean {
  try {
    new Intl.NumberFormat(tag);
    return true;
  } catch {
    return false;
  }
}

/** Replaces an unusable `navigator.language` tag. Runs on import. */
export function guardNavigatorLanguage(): void {
  if (typeof navigator === "undefined" || typeof navigator.language !== "string") return;
  if (intlAccepts(navigator.language)) return;
  try {
    Object.defineProperty(Navigator.prototype, "language", {
      get: () => "en",
      configurable: true,
    });
  } catch {
    // Very old engines: nothing to do beyond letting the guard above run.
  }
}

guardNavigatorLanguage();
