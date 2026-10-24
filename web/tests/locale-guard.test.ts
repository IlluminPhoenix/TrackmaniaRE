import { afterEach, describe, expect, it, vi } from "vitest";

const original = Object.getOwnPropertyDescriptor(Navigator.prototype, "language");

afterEach(() => {
  if (original) Object.defineProperty(Navigator.prototype, "language", original);
  vi.resetModules();
});

describe("locale guard", () => {
  it("replaces a tag Intl rejects with one it accepts", async () => {
    Object.defineProperty(Navigator.prototype, "language", {
      get: () => "en-US@posix",
      configurable: true,
    });
    expect(() => new Intl.NumberFormat(navigator.language)).toThrow();
    vi.resetModules();
    await import("../src/lib/locale-guard");
    expect(() => new Intl.NumberFormat(navigator.language)).not.toThrow();
  });

  it("leaves a usable tag alone", async () => {
    Object.defineProperty(Navigator.prototype, "language", {
      get: () => "de-DE",
      configurable: true,
    });
    vi.resetModules();
    await import("../src/lib/locale-guard");
    expect(navigator.language).toBe("de-DE");
  });
});
