//! 3D path view: one polyline per run, coloured by speed or input, with
//! respawn and wall-hit markers and a car marker that follows playback.
//!
//! three.js is loaded on demand so the rest of the page does not pay for it.
//! No map geometry is rendered: this is the recorded path only.
//!
//! Orientation comes from the recorded axis-angle samples:
//! `axis = (cos(pitch)·cos(heading), cos(pitch)·sin(heading), sin(pitch))`
//! with the stored angle doubled, local forward `+Z`, local up `+Y`. That
//! mapping was checked against the velocity direction of six real replays.

import { bestRun, runDisplayName, runRoleLabel, selectedRun, visibleRuns, type App } from "../app";
import { clear, h } from "../lib/dom";
import type * as ThreeNS from "three";
import { GRIP, SLIP_SLIDE_DEG } from "../lib/derive";
import { fmtTime } from "../lib/format";
import { icon, type IconName } from "../lib/icons";
import { PLAYBACK_RATES, formatRate } from "../lib/rates";
import { adaptiveSpeedRangeKmh, type SpeedRange } from "../lib/speed-scale";
import type { Derived, RunData, Samples } from "../lib/types";

type ThreeModule = typeof import("three");
type OrbitControls = import("three/examples/jsm/controls/OrbitControls.js").OrbitControls;

type ColorMode = "speed" | "steer" | "pedals" | "grip";

const RESPAWN_COLOR = 0xef476f;
const WALL_COLOR = 0xf8961e;
/** Grip colour mode: green gripping, amber sliding, blue airborne. */
const GRIP_COLORS = [0x43aa8b, 0xf9c74f, 0x4cc9f0];

/** Colour stops for speed, shared by the line and the legend ramp. */
const SPEED_STOPS = [
  { at: 0, color: 0x2b6cb0, css: "#2b6cb0" },
  { at: 0.34, color: 0x4cc9f0, css: "#4cc9f0" },
  { at: 0.67, color: 0xf9c74f, css: "#f9c74f" },
  { at: 1, color: 0xf72585, css: "#f72585" },
] as const;
const SPEED_GRADIENT = `linear-gradient(90deg, ${SPEED_STOPS.map(
  (stop) => `${stop.css} ${Math.round(stop.at * 100)}%`,
).join(", ")})`;

export function mountPath3d(root: HTMLElement, app: App): void {
  const host = h("div", { class: "view3d" });
  const modeSelect = h(
    "select",
    { "aria-label": "Colour the path by" },
    ...([
      ["speed", "Speed"],
      ["steer", "Steering"],
      ["pedals", "Pedals"],
      ["grip", "Grip"],
    ] as [ColorMode, string][]).map(([value, label]) => h("option", { value, text: label })),
  ) as HTMLSelectElement;
  const fitButton = h(
    "button",
    { class: "icon-btn", title: "Fit view: frame every visible run", "aria-label": "Fit view" },
    icon("fit"),
  );
  const followButton = h(
    "button",
    {
      class: "icon-btn",
      title: "Follow the car: orbit locked to the selected run",
      "aria-label": "Follow the car",
      "aria-pressed": "false",
    },
    icon("follow"),
  );
  const fullButton = h(
    "button",
    { class: "icon-btn", title: "Fullscreen (F)", "aria-label": "Fullscreen", "aria-pressed": "false" },
    icon("fullscreen"),
  );
  const modeControl = h("label", { class: "view3d-mode" }, h("span", { text: "Colour" }), modeSelect);
  const tools = h("div", { class: "view3d-tools" }, modeControl, fitButton, followButton, fullButton);
  const hint = h("div", { class: "hint", text: "drag to orbit · wheel to zoom · right-drag to pan" });
  const legend = h("div", { class: "legend" });
  const empty = h("div", { class: "view3d-empty", hidden: true });

  // Video-style playback bar, shown only in fullscreen where the bottom
  // transport is out of reach.
  const restartButton = h(
    "button",
    { class: "player-btn", title: "Restart (Home)", "aria-label": "Restart" },
    icon("restart"),
  ) as HTMLButtonElement;
  const playButton = h(
    "button",
    { class: "player-btn", title: "Play/pause (Space)", "aria-label": "Play" },
    icon("play"),
  ) as HTMLButtonElement;
  const timeLabel = h("span", { class: "player-time mono", text: "0:00.000" });
  const totalLabel = h("span", { class: "player-time mono muted", text: "/ 0:00.000" });
  const timeline = h("input", {
    type: "range",
    class: "slider timeline player-range",
    min: "0",
    max: "1000",
    step: "10",
    value: "0",
    "aria-label": "Playback position",
  }) as HTMLInputElement;
  const rateSelect = h(
    "select",
    { class: "player-rate", "aria-label": "Playback speed", title: "Playback speed" },
    ...PLAYBACK_RATES.map((rate) => h("option", { value: String(rate), text: formatRate(rate) })),
  ) as HTMLSelectElement;
  rateSelect.value = "1";
  rateSelect.addEventListener("change", () => app.actions.setRate(Number(rateSelect.value)));
  const player = h(
    "div",
    { class: "view3d-player" },
    restartButton,
    playButton,
    timeLabel,
    timeline,
    totalLabel,
    rateSelect,
  );

  host.append(tools, hint, legend, empty, player);
  root.append(host);

  let THREE: ThreeModule | null = null;
  let OrbitControlsCtor: (new (camera: ThreeNS.Camera, dom: HTMLElement) => OrbitControls) | null = null;
  let renderer: ThreeNS.WebGLRenderer | null = null;
  let scene: ThreeNS.Scene | null = null;
  let camera: ThreeNS.PerspectiveCamera | null = null;
  let controls: OrbitControls | null = null;
  let runsGroup: ThreeNS.Group | null = null;
  let grid: ThreeNS.GridHelper | null = null;
  let axes: ThreeNS.AxesHelper | null = null;
  let needsRender = true;
  let follow = false;
  let mode: ColorMode = "speed";
  let speedRange: SpeedRange | null = null;
  const carMarkers = new Map<string, ThreeNS.Group>();

  modeSelect.addEventListener("change", () => {
    mode = modeSelect.value as ColorMode;
    // Recolour only: the camera stays where the user put it.
    rebuildRuns(false);
  });
  fitButton.addEventListener("click", () => {
    if (follow) setFollow(false);
    fitView();
  });
  followButton.addEventListener("click", () => {
    setFollow(!follow);
  });
  fullButton.addEventListener("click", toggleFullscreen);
  restartButton.addEventListener("click", () => app.actions.seek(0));
  playButton.addEventListener("click", () => app.actions.togglePlay());
  timeline.addEventListener("input", () => app.actions.seek(Number(timeline.value)));
  document.addEventListener("fullscreenchange", () => {
    const on = document.fullscreenElement === host;
    fullButton.setAttribute("aria-pressed", String(on));
    setButtonIcon(fullButton, on ? "exit-fullscreen" : "fullscreen");
    fullButton.title = on ? "Exit fullscreen (Esc)" : "Fullscreen (F)";
    needsRender = true;
  });
  document.addEventListener("keydown", (event) => {
    const target = event.target as HTMLElement | null;
    if (target && /^(INPUT|TEXTAREA|SELECT)$/.test(target.tagName)) return;
    if (event.key === "Escape") {
      // Browsers normally handle this themselves; make sure it works anywhere.
      if (document.fullscreenElement === host) void document.exitFullscreen().catch(() => {});
      return;
    }
    if (event.key !== "f" && event.key !== "F") return;
    if (event.ctrlKey || event.metaKey || event.altKey) return;
    // Keep the global shortcut rule: controls own their keyboard input.
    if (target?.closest("input, textarea, select, button, a, [contenteditable='true']")) return;
    // Only when the 3D view is on screen; the other tabs have no canvas.
    if (!host.getClientRects().length) return;
    event.preventDefault();
    toggleFullscreen();
  });

  function toggleFullscreen(): void {
    if (document.fullscreenElement === host) {
      void document.exitFullscreen();
      return;
    }
    if (!host.requestFullscreen) {
      app.actions.setStatus("Fullscreen is not available in this browser.");
      return;
    }
    void host.requestFullscreen().catch(() => {
      app.actions.setStatus("Fullscreen was refused by the browser.");
    });
  }

  /** Swaps an icon button's glyph; the `data-icon` marker avoids rebuilding it. */
  function setButtonIcon(button: HTMLElement, name: IconName): void {
    if (button.dataset.icon === name) return;
    button.dataset.icon = name;
    clear(button);
    button.append(icon(name));
  }

  function setFollow(on: boolean): void {
    follow = on;
    followButton.setAttribute("aria-pressed", String(on));
    followButton.title = on ? "Following the car — click to stop" : "Follow the car: orbit locked to the selected run";
    hint.textContent = on
      ? "drag to orbit · wheel to zoom · following the car"
      : "drag to orbit · wheel to zoom · right-drag to pan";
    if (controls) controls.enablePan = !on;
    if (on) frameCar();
    else needsRender = true;
  }

  void (async () => {
    try {
      const [three, controlsModule] = await Promise.all([
        import("three"),
        import("three/examples/jsm/controls/OrbitControls.js"),
      ]);
      THREE = three;
      OrbitControlsCtor = controlsModule.OrbitControls;
      initScene();
      rebuildRuns();
    } catch (err) {
      showUnavailable(err);
    }
  })();

  function showUnavailable(err: unknown): void {
    clear(host);
    host.append(
      h("div", { class: "panel-unavailable" },
        icon("warning", 18),
        h("strong", { text: "The 3D view could not start in this browser." }),
        h("p", { class: "muted small", text: `WebGL is unavailable. ${err instanceof Error ? err.message : String(err)}` }),
      ),
    );
  }

  function renderEmpty(): void {
    const selected = selectedRun(app);
    const entries = visibleRuns(app).filter((entry) => entry.run.samples && entry.run.samples.times.length > 1);
    clear(empty);
    if (entries.length) {
      empty.hidden = true;
      return;
    }
    empty.hidden = false;
    const sampleRuns = app.state.get().runs.filter((run) => run.samples && run.samples.times.length > 1);
    const fallback = bestRun(sampleRuns);
    empty.append(
      icon("info", 20),
      h("strong", { text: selected ? "No 3D path for this run" : "Select a run to draw its path" }),
      h("p", {
        text: selected
          ? "This run has no vehicle samples, so there is no recorded position to draw."
          : "Runs with vehicle samples appear here. Input-only ghosts have no position data.",
      }),
    );
    if (fallback && fallback.id !== selected?.id) {
      empty.append(
        h("button", {
          class: "btn",
          type: "button",
          text: `Show ${runDisplayName(fallback)} (has samples)`,
          onclick: () => app.actions.select(fallback.id),
        }),
      );
    }
  }

  function initScene(): void {
    const T = THREE;
    const ControlsCtor = OrbitControlsCtor;
    if (!T || !ControlsCtor) return;
    try {
      renderer = new T.WebGLRenderer({ antialias: true, powerPreference: "high-performance" });
    } catch (err) {
      showUnavailable(err);
      return;
    }
    renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 2));
    renderer.setSize(host.clientWidth || 640, host.clientHeight || 440);
    host.append(renderer.domElement);
    scene = new T.Scene();
    scene.background = new T.Color(0x080b0f);
    // The car model is lit; the path lines and markers are unlit basics.
    scene.add(new T.HemisphereLight(0xffffff, 0x1a2633, 1.5));
    const sun = new T.DirectionalLight(0xffffff, 1.8);
    sun.position.set(1, 2, 1.2);
    scene.add(sun);
    camera = new T.PerspectiveCamera(60, (host.clientWidth || 640) / (host.clientHeight || 440), 0.5, 200_000);
    camera.position.set(100, 80, 100);
    controls = new ControlsCtor(camera, renderer.domElement);
    controls.enableDamping = true;
    controls.dampingFactor = 0.12;
    controls.enablePan = !follow;
    controls.addEventListener("change", () => (needsRender = true));
    renderer.setAnimationLoop(() => {
      controls?.update();
      if (needsRender && renderer && scene && camera) {
        renderer.render(scene, camera);
        needsRender = false;
      }
    });
    new ResizeObserver(() => {
      if (!renderer || !camera) return;
      const width = host.clientWidth;
      const height = host.clientHeight;
      if (width <= 0 || height <= 0) return;
      renderer.setSize(width, height);
      camera.aspect = width / height;
      camera.updateProjectionMatrix();
      needsRender = true;
    }).observe(host);
  }

  function disposeRuns(): void {
    const T = THREE;
    if (!runsGroup || !scene || !T) return;
    scene.remove(runsGroup);
    runsGroup.traverse((object) => {
      const mesh = object as ThreeNS.Mesh;
      mesh.geometry?.dispose();
      const material = mesh.material as ThreeNS.Material | ThreeNS.Material[] | undefined;
      if (Array.isArray(material)) material.forEach((entry) => entry.dispose());
      else material?.dispose();
    });
    for (const marker of carMarkers.values()) {
      scene.remove(marker);
      marker.traverse((object) => {
        const mesh = object as ThreeNS.Mesh;
        mesh.geometry?.dispose();
        const material = mesh.material as ThreeNS.Material | ThreeNS.Material[] | undefined;
        if (Array.isArray(material)) material.forEach((entry) => entry.dispose());
        else material?.dispose();
      });
    }
    carMarkers.clear();
    runsGroup = null;
  }

  function rebuildRuns(refit = true): void {
    renderEmpty();
    if (!THREE || !scene) {
      renderLegend();
      return;
    }
    disposeRuns();
    const entries = visibleRuns(app).filter((entry) => entry.run.samples && entry.run.samples.times.length > 1);
    const hasSpeedPath = entries.some((entry) => !entry.isReference);
    speedRange = mode === "speed" && hasSpeedPath
      ? adaptiveSpeedRangeKmh(entries.map(({ run }) => (run.samples as Samples).speeds))
      : null;
    runsGroup = new THREE.Group();
    scene.add(runsGroup);
    for (const { run, isReference } of entries) {
      const samples = run.samples as Samples;
      const group = new THREE.Group();
      const count = samples.times.length;
      const positions = new Float32Array(count * 3);
      positions.set(samples.positions.subarray(0, count * 3));
      const colors = new Float32Array(count * 3);
      if (isReference) {
        const color = new THREE.Color(run.color || "#8fa3b8");
        for (let i = 0; i < count; i++) {
          colors[3 * i] = color.r;
          colors[3 * i + 1] = color.g;
          colors[3 * i + 2] = color.b;
        }
      } else {
        fillColors(colors, samples, mode, run.derived, speedRange);
      }
      const geometry = new THREE.BufferGeometry();
      geometry.setAttribute("position", new THREE.BufferAttribute(positions, 3));
      geometry.setAttribute("color", new THREE.BufferAttribute(colors, 3));
      const material = new THREE.LineBasicMaterial({
        vertexColors: true,
        // Both lines live in the transparent pass so the selected run can be
        // drawn after the dimmed references. Otherwise the reference overlay
        // tints the speed colours into one muddy run colour.
        transparent: true,
        opacity: isReference ? 0.4 : 1,
        depthWrite: !isReference,
      });
      const line = new THREE.Line(geometry, material);
      line.renderOrder = isReference ? 0 : 1;
      group.add(line);
      if (!isReference) group.add(buildMarkers(run, samples));
      runsGroup.add(group);
    }
    buildCarMarkers(entries.map((entry) => entry.run));
    if (refit) {
      if (follow) frameCar();
      else fitView();
    }
    updateMarkers(app.clock.get().timeMs);
    renderLegend();
  }

  function fillColors(
    colors: Float32Array,
    samples: Samples,
    colorMode: ColorMode,
    derived: Derived | null,
    range: SpeedRange | null,
  ): void {
    const T = THREE as ThreeModule;
    const color = new T.Color();
    const n = samples.times.length;
    for (let i = 0; i < n; i++) {
      if (colorMode === "speed") {
        if (range) speedRamp(color, samples.speeds[i] * 3.6, range);
        else color.setHex(0x8fa3b8);
      } else if (colorMode === "steer") {
        const steer = Math.max(-1, Math.min(1, samples.steer[i]));
        if (steer >= 0) color.setHex(0x8fa3b8).lerp(new T.Color(0xf72585), steer);
        else color.setHex(0x8fa3b8).lerp(new T.Color(0x4cc9f0), -steer);
      } else if (colorMode === "grip") {
        color.setHex(GRIP_COLORS[derived?.gripStates[i] ?? GRIP]);
      } else {
        color.setHex(samples.brake[i] >= 0.5 ? 0xef476f : samples.gas[i] >= 0.5 ? 0x43aa8b : 0x46607a);
      }
      colors[3 * i] = color.r;
      colors[3 * i + 1] = color.g;
      colors[3 * i + 2] = color.b;
    }
  }

  function speedRamp(out: ThreeNS.Color, kmh: number, range: SpeedRange): void {
    const T = THREE as ThreeModule;
    const span = Math.max(1, range.high - range.low);
    const t = Math.max(0, Math.min(1, (kmh - range.low) / span));
    let index = 1;
    while (index < SPEED_STOPS.length - 1 && t > SPEED_STOPS[index].at) index++;
    const from = SPEED_STOPS[index - 1];
    const to = SPEED_STOPS[index];
    const local = Math.max(0, Math.min(1, (t - from.at) / (to.at - from.at)));
    out.setHex(from.color).lerp(new T.Color(to.color), local);
  }

  function buildMarkers(run: RunData, samples: Samples): ThreeNS.Object3D {
    const T = THREE as ThreeModule;
    const group = new T.Group();
    const respawnPoints: number[] = [];
    const wallPoints: number[] = [];
    for (const time of run.derived?.respawnTimes ?? []) {
      const index = indexOfTime(samples.times, time);
      if (index < 0) continue;
      respawnPoints.push(samples.positions[3 * index], samples.positions[3 * index + 1], samples.positions[3 * index + 2]);
    }
    for (const hit of run.derived?.wallHits ?? []) {
      wallPoints.push(samples.positions[3 * hit.index], samples.positions[3 * hit.index + 1], samples.positions[3 * hit.index + 2]);
    }
    const add = (points: number[], hex: number, size: number) => {
      if (!points.length) return;
      const geometry = new T.BufferGeometry();
      geometry.setAttribute("position", new T.Float32BufferAttribute(points, 3));
      group.add(new T.Points(geometry, new T.PointsMaterial({ color: hex, size, sizeAttenuation: true })));
    };
    add(respawnPoints, RESPAWN_COLOR, 8);
    add(wallPoints, WALL_COLOR, 5);
    return group;
  }

  function buildCarMarkers(runs: RunData[]): void {
    if (!THREE || !scene) return;
    for (const run of runs) {
      const marker = buildCar(run.color || "#ffffff");
      marker.visible = false;
      carMarkers.set(run.id, marker);
      scene.add(marker);
    }
  }

  /**
   * A schematic car, in world metres: 4.0 m long, 1.9 m wide, +Z forward,
   * +Y up, wheels below the body, headlights at the nose. Not the game model;
   * it exists so the recorded orientation is readable at a glance. Its size is
   * constant, so at a fitted whole-track view only the small dot shows.
   */
  function buildCar(color: string): ThreeNS.Group {
    const T = THREE as ThreeModule;
    const car = new T.Group();
    const body = new T.MeshStandardMaterial({ color: new T.Color(color), roughness: 0.5, metalness: 0.25 });
    const glass = new T.MeshStandardMaterial({ color: 0x0e1720, roughness: 0.25, metalness: 0.4 });
    const tyre = new T.MeshStandardMaterial({ color: 0x0a0d10, roughness: 0.95 });
    const headlight = new T.MeshBasicMaterial({ color: 0xeaf6ff });
    const taillight = new T.MeshBasicMaterial({ color: 0xef476f });
    const add = (geometry: ThreeNS.BufferGeometry, material: ThreeNS.Material, x: number, y: number, z: number): void => {
      const mesh = new T.Mesh(geometry, material);
      mesh.position.set(x, y, z);
      car.add(mesh);
    };
    add(new T.BoxGeometry(1.9, 0.5, 4.0), body, 0, 0.55, 0);
    add(new T.BoxGeometry(1.5, 0.46, 1.6), glass, 0, 1.0, -0.3);
    add(new T.BoxGeometry(1.8, 0.07, 0.36), body, 0, 1.16, -1.9);
    add(new T.BoxGeometry(0.09, 0.32, 0.5), body, 0.65, 1.0, -1.8);
    add(new T.BoxGeometry(0.09, 0.32, 0.5), body, -0.65, 1.0, -1.8);
    for (const x of [-0.98, 0.98]) {
      for (const z of [-1.3, 1.3]) {
        const wheel = new T.CylinderGeometry(0.38, 0.38, 0.3, 16);
        wheel.rotateZ(Math.PI / 2);
        add(wheel, tyre, x, 0.38, z);
      }
    }
    add(new T.BoxGeometry(0.36, 0.12, 0.06), headlight, 0.6, 0.63, 2.01);
    add(new T.BoxGeometry(0.36, 0.12, 0.06), headlight, -0.6, 0.63, 2.01);
    add(new T.BoxGeometry(0.36, 0.12, 0.06), taillight, 0.6, 0.63, -2.01);
    add(new T.BoxGeometry(0.36, 0.12, 0.06), taillight, -0.6, 0.63, -2.01);
    // Constant pixel size, so the playhead stays visible in a fitted view.
    const dot = new T.BufferGeometry();
    dot.setAttribute("position", new T.Float32BufferAttribute([0, 0.8, 0], 3));
    const dotPoints = new T.Points(
      dot,
      new T.PointsMaterial({ color: new T.Color(color), size: 5, sizeAttenuation: false, depthWrite: false }),
    );
    dotPoints.renderOrder = 2;
    car.add(dotPoints);
    return car;
  }

  function indexOfTime(times: Float64Array, t: number): number {
    if (!times.length) return -1;
    let lo = 0;
    let hi = times.length - 1;
    if (t <= times[0]) return 0;
    if (t >= times[hi]) return hi;
    while (lo + 1 < hi) {
      const mid = (lo + hi) >> 1;
      if (times[mid] <= t) lo = mid;
      else hi = mid;
    }
    return lo;
  }

  /** Interpolated position and rotation of one run at `timeMs`. */
  function samplePose(run: RunData, timeMs: number): { position: ThreeNS.Vector3; quaternion: ThreeNS.Quaternion } | null {
    const T = THREE as ThreeModule;
    const samples = run.samples;
    if (!T || !samples) return null;
    const times = samples.times;
    const i = indexOfTime(times, timeMs);
    if (i < 0) return null;
    const j = Math.min(i + 1, times.length - 1);
    const t0 = times[i];
    const t1 = times[j];
    const alpha = t1 > t0 ? Math.max(0, Math.min(1, (timeMs - t0) / (t1 - t0))) : 0;
    const position = new T.Vector3(
      lerp(samples.positions[3 * i], samples.positions[3 * j], alpha),
      lerp(samples.positions[3 * i + 1], samples.positions[3 * j + 1], alpha),
      lerp(samples.positions[3 * i + 2], samples.positions[3 * j + 2], alpha),
    );
    const qa = new T.Quaternion();
    const qb = new T.Quaternion();
    rotationQuaternion(samples.rotations, i, qa);
    rotationQuaternion(samples.rotations, j, qb);
    return { position, quaternion: qa.slerp(qb, alpha) };
  }

  function updateMarkers(timeMs: number): void {
    if (!THREE || !carMarkers.size) return;
    for (const { run } of visibleRuns(app)) {
      const marker = carMarkers.get(run.id);
      if (!marker) continue;
      const pose = samplePose(run, timeMs);
      if (!pose) {
        marker.visible = false;
        continue;
      }
      marker.visible = true;
      marker.position.copy(pose.position);
      marker.quaternion.copy(pose.quaternion);
    }
    needsRender = true;
  }

  function rotationQuaternion(rotations: Float32Array, i: number, out: ThreeNS.Quaternion): void {
    const T = THREE as ThreeModule;
    const angle = rotations[3 * i] * 2;
    const heading = rotations[3 * i + 1];
    const pitch = rotations[3 * i + 2];
    const x = Math.cos(pitch) * Math.cos(heading);
    const y = Math.cos(pitch) * Math.sin(heading);
    const z = Math.sin(pitch);
    const length = Math.hypot(x, y, z);
    if (!Number.isFinite(angle) || length < 1e-6) {
      out.identity();
      return;
    }
    out.setFromAxisAngle(new T.Vector3(x / length, y / length, z / length), angle);
  }

  function pathBox(): ThreeNS.Box3 | null {
    const T = THREE;
    if (!T) return null;
    const box = new T.Box3();
    for (const { run } of visibleRuns(app)) {
      const samples = run.samples;
      if (!samples) continue;
      const positions = samples.positions;
      for (let i = 0; i < samples.times.length; i++) {
        box.expandByPoint(new T.Vector3(positions[3 * i], positions[3 * i + 1], positions[3 * i + 2]));
      }
    }
    return box.isEmpty() ? null : box;
  }

  function fitView(): void {
    const T = THREE;
    if (!T || !camera || !controls || !scene) return;
    const box = pathBox();
    if (!box) return;
    const center = box.getCenter(new T.Vector3());
    const size = box.getSize(new T.Vector3());
    const diagonal = Math.max(size.length(), 20);
    camera.position.set(center.x + diagonal * 0.58, center.y + diagonal * 0.5, center.z + diagonal * 0.58);
    camera.near = Math.max(0.5, diagonal / 2000);
    camera.far = diagonal * 20;
    camera.updateProjectionMatrix();
    controls.target.copy(center);
    controls.update();
    if (grid) scene.remove(grid);
    if (axes) scene.remove(axes);
    grid = new T.GridHelper(Math.max(diagonal, 50), 20, 0x1f2c38, 0x16202a);
    grid.position.set(center.x, box.min.y, center.z);
    scene.add(grid);
    axes = new T.AxesHelper(Math.max(diagonal * 0.08, 5));
    axes.position.set(box.min.x, box.min.y, box.min.z);
    scene.add(axes);
    needsRender = true;
  }

  /** Move the camera near the selected car, keeping the viewing direction. */
  function frameCar(): void {
    const T = THREE;
    if (!T || !camera || !controls) return;
    const run = selectedRun(app);
    if (!run?.samples) return;
    const pose = samplePose(run, app.clock.get().timeMs);
    if (!pose) return;
    const box = pathBox();
    const diagonal = box ? Math.max(box.getSize(new T.Vector3()).length(), 20) : 20;
    const distance = Math.max(12, Math.min(70, diagonal * 0.02));
    const direction = new T.Vector3().subVectors(camera.position, controls.target);
    if (direction.lengthSq() < 1e-6) direction.set(1, 0.7, 1);
    direction.normalize();
    controls.target.copy(pose.position);
    camera.position.copy(pose.position).addScaledVector(direction, distance);
    controls.update();
    needsRender = true;
  }

  /** Keep the orbit target on the car while playback moves it. */
  function followCar(): void {
    const T = THREE;
    if (!follow || !T || !camera || !controls) return;
    const run = selectedRun(app);
    if (!run?.samples) return;
    const pose = samplePose(run, app.clock.get().timeMs);
    if (!pose) return;
    const delta = new T.Vector3().subVectors(pose.position, controls.target);
    if (delta.lengthSq() < 1e-8) return;
    controls.target.copy(pose.position);
    camera.position.add(delta);
    controls.update();
    needsRender = true;
  }

  function lerp(a: number, b: number, alpha: number): number {
    return a + (b - a) * alpha;
  }

  function renderLegend(): void {
    clear(legend);
    if (mode === "speed" && speedRange) {
      legend.append(
        h(
          "span",
          { class: "key speed-legend", title: "Adaptive to the speed range of the visible runs" },
          h("span", { class: "speed-ramp", style: `background:${SPEED_GRADIENT}`, "aria-hidden": "true" }),
          h("span", { class: "legend-label", text: `Speed ${speedRange.low}–${speedRange.high} km/h` }),
        ),
      );
    }
    for (const { run } of visibleRuns(app)) {
      if (!run.samples || run.samples.times.length < 2) continue;
      legend.append(
        h(
          "span",
          { class: "key" },
          h("span", { class: "dot", style: `background:${run.color || "#8fa3b8"}` }),
          runRoleLabel(run, app),
        ),
      );
    }
    if (mode === "grip") {
      for (const [color, label] of [
        ["#43aa8b", "grip"],
        ["#f9c74f", `sliding (> ${SLIP_SLIDE_DEG}°)`],
        ["#4cc9f0", "airborne"],
      ] as const) {
        legend.append(h("span", { class: "key" }, h("span", { class: "dot", style: `background:${color}` }), label));
      }
    }
    legend.append(
      h("span", { class: "key" }, h("span", { class: "dot", style: "background:#ef476f" }), "respawn"),
      h("span", { class: "key" }, h("span", { class: "dot", style: "background:#f8961e" }), "wall hit"),
    );
  }

  /** Mirrors the sidebar transport into the fullscreen player bar. */
  function renderPlayer(): void {
    const run = selectedRun(app);
    const playable = Boolean(run?.samples) || Boolean(run?.inputs);
    const duration = Math.max(run?.durationMs ?? 0, 1);
    const { timeMs, playing, rate } = app.clock.get();
    timeLabel.textContent = fmtTime(timeMs);
    totalLabel.textContent = `/ ${fmtTime(run?.durationMs ?? 0)}`;
    timeline.max = String(Math.round(duration));
    timeline.disabled = !playable;
    playButton.disabled = !playable;
    restartButton.disabled = !playable;
    rateSelect.disabled = !playable;
    if (document.activeElement !== timeline) timeline.value = String(Math.round(timeMs));
    timeline.style.setProperty("--pct", String(Math.min(1, timeMs / duration)));
    if (document.activeElement !== rateSelect) {
      const value = String(rate);
      if (![...rateSelect.options].some((option) => option.value === value)) {
        rateSelect.append(h("option", { value, text: formatRate(rate) }));
      }
      rateSelect.value = value;
    }
    setButtonIcon(playButton, playing ? "pause" : "play");
    playButton.setAttribute("aria-label", playing ? "Pause" : "Play");
    playButton.title = playing ? "Pause (Space)" : "Play (Space)";
  }

  app.state.subscribe(() => {
    rebuildRuns();
    renderPlayer();
  });
  app.clock.subscribe((clock) => {
    if (selectedRun(app)?.samples) updateMarkers(clock.timeMs);
    if (follow) followCar();
    renderPlayer();
  });
  renderLegend();
  renderPlayer();
  renderEmpty();
}
