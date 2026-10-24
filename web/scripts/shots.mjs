#!/usr/bin/env node
// Repeatable screenshot pass for the web UI.
//
// Starts Vite, loads the fixture replays through the real file input, drives the
// shell and writes screenshots to web/.shots/ (gitignored). Run:
//
//   npm run shots
//
// Set SHOTS_BASE_URL to reuse a running dev server, SHOTS_OUT to choose a
// different output directory, and SHOTS_CHROMIUM to point at a Chromium binary
// when Playwright's own browser download is not installed. Chromium may need
// extra shared libraries on minimal Linux images; set LD_LIBRARY_PATH before
// running in that case.
//
// Screenshots are viewport-sized on purpose. A full-page capture moves sticky
// headers and fixed transport controls away from the positions a user sees, so
// anything below the fold is captured by scrolling to it instead.

import { spawn } from "node:child_process";
import { mkdir, rm } from "node:fs/promises";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright";

const webRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const fixtureDir = resolve(webRoot, "..", "crates/trackmaniare/tests/fixtures");
const outDir = process.env.SHOTS_OUT ? resolve(process.env.SHOTS_OUT) : join(webRoot, ".shots");
const port = Number(process.env.SHOTS_PORT ?? 4179);
const baseUrl = process.env.SHOTS_BASE_URL ?? `http://127.0.0.1:${port}`;
const merged = join(fixtureDir, "tm2020-merged-record.Replay.Gbx");
const short = join(fixtureDir, "tm2020-short.Replay.Gbx");

const viewports = [
  { name: "1440", width: 1440, height: 1000 },
  { name: "1024", width: 1024, height: 900 },
  { name: "390", width: 390, height: 844, isMobile: true, hasTouch: true },
];

const consoleErrors = [];
const pageErrors = [];

async function waitForServer(url, timeoutMs = 30000) {
  const started = Date.now();
  while (Date.now() - started < timeoutMs) {
    try {
      const response = await fetch(url);
      if (response.ok) return;
    } catch {
      // Vite is still starting.
    }
    await new Promise((resolve) => setTimeout(resolve, 200));
  }
  throw new Error(`Timed out waiting for ${url}`);
}

async function dismissToasts(page) {
  await page.evaluate(() => {
    for (const button of document.querySelectorAll("#status .toast-close")) {
      button instanceof HTMLElement && button.click();
    }
  });
  await page.waitForFunction(() => !document.querySelector("#status .toast"));
}

async function waitForRuns(page, count) {
  await page.waitForFunction(
    (expected) => document.querySelectorAll(".run-row").length === expected,
    count,
  );
  await page.waitForFunction(() => document.querySelectorAll(".run-row.selected").length === 1);
}

async function selectRun(page, text) {
  const row = page.locator(".run-row").filter({ hasText: text });
  await row.locator(".run-select").click();
  await page.waitForFunction(
    (needle) => [...document.querySelectorAll(".run-row.selected")].some((el) => el.textContent?.includes(needle)),
    text,
  );
}

async function drawerIsOpen(page) {
  return page.locator("#runs-drawer").evaluate((el) => el.classList.contains("open"));
}

async function openDrawerIfNeeded(page) {
  const toggle = page.locator("#runs-toggle");
  if (await toggle.isVisible()) {
    if (!(await drawerIsOpen(page))) await toggle.click();
    await page.locator("#runs-drawer.open").waitFor();
  }
}

async function closeDrawerIfNeeded(page) {
  if (await page.locator("#runs-toggle").isVisible()) {
    if (await drawerIsOpen(page)) {
      await page.locator("#runs-close").click();
      await page.waitForFunction(() => !document.getElementById("runs-drawer")?.classList.contains("open"));
    }
  }
}

async function compareRun(page, text) {
  await openDrawerIfNeeded(page);
  const row = page.locator(".run-row").filter({ hasText: text });
  const checkbox = row.locator("input[type=checkbox]");
  if (!(await checkbox.isChecked())) await checkbox.check();
  await page.locator("#compare-bar:not([hidden])").waitFor();
  await closeDrawerIfNeeded(page);
  await page.waitForTimeout(180);
}

async function loadFile(page, path, expectedRuns) {
  await page.locator("#file-input").setInputFiles(path);
  await waitForRuns(page, expectedRuns);
  await page.waitForFunction(() => document.getElementById("status")?.textContent?.includes("Loaded"));
  await page.waitForTimeout(250);
}

async function shot(page, viewport, name, delay = 220) {
  await page.waitForTimeout(delay);
  const file = join(outDir, `${viewport}-${name}.png`);
  await page.screenshot({ path: file });
  console.log(`  wrote ${file.replace(`${webRoot}/`, "")}`);
}

async function isScrollable(page) {
  return page.evaluate(() => document.documentElement.scrollHeight > window.innerHeight + 4);
}

async function scrollToTop(page) {
  await page.evaluate(() => window.scrollTo(0, 0));
}

async function scrollToBottom(page) {
  if (!(await isScrollable(page))) return false;
  await page.evaluate(() => window.scrollTo(0, document.documentElement.scrollHeight));
  await page.waitForTimeout(120);
  return true;
}

async function scrollToPanel(page, id) {
  await page.evaluate((panelId) => {
    const panel = document.getElementById(panelId);
    if (!panel) return;
    const root = getComputedStyle(document.documentElement);
    const header = Number.parseFloat(root.getPropertyValue("--header-h")) || 56;
    const top = panel.getBoundingClientRect().top + window.scrollY - header - 49;
    window.scrollTo(0, Math.max(0, top));
  }, id);
  await page.waitForTimeout(120);
}

async function shotAtBottom(page, viewport, name) {
  if (!(await scrollToBottom(page))) return false;
  await shot(page, viewport, name);
  await scrollToTop(page);
  return true;
}

async function shotAtPanel(page, viewport, id, name) {
  await scrollToPanel(page, id);
  await shot(page, viewport, name);
  await scrollToTop(page);
}

async function runViewport(browser, viewport) {
  const context = await browser.newContext({
    viewport: { width: viewport.width, height: viewport.height },
    deviceScaleFactor: 1,
    isMobile: Boolean(viewport.isMobile),
    hasTouch: Boolean(viewport.hasTouch),
    reducedMotion: "reduce",
  });
  const page = await context.newPage();
  page.setDefaultTimeout(15000);
  page.on("console", (message) => {
    if (message.type() === "error") consoleErrors.push(`[${viewport.name}] ${message.text()}`);
  });
  page.on("pageerror", (error) => {
    pageErrors.push(`[${viewport.name}] ${error.message}`);
  });

  console.log(`\n== ${viewport.name}px ==`);

  await page.goto(baseUrl, { waitUntil: "networkidle" });
  await page.locator("#workspace-empty").waitFor();
  await shot(page, viewport.name, "01-empty");

  await loadFile(page, merged, 2);
  await shot(page, viewport.name, "02-merged-record-overview");
  await shot(page, viewport.name, "02b-loaded-toast");
  await dismissToasts(page);

  // The first ghost has inputs but no vehicle samples. On narrow screens the
  // runs rail is a drawer, so this also exercises its open/close path.
  await openDrawerIfNeeded(page);
  if (viewport.name === "390") await shot(page, viewport.name, "03a-runs-drawer");
  await selectRun(page, "no samples");
  await closeDrawerIfNeeded(page);
  await scrollToTop(page);
  await shot(page, viewport.name, "03-merged-ghost-overview");

  // Confirm the 3D tab's empty state when the selected run has no samples.
  await page.locator("#tabbutton-3d").click();
  await page.waitForTimeout(550);
  await shot(page, viewport.name, "03b-3d-no-samples", 450);
  await page.locator("#tabbutton-overview").click();
  await scrollToTop(page);

  // A second file supplies a run that has both inputs and samples for the
  // chart-heavy tabs and the comparison.
  await loadFile(page, short, 3);
  await dismissToasts(page);
  await scrollToTop(page);
  await shot(page, viewport.name, "04-short-overview");
  await shotAtBottom(page, viewport.name, "04b-short-overview-bottom");

  await compareRun(page, "391 samples");
  await scrollToTop(page);
  await shot(page, viewport.name, "05-comparison-overview");
  await shotAtBottom(page, viewport.name, "05b-comparison-overview-bottom");

  // Add the input-only ghost as a second comparison entry: the bar must wrap,
  // the radio group must stay usable and the 3D legend must skip runs it cannot
  // draw.
  await compareRun(page, "no samples");
  await scrollToTop(page);
  await shot(page, viewport.name, "05c-comparison-multi");

  await page.locator("#tabbutton-3d").click();
  await scrollToTop(page);
  await page.waitForTimeout(700);
  await shot(page, viewport.name, "06-3d-path");
  if (await isScrollable(page)) {
    await shotAtPanel(page, viewport.name, "panel-live", "06b-3d-live");
  }

  const fullscreen = page.locator("#panel-path button[aria-label='Fullscreen']");
  if (await fullscreen.isVisible()) {
    await fullscreen.click();
    try {
      await page.waitForFunction(() => Boolean(document.fullscreenElement), null, { timeout: 3000 });
      await shot(page, viewport.name, "06c-3d-fullscreen", 350);
    } catch {
      console.log("  fullscreen was not granted by the browser; skipped 06c");
    } finally {
      await page.keyboard.press("Escape");
      await page.waitForTimeout(250);
    }
    await scrollToTop(page);
  }

  await page.locator("#tabbutton-inputs").click();
  await scrollToTop(page);
  await page.waitForTimeout(450);
  await shot(page, viewport.name, "07-inputs");
  await shotAtPanel(page, viewport.name, "panel-speed", "07b-inputs-charts");
  if (await scrollToBottom(page)) {
    await shot(page, viewport.name, "07c-inputs-tas");
    await scrollToTop(page);
  }

  if (viewport.name === "390") {
    await page.locator("#tabbutton-overview").click();
    await scrollToTop(page);
    await openDrawerIfNeeded(page);
    await shot(page, viewport.name, "08-drawer");
    await closeDrawerIfNeeded(page);
  }

  await page.locator("#tabbutton-overview").click();
  await scrollToTop(page);
  await page.locator("#shortcuts-open").click();
  await page.locator("#shortcuts-dialog[open]").waitFor();
  await shot(page, viewport.name, "09-shortcuts", 150);
  await page.locator("#shortcuts-close").click();

  await context.close();
}

await rm(outDir, { recursive: true, force: true });
await mkdir(outDir, { recursive: true });

let server = null;
let stopping = false;
try {
  if (!process.env.SHOTS_BASE_URL) {
    server = spawn(process.execPath, [join(webRoot, "node_modules/vite/bin/vite.js"), "--port", String(port), "--strictPort"], {
      cwd: webRoot,
      stdio: ["ignore", "pipe", "pipe"],
    });
    let serverLog = "";
    server.stdout.on("data", (chunk) => (serverLog += chunk.toString()));
    server.stderr.on("data", (chunk) => (serverLog += chunk.toString()));
    server.on("exit", (code) => {
      if (!stopping && code && code !== 0 && code !== null) console.error(`Vite exited with ${code}\n${serverLog}`);
    });
    await waitForServer(baseUrl);
  }

  // A fresh browser per viewport keeps the WebGL context and GPU caches from
  // leaking between captures.
  for (const viewport of viewports) {
    const browser = await chromium.launch({
      headless: true,
      executablePath: process.env.SHOTS_CHROMIUM || undefined,
      args: ["--use-gl=angle", "--use-angle=swiftshader", "--enable-unsafe-swiftshader"],
    });
    try {
      await runViewport(browser, viewport);
    } finally {
      await browser.close();
    }
  }
} finally {
  if (server) {
    stopping = true;
    server.kill("SIGTERM");
    await new Promise((resolve) => setTimeout(resolve, 250));
    if (!server.killed) server.kill("SIGKILL");
  }
}

if (consoleErrors.length) {
  console.error(`\nConsole errors (${consoleErrors.length}):`);
  for (const message of consoleErrors) console.error(`- ${message}`);
}
if (pageErrors.length) {
  console.error(`\nPage errors (${pageErrors.length}):`);
  for (const message of pageErrors) console.error(`- ${message}`);
  process.exitCode = 1;
}
console.log(`\nScreenshots: ${outDir}`);
