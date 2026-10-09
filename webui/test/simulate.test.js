import { describe, it, expect, beforeEach } from "vitest";
import { simulate } from "../src/lib/state/simulate.svelte.js";

beforeEach(() => simulate.reset());

describe("simulated inputs", () => {
  it("leave blank fields out and send numbers as numbers", () => {
    simulate.battery_voltage = "3.9";
    expect(simulate.simulated({ withParams: true })).toEqual({ battery_voltage: 3.9, params: {} });
  });
  it("send the clock only while it is frozen", () => {
    simulate.timestamp = 1700000000;
    expect(simulate.simulated({ withParams: false }).timestamp).toBeUndefined();
    simulate.timeLocked = true;
    expect(simulate.simulated({ withParams: false }).timestamp).toBe(1700000000);
  });
  it("throw on invalid params JSON, and leave params out in mode c", () => {
    simulate.params = "{nope";
    expect(() => simulate.simulated({ withParams: true })).toThrow(/Invalid JSON in parameters/);
    expect(simulate.simulated({ withParams: false }).params).toBeUndefined();
  });
  it("report pending when any input is set", () => {
    expect(simulate.pending).toBe(false);
    simulate.rssi = -70;
    expect(simulate.pending).toBe(true);
  });
});
