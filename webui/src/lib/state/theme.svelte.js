// Three-way colour scheme: System / Light / Dark (spec §8.9, settled -- not
// open for relitigation). Persisted alone (not folded into layout.svelte.js's
// record) because it is broadcast to the pop-out and layout is not.
//
// Wire contract (Ruling 51): `onBroadcast(cb)` is a LOCAL observer seam --
// its callback receives the plain mode string ("dark"), matching what this
// module's own test pins. The actual cross-window transport is a SEPARATE,
// real `BroadcastChannel("byonk-preview")` message shaped `{theme: mode}` --
// the same channel `static/dev/preview.html` and PreviewPane.svelte already
// use for render frames, because one channel is simpler than two. The seam
// exists for in-page listeners (e.g. tests); it does not substitute for the
// wire, and the wire does not substitute for it.

export const STORAGE_KEY = "byonk.theme.v1";
export const CHANNEL_NAME = "byonk-preview";
const MODES = ["system", "light", "dark"];

function safeGetItem(key) {
  try {
    return localStorage.getItem(key);
  } catch {
    // Private window or blocked site data: behave as if nothing is stored.
    return null;
  }
}

function safeSetItem(key, value) {
  try {
    localStorage.setItem(key, value);
  } catch {
    // The choice is a convenience; losing it must never break the editor.
  }
}

// `window.matchMedia` does not exist in this project's jsdom test
// environment (confirmed directly) and is missing in a few real browsers
// too. Every call site goes through here so a throw or a missing function
// degrades to "assume light" instead of taking the caller down with it.
function getMediaQuery() {
  if (typeof window === "undefined" || typeof window.matchMedia !== "function") {
    return null;
  }
  try {
    return window.matchMedia("(prefers-color-scheme: dark)");
  } catch {
    return null;
  }
}

function systemPrefersDark() {
  return getMediaQuery()?.matches === true;
}

function openChannel() {
  try {
    return new BroadcastChannel(CHANNEL_NAME);
  } catch {
    // Constructor missing in a few browsers -- same guard PreviewPane.svelte
    // and static/dev/preview.html already use for this channel.
    return null;
  }
}

class Theme {
  #mode = $state("system");
  #channel = null;
  #listeners = [];
  #mqlBound = false;

  get mode() {
    return this.#mode;
  }

  /** Whether the resolved scheme (mode + system preference) is dark. */
  get #effectiveDark() {
    return this.#mode === "dark" || (this.#mode === "system" && systemPrefersDark());
  }

  #applyDom() {
    document.documentElement.classList.toggle("dark", this.#effectiveDark);
  }

  #ensureSystemListener() {
    if (this.#mqlBound) return;
    const mql = getMediaQuery();
    if (!mql) return;
    const onChange = () => {
      if (this.#mode === "system") this.#applyDom();
    };
    // addEventListener is the modern API; addListener is the Safari <14
    // fallback -- same pair static/dev pages have had to account for
    // elsewhere in this codebase.
    if (typeof mql.addEventListener === "function") {
      mql.addEventListener("change", onChange);
    } else if (typeof mql.addListener === "function") {
      mql.addListener(onChange);
    }
    this.#mqlBound = true;
  }

  set(mode) {
    if (!MODES.includes(mode)) return;
    this.#mode = mode;
    this.#applyDom();
    safeSetItem(STORAGE_KEY, mode);
    this.#channel ??= openChannel();
    this.#channel?.postMessage({ theme: mode });
    for (const cb of this.#listeners) cb(mode);
  }

  /** Read the persisted choice and apply it. Does not touch the wire. */
  load() {
    const stored = safeGetItem(STORAGE_KEY);
    this.#mode = MODES.includes(stored) ? stored : "system";
    this.#applyDom();
    this.#ensureSystemListener();
  }

  /**
   * Clear IN-MEMORY state only -- never localStorage. This is what lets a
   * test (or a future "revert" affordance) get back to the just-installed
   * default without discarding what a real user already chose; `load()` is
   * the only thing that reads storage back in.
   */
  reset() {
    this.#mode = "system";
    this.#applyDom();
  }

  /** Local observer seam (Ruling 51) -- receives the mode string. */
  onBroadcast(cb) {
    this.#listeners.push(cb);
    return () => {
      this.#listeners = this.#listeners.filter((l) => l !== cb);
    };
  }
}

export const theme = new Theme();
