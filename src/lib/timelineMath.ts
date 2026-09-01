export const MIN_SEGMENT_DURATION = 0.1;

export function clamp(value: number, min: number, max: number): number {
  return Math.min(Math.max(value, min), max);
}

export function computeMoveDrag(
  origStart: number,
  origEnd: number,
  deltaSec: number,
  duration: number
): { start: number; end: number } {
  if (!(duration > 0) || !Number.isFinite(duration)) return { start: origStart, end: origEnd };
  const shift = clamp(deltaSec, -origStart, duration - origEnd);
  return { start: origStart + shift, end: origEnd + shift };
}

export function computeTrimStartDrag(
  origStart: number,
  origEnd: number,
  deltaSec: number
): { start: number; end: number } {
  const upper = Math.max(0, origEnd - MIN_SEGMENT_DURATION);
  const start = clamp(origStart + deltaSec, 0, upper);
  return { start, end: origEnd };
}

export function computeTrimEndDrag(
  origStart: number,
  origEnd: number,
  deltaSec: number,
  duration: number
): { start: number; end: number } {
  if (!(duration > 0) || !Number.isFinite(duration)) return { start: origStart, end: origEnd };
  const lower = Math.min(duration, origStart + MIN_SEGMENT_DURATION);
  const end = clamp(origEnd + deltaSec, lower, duration);
  return { start: origStart, end };
}
