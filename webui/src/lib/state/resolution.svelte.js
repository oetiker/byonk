// The last render's provenance (spec §5): written by the Preview panel, read
// by the Edit panel's ladder. A shared store, not PreviewPane state, because
// Preview can be closed while Edit is open -- the ladder then shows the last
// payload it received, labelled as such.

export function parseResolutionHeader(raw) {
  if (!raw) return null;
  return JSON.parse(raw);
}

class Resolution {
  /** "none" before any render for this subject, "ok", or "failed". */
  state = $state("none");
  payload = $state(null);
  failure = $state(null);
  subjectKey = $state(null);

  record(payload, subjectKey) {
    this.state = "ok";
    this.payload = payload;
    this.failure = null;
    this.subjectKey = subjectKey;
  }

  /** A failed render carries no header; showing the previous rungs would lie. */
  fail(message, subjectKey) {
    this.state = "failed";
    this.payload = null;
    this.failure = message;
    this.subjectKey = subjectKey;
  }

  setting(key) {
    return this.payload?.[key] ?? null;
  }

  reset() {
    this.state = "none";
    this.payload = null;
    this.failure = null;
    this.subjectKey = null;
  }
}

export const resolution = new Resolution();
