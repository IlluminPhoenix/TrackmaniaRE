//! Formatting and small download/copy helpers.

/** `1:02.345`, `12.345`, `1:02:03.456` for hours. */
export function fmtTime(ms: number | null | undefined): string {
  if (ms === null || ms === undefined || !Number.isFinite(ms)) return "–";
  const sign = ms < 0 ? "-" : "";
  const total = Math.abs(Math.round(ms));
  const h = Math.floor(total / 3_600_000);
  const m = Math.floor((total % 3_600_000) / 60_000);
  const s = Math.floor((total % 60_000) / 1000);
  const millis = total % 1000;
  if (h > 0) return `${sign}${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}.${String(millis).padStart(3, "0")}`;
  return `${sign}${m}:${String(s).padStart(2, "0")}.${String(millis).padStart(3, "0")}`;
}

/** Signed delta in ms, e.g. `+0.345` / `-1.204`. */
export function fmtDelta(ms: number | null | undefined): string {
  if (ms === null || ms === undefined || !Number.isFinite(ms)) return "–";
  const sign = ms > 0 ? "+" : ms < 0 ? "-" : "";
  const total = Math.abs(ms);
  if (total < 60_000) return `${sign}${(total / 1000).toFixed(3)}s`;
  return `${sign}${fmtTime(total)}`;
}

export function fmtSpeedKmh(mps: number | null | undefined): string {
  if (mps === null || mps === undefined || !Number.isFinite(mps)) return "–";
  return `${(mps * 3.6).toFixed(1)} km/h`;
}

export function fmtMeters(m: number | null | undefined): string {
  if (m === null || m === undefined || !Number.isFinite(m)) return "–";
  if (Math.abs(m) >= 1000) return `${(m / 1000).toFixed(3)} km`;
  return `${m.toFixed(1)} m`;
}

export function fmtBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} kB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/** Full steering lock in the recorded raw units (-127 .. 127). */
export const STEER_FULL = 127;

/** Signed steering as a share of full lock, e.g. `+63%`, `0%`, `-100%`. */
export function fmtSteerPercent(steer: number): string {
  const percent = Math.round((steer / STEER_FULL) * 100);
  if (percent === 0) return "0%";
  return `${percent > 0 ? "+" : ""}${percent}%`;
}

export function fmtCount(n: number): string {
  return n.toLocaleString("en-US");
}

export function downloadText(fileName: string, textContents: string, mime = "text/plain"): void {
  downloadBlob(fileName, new Blob([textContents], { type: mime }));
}

export function downloadBytes(fileName: string, bytes: Uint8Array, mime = "application/octet-stream"): void {
  const copy = new Uint8Array(bytes.length);
  copy.set(bytes);
  downloadBlob(fileName, new Blob([copy], { type: mime }));
}

function downloadBlob(fileName: string, blob: Blob): void {
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = fileName;
  document.body.appendChild(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 10_000);
}

/** Clipboard copy with a textarea fallback for non-secure contexts. */
export async function copyText(value: string): Promise<boolean> {
  try {
    if (navigator.clipboard && window.isSecureContext) {
      await navigator.clipboard.writeText(value);
      return true;
    }
  } catch {
    // fall through to the textarea path
  }
  try {
    const area = document.createElement("textarea");
    area.value = value;
    area.style.position = "fixed";
    area.style.opacity = "0";
    document.body.appendChild(area);
    area.select();
    const ok = document.execCommand("copy");
    area.remove();
    return ok;
  } catch {
    return false;
  }
}

export function fileNameStem(fileName: string): string {
  return fileName.replace(/\.(Replay|Ghost)\.Gbx$/i, "").replace(/\.Gbx$/i, "");
}
