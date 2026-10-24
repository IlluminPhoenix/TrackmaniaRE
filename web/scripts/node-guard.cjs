// Preflight for the scripts that run modern tooling (vite 8, vitest 5, tsc 5).
// Keep this file free of syntax an old Node cannot parse (`?.`, `??=`, and
// friends): it has to run before it can tell you your Node is too old.
//
// Keep MIN in sync with the `engines` field in web/package.json.

const MIN = [22, 22, 2];
const [major, minor, patch] = process.versions.node.split(".").map(Number);
const current = process.versions.node;

const ok =
  major > MIN[0] ||
  (major === MIN[0] && (minor > MIN[1] || (minor === MIN[1] && patch >= MIN[2])));

if (!ok) {
  console.error("web/ needs Node " + MIN.join(".") + " or newer, this is v" + current + ".");
  console.error("CI uses 24; put a newer Node first on PATH and run the command again.");
  process.exit(1);
}
