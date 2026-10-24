// @vitest-environment jsdom
//
// The 3D controls are icon-only, so a broken icon name would silently render an
// empty button. Keep the icon set honest.

import { describe, expect, it } from "vitest";
import { icon } from "../src/lib/icons";

describe("inline icons", () => {
  it("draws a 24x24 svg that inherits the button colour", () => {
    const svg = icon("fit");
    expect(svg.getAttribute("viewBox")).toBe("0 0 24 24");
    expect(svg.getAttribute("stroke")).toBe("currentColor");
    expect(svg.querySelectorAll("path").length).toBeGreaterThan(1);
  });

  it("supports filled glyphs and circles", () => {
    const play = icon("play");
    expect(play.querySelector("path")?.getAttribute("fill")).toBe("currentColor");
    const follow = icon("follow");
    expect(follow.querySelector("circle")?.getAttribute("r")).toBe("7");
  });
});
