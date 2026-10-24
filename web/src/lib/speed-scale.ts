//! Adaptive speed range for the 3D path colouring.
//!
//! A fixed 0–220 km/h ramp turns most Trackmania maps into one saturated red
//! line: fast maps spend nearly all their samples above the top stop. The
//! colour range is therefore derived from the visible replays themselves.
//!
//! The range uses the 2nd–98th percentile, rounded outwards to 10 km/h, so a
//! single noisy sample cannot steal a whole end of the ramp. Values outside
//! the range clamp to the endpoint colours.

/** Inclusive km/h range mapped onto the speed colour ramp. */
export interface SpeedRange {
  low: number;
  high: number;
}

const LOW_PERCENTILE = 0.02;
const HIGH_PERCENTILE = 0.98;
const ROUND_KMH = 10;
const MIN_SPAN_KMH = 20;

/**
 * Builds one shared range from sample speed groups in metres per second.
 *
 * A shared range keeps the selected run comparable when the primary selection
 * changes. Runs without samples are simply not passed in.
 */
export function adaptiveSpeedRangeKmh(
  groupsMps: Iterable<ArrayLike<number>>,
  lowPercentile = LOW_PERCENTILE,
  highPercentile = HIGH_PERCENTILE,
): SpeedRange | null {
  const values: number[] = [];
  for (const group of groupsMps) {
    for (let i = 0; i < group.length; i++) {
      const kmh = group[i] * 3.6;
      if (Number.isFinite(kmh)) values.push(Math.max(0, kmh));
    }
  }
  if (!values.length) return null;
  values.sort((a, b) => a - b);

  const low = quantile(values, lowPercentile);
  const high = quantile(values, highPercentile);
  let lowKmh = Math.max(0, Math.floor(low / ROUND_KMH) * ROUND_KMH);
  let highKmh = Math.ceil(high / ROUND_KMH) * ROUND_KMH;

  if (highKmh - lowKmh < MIN_SPAN_KMH) {
    const middle = (lowKmh + highKmh) / 2;
    lowKmh = Math.max(0, Math.floor((middle - MIN_SPAN_KMH / 2) / ROUND_KMH) * ROUND_KMH);
    highKmh = lowKmh + MIN_SPAN_KMH;
  }
  return { low: lowKmh, high: highKmh };
}

/** Linear-interpolated quantile for a sorted, non-empty array. */
function quantile(sorted: number[], fraction: number): number {
  const position = Math.max(0, Math.min(1, fraction)) * (sorted.length - 1);
  const lower = Math.floor(position);
  const upper = Math.ceil(position);
  if (lower === upper) return sorted[lower];
  return sorted[lower] + (sorted[upper] - sorted[lower]) * (position - lower);
}
