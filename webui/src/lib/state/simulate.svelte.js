// Simulated inputs and view options (spec §3.3, §7). Nothing here is a
// render setting: what the device would report about itself, and how the
// answer is depicted. Device, panel and the tuning knobs moved to the
// subject and the config editor.

const DEFAULTS = {
  use_actual: true,
  battery_voltage: "",
  rssi: "",
  timestamp: "",
  timeLocked: false,
  params: "{}",
};

/** "" (untouched text field), null (cleared number field) or undefined. */
function isBlank(v) {
  return v === "" || v === undefined || v === null;
}

class Simulate {
  /** The last failed render `{op: "render", message}` (Ruling 41/66). */
  error = $state(null);
  use_actual = $state(DEFAULTS.use_actual);
  battery_voltage = $state(DEFAULTS.battery_voltage);
  rssi = $state(DEFAULTS.rssi);
  timestamp = $state(DEFAULTS.timestamp);
  timeLocked = $state(DEFAULTS.timeLocked);
  params = $state(DEFAULTS.params);

  reset() {
    for (const [k, v] of Object.entries(DEFAULTS)) this[k] = v;
  }

  /** A snapshot an effect reads to re-render on any change. */
  get fields() {
    return {
      use_actual: this.use_actual,
      battery_voltage: this.battery_voltage,
      rssi: this.rssi,
      timestamp: this.timestamp,
      timeLocked: this.timeLocked,
      params: this.params,
    };
  }

  /** Anything the user set that a real device would not report. */
  get pending() {
    return (
      !isBlank(this.battery_voltage) ||
      !isBlank(this.rssi) ||
      this.timeLocked ||
      (this.params.trim() !== "" && this.params.trim() !== "{}")
    );
  }

  /**
   * The `simulated` object of `POST api/render`. Throws on invalid params
   * JSON so the user never sees a render made from parameters they did not
   * write; `renderNow()` catches it.
   */
  simulated({ withParams }) {
    const out = {};
    if (!isBlank(this.battery_voltage)) out.battery_voltage = Number(this.battery_voltage);
    if (!isBlank(this.rssi)) out.rssi = Number(this.rssi);
    if (this.timeLocked && !isBlank(this.timestamp)) out.timestamp = Number(this.timestamp);
    if (withParams) {
      try {
        out.params = JSON.parse(this.params || "{}");
      } catch (e) {
        throw new Error("Invalid JSON in parameters: " + e.message);
      }
    }
    return out;
  }
}

export const simulate = new Simulate();
