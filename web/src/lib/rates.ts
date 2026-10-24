//! Shared playback speed presets. Keeping one list means the bottom transport
//! and the fullscreen 3D player offer the same rates.

export const PLAYBACK_RATES = [0.1, 0.25, 0.5, 0.75, 1, 1.25, 1.5, 2, 3, 4];

export function formatRate(rate: number): string {
  return `${rate}×`;
}
