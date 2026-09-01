import { describe, it, expect } from "vitest";
import {
  clamp,
  computeMoveDrag,
  computeTrimStartDrag,
  computeTrimEndDrag,
  MIN_SEGMENT_DURATION,
} from "./timelineMath";

describe("clamp", () => {
  it("passes through in-range values", () => {
    expect(clamp(5, 0, 10)).toBe(5);
  });
  it("clamps below min", () => {
    expect(clamp(-5, 0, 10)).toBe(0);
  });
  it("clamps above max", () => {
    expect(clamp(15, 0, 10)).toBe(10);
  });
});

describe("computeMoveDrag", () => {
  it("shifts both start and end by delta", () => {
    expect(computeMoveDrag(10, 12, 3, 100)).toEqual({ start: 13, end: 15 });
  });
  it("clamps shift so start does not go below 0", () => {
    expect(computeMoveDrag(2, 5, -10, 100)).toEqual({ start: 0, end: 3 });
  });
  it("clamps shift so end does not exceed duration", () => {
    expect(computeMoveDrag(95, 98, 10, 100)).toEqual({ start: 97, end: 100 });
  });
});

describe("computeTrimStartDrag", () => {
  it("moves start left/right within bounds", () => {
    expect(computeTrimStartDrag(10, 20, 3)).toEqual({ start: 13, end: 20 });
  });
  it("clamps at 0", () => {
    expect(computeTrimStartDrag(2, 20, -10)).toEqual({ start: 0, end: 20 });
  });
  it("does not cross MIN_SEGMENT_DURATION floor relative to end", () => {
    const result = computeTrimStartDrag(10, 10.15, 10);
    expect(result.start).toBeCloseTo(10.15 - MIN_SEGMENT_DURATION, 5);
    expect(result.end).toBe(10.15);
  });
  it("never goes negative even for a degenerate near-zero end", () => {
    const result = computeTrimStartDrag(0, 0.05, -5);
    expect(result.start).toBe(0);
  });
});

describe("computeTrimEndDrag", () => {
  it("moves end left/right within bounds", () => {
    expect(computeTrimEndDrag(10, 20, -3, 100)).toEqual({ start: 10, end: 17 });
  });
  it("clamps at duration", () => {
    expect(computeTrimEndDrag(10, 95, 20, 100)).toEqual({ start: 10, end: 100 });
  });
  it("does not cross MIN_SEGMENT_DURATION floor relative to start", () => {
    const result = computeTrimEndDrag(10, 10.15, -10, 100);
    expect(result.start).toBe(10);
    expect(result.end).toBeCloseTo(10 + MIN_SEGMENT_DURATION, 5);
  });
  it("never exceeds duration even for a degenerate near-duration start", () => {
    const result = computeTrimEndDrag(99.98, 100, 5, 100);
    expect(result.end).toBe(100);
  });
});
