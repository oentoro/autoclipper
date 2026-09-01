import { describe, it, expect } from "vitest";
import { secondsToSrtTime } from "./time";

describe("secondsToSrtTime", () => {
  it("formats zero as 00:00:00,000", () => {
    expect(secondsToSrtTime(0)).toBe("00:00:00,000");
  });

  it("formats sub-minute seconds with millis", () => {
    expect(secondsToSrtTime(5.25)).toBe("00:00:05,250");
  });

  it("formats minutes and seconds", () => {
    expect(secondsToSrtTime(75.5)).toBe("00:01:15,500");
  });

  it("formats hours", () => {
    expect(secondsToSrtTime(3661.001)).toBe("01:01:01,001");
  });

  it("zero-pads millis to 3 digits", () => {
    expect(secondsToSrtTime(1.005)).toBe("00:00:01,005");
  });

  it("caps milliseconds at 999 instead of rolling to 1000", () => {
    expect(secondsToSrtTime(0.9995)).toBe("00:00:00,999");
  });
});
