import { describe, expect, it } from "vitest";

import { fileNameStem, fmtBytes, fmtDelta, fmtMeters, fmtTime } from "../src/lib/format";

describe("fmtTime", () => {
  it("prints trackmania style times", () => {
    expect(fmtTime(0)).toBe("0:00.000");
    expect(fmtTime(62_345)).toBe("1:02.345");
    expect(fmtTime(3_723_456)).toBe("1:02:03.456");
  });

  it("handles missing and negative values", () => {
    expect(fmtTime(null)).toBe("–");
    expect(fmtTime(undefined)).toBe("–");
    expect(fmtTime(Number.NaN)).toBe("–");
    expect(fmtTime(-1_500)).toBe("-0:01.500");
  });
});

describe("fmtDelta", () => {
  it("signs and scales deltas", () => {
    expect(fmtDelta(345)).toBe("+0.345s");
    expect(fmtDelta(-1204)).toBe("-1.204s");
    expect(fmtDelta(0)).toBe("0.000s");
    expect(fmtDelta(null)).toBe("–");
  });
});

describe("other formatters", () => {
  it("formats metres and bytes", () => {
    expect(fmtMeters(12.34)).toBe("12.3 m");
    expect(fmtMeters(1234)).toBe("1.234 km");
    expect(fmtBytes(512)).toBe("512 B");
    expect(fmtBytes(2048)).toBe("2 kB");
    expect(fmtBytes(3 * 1024 * 1024)).toBe("3.0 MB");
  });

  it("strips replay and ghost extensions from file names", () => {
    expect(fileNameStem("run.Replay.Gbx")).toBe("run");
    expect(fileNameStem("run.Ghost.Gbx")).toBe("run");
    expect(fileNameStem("run.Gbx")).toBe("run");
    expect(fileNameStem("run")).toBe("run");
  });
});
