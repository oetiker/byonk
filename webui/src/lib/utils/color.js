// Colour conversion for panel-colour tuning.
//
// A verbatim behavioural port of `static/dev/dev.js:1168-1238` (the old
// popup's `hexToRgb` / `rgbToHex` / `rgbToHsl` / `hslToRgb` / `hexToHsl` /
// `hslToHex`). `static/dev/dev.js` is deleted by Task 12, so this is the
// surviving copy -- the rounding and clamping are copied as-is, not
// "improved": a subtly different conversion changes what gets written into
// a real panel calibration, and the round-trip between the hex box and the
// H/S/L sliders (Tune's UI writes both) only stays lossless if both
// directions agree exactly with the original.

/** @param {string} hex e.g. "#00994D" or "00994D" */
function hexToRgb(hex) {
  hex = hex.replace("#", "");
  return {
    r: parseInt(hex.substring(0, 2), 16),
    g: parseInt(hex.substring(2, 4), 16),
    b: parseInt(hex.substring(4, 6), 16),
  };
}

/** @param {number} r @param {number} g @param {number} b -> "#RRGGBB" */
function rgbToHex(r, g, b) {
  return (
    "#" +
    [r, g, b]
      .map((v) =>
        Math.round(Math.max(0, Math.min(255, v)))
          .toString(16)
          .padStart(2, "0"),
      )
      .join("")
      .toUpperCase()
  );
}

/** @param {number} r @param {number} g @param {number} b (0-255) */
function rgbToHsl(r, g, b) {
  r /= 255;
  g /= 255;
  b /= 255;
  const max = Math.max(r, g, b),
    min = Math.min(r, g, b);
  let h, s;
  const l = (max + min) / 2;
  if (max === min) {
    h = s = 0;
  } else {
    const d = max - min;
    s = l > 0.5 ? d / (2 - max - min) : d / (max + min);
    switch (max) {
      case r:
        h = ((g - b) / d + (g < b ? 6 : 0)) / 6;
        break;
      case g:
        h = ((b - r) / d + 2) / 6;
        break;
      default:
        h = ((r - g) / d + 4) / 6;
        break;
    }
  }
  return { h: Math.round(h * 360), s: Math.round(s * 100), l: Math.round(l * 100) };
}

/** @param {number} h (0-360) @param {number} s (0-100) @param {number} l (0-100) */
function hslToRgb(h, s, l) {
  h /= 360;
  s /= 100;
  l /= 100;
  let r, g, b;
  if (s === 0) {
    r = g = b = l;
  } else {
    const hue2rgb = (p, q, t) => {
      if (t < 0) t += 1;
      if (t > 1) t -= 1;
      if (t < 1 / 6) return p + (q - p) * 6 * t;
      if (t < 1 / 2) return q;
      if (t < 2 / 3) return p + (q - p) * (2 / 3 - t) * 6;
      return p;
    };
    const q = l < 0.5 ? l * (1 + s) : l + s - l * s;
    const p = 2 * l - q;
    r = hue2rgb(p, q, h + 1 / 3);
    g = hue2rgb(p, q, h);
    b = hue2rgb(p, q, h - 1 / 3);
  }
  return { r: Math.round(r * 255), g: Math.round(g * 255), b: Math.round(b * 255) };
}

/**
 * A hex colour ("#RRGGBB") as `{h, s, l}` -- `h` in 0-360, `s`/`l` in 0-100,
 * all rounded to the nearest integer.
 */
export function hexToHsl(hex) {
  const { r, g, b } = hexToRgb(hex);
  return rgbToHsl(r, g, b);
}

/**
 * `{h, s, l}` (`h` 0-360, `s`/`l` 0-100) as an upper-case "#RRGGBB" hex
 * colour.
 */
export function hslToHex(h, s, l) {
  const { r, g, b } = hslToRgb(h, s, l);
  return rgbToHex(r, g, b);
}
