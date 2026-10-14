// One place that turns the subject into a render request (spec §4). Pure:
// the stores are arguments, so each mode's body is a unit-testable value.
// The preview never sends `override` -- Push is the only way onto that rung,
// and it is the Edit panel's (spec §2 rule 2).

export const DEFAULT_SIZE = { width: 800, height: 480 };
/** The server's `resolve_preview_dimensions`: a model's size when no panel states one. */
const MODEL_SIZE = { x: { width: 1872, height: 1404 } };

function sizeOf(meta, panelId, model) {
  const p = (meta.panels ?? []).find((x) => x.id === panelId);
  if (p && p.width && p.height) return { width: p.width, height: p.height };
  return MODEL_SIZE[model] ?? DEFAULT_SIZE;
}

/** A device's panel size, else the size of the model it last reported (`DeviceInfo.model`). */
function deviceSize({ meta, usage }, deviceId) {
  const d = (meta.devices ?? []).find((x) => x.id === deviceId);
  return sizeOf(meta, devicePanel({ meta, usage }, deviceId), d?.model);
}

/** The device's explicit panel, else the panel the server says it resolves to. */
export function devicePanel({ meta, usage }, deviceId) {
  const d = (meta.devices ?? []).find((x) => x.id === deviceId);
  if (d?.panel) return d.panel;
  for (const [panel, keys] of Object.entries(usage?.panels ?? {})) {
    if (keys.includes(deviceId)) return panel;
  }
  return null;
}

export function previewSize({ subject, meta, usage }) {
  switch (subject.kind) {
    case "screen": {
      const t = subject.target;
      if (t?.kind === "panel") return sizeOf(meta, t.id);
      if (t?.kind === "device") return deviceSize({ meta, usage }, t.id);
      return DEFAULT_SIZE;
    }
    case "panel":
      return sizeOf(meta, subject.id);
    case "device":
      return deviceSize({ meta, usage }, subject.id);
    default:
      return DEFAULT_SIZE;
  }
}

/** The subjects that have a picture; a screen repo has none (U11). */
const RENDERED = ["screen", "panel", "device"];

export function buildRenderRequest({ subject, draft, simulate, meta, usage }) {
  if (!RENDERED.includes(subject.kind)) return null;
  const size = previewSize({ subject, meta, usage });
  const view = { use_actual: !!simulate.use_actual };
  if (subject.kind === "screen") {
    const simulated = simulate.simulated({ withParams: true });
    const body = { screen: subject.id, ...size, simulated, view };
    const t = subject.target;
    if (t?.kind === "panel") body.panel = t.id;
    if (t?.kind === "device") body.device = t.id;
    return { body, size, screen: subject.id, as: t ? t.id : null };
  }
  if (subject.kind === "panel") {
    const simulated = simulate.simulated({ withParams: true });
    const body = {
      screen: subject.borrowedScreen,
      panel: subject.id,
      ...size,
      draft: { panel: draft.panelWire(subject.id) },
      simulated,
      view,
    };
    // P2-6: watching a device renders as that device; the panel still wins.
    if (subject.watchDevice) body.device = subject.watchDevice;
    return { body, size, screen: subject.borrowedScreen, as: subject.id };
  }
  // Mode c: no simulated params -- the device owns real ones (spec §4).
  const simulated = simulate.simulated({ withParams: false });
  const w = draft.deviceWire(subject.id);
  const body = {
    device: subject.id,
    screen: w.screen,
    ...size,
    draft: { base: w.base, final: w.final, params: w.params },
    simulated,
    view,
  };
  return { body, size, screen: w.screen, as: subject.id };
}
