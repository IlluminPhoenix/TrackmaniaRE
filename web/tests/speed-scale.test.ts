import { describe, expect, it } from "vitest";
import { adaptiveSpeedRangeKmh } from "../src/lib/speed-scale";

const mps = (kmh: number[]): Float32Array => Float32Array.from(kmh, (value) => value / 3.6);

describe("adaptiveSpeedRangeKmh", () => {
  it("returns null when no group contains a finite speed", () => {
    expect(adaptiveSpeedRangeKmh([])).toBeNull();
    expect(adaptiveSpeedRangeKmh([mps([]), Float32Array.from([Number.NaN])])).toBeNull();
  });

  it("rounds the robust range outwards to 10 km/h", () => {
    const range = adaptiveSpeedRangeKmh([mps([0, 12, 36, 58, 71])]);
    expect(range).toEqual({ low: 0, high: 70 });
  });

  it("keeps a usable span for a run that never changes speed", () => {
    const range = adaptiveSpeedRangeKmh([mps([100, 100, 100])]);
    expect(range).toEqual({ low: 90, high: 110 });
  });

  it("ignores a single extreme sample when setting the top of the scale", () => {
    const normal = Array.from({ length: 99 }, (_, index) => index + 1);
    const range = adaptiveSpeedRangeKmh([mps([...normal, 2000])]);
    expect(range?.high).toBeLessThanOrEqual(110);
  });

  it("shares one range across every visible replay", () => {
    const range = adaptiveSpeedRangeKmh([mps([20, 40, 60]), mps([300, 320, 340])]);
    expect(range).toEqual({ low: 20, high: 340 });
  });

  it("clamps negative and non-finite values instead of producing a bad range", () => {
    const range = adaptiveSpeedRangeKmh([Float32Array.from([-5, Number.POSITIVE_INFINITY, 15])]);
    expect(range).toEqual({ low: 0, high: 60 });
  });
});
