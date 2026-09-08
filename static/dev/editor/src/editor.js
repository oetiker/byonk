import { EditorState, Compartment } from "@codemirror/state";
import { EditorView, keymap, lineNumbers, highlightActiveLine } from "@codemirror/view";
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import { bracketMatching, StreamLanguage, syntaxHighlighting, defaultHighlightStyle, HighlightStyle } from "@codemirror/language";
import { tags as t } from "@lezer/highlight";
import { searchKeymap, highlightSelectionMatches } from "@codemirror/search";
import { lintGutter, setDiagnostics as cmSetDiagnostics } from "@codemirror/lint";
import { xml } from "@codemirror/lang-xml";
import { yaml } from "@codemirror/lang-yaml";
import { lua } from "@codemirror/legacy-modes/mode/lua";

function languageFor(name) {
  if (name === "lua") return StreamLanguage.define(lua);
  if (name === "xml") return xml();
  if (name === "yaml") return yaml();
  return [];
}

// The editor chrome -- everything that is not a syntax token -- is painted
// from the page's own CSS custom properties (static/dev/dev.css), so gutters,
// the active line, selections and the lint marker follow the scheme without a
// second palette living here. Returns a fresh object per call; both themes get
// their own so neither can be affected by the other's construction.
function chromeSpec() {
  return {
    "&": {
      color: "var(--text-color)",
      backgroundColor: "var(--bg-color)",
    },
    ".cm-content": { caretColor: "var(--accent-color)" },
    "&.cm-focused .cm-cursor": { borderLeftColor: "var(--accent-color)" },
    // All three selectors are needed: CodeMirror draws its own selection
    // layer when focused, keeps a dimmed one when not, and falls back to the
    // native ::selection where the layer is disabled.
    "&.cm-focused .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection": {
      backgroundColor: "var(--selection-bg)",
    },
    ".cm-selectionMatch": { backgroundColor: "var(--match-bg)" },
    "&.cm-focused .cm-matchingBracket": {
      backgroundColor: "var(--match-bg)",
      outline: "1px solid var(--accent-color)",
    },
    ".cm-gutters": {
      backgroundColor: "var(--panel-bg)",
      color: "var(--text-muted)",
      border: "none",
      borderRight: "1px solid var(--border-color)",
    },
    ".cm-activeLine": { backgroundColor: "var(--active-line-bg)" },
    ".cm-activeLineGutter": {
      backgroundColor: "var(--active-line-bg)",
      color: "var(--text-color)",
    },
    // @codemirror/lint ships the error marker as a fixed-colour SVG in a
    // `content:` url. `content: normal` drops that replaced content so the
    // marker can be drawn from --error-color instead.
    ".cm-lint-marker-error": {
      content: "normal",
      width: "0.8em",
      height: "0.8em",
      margin: "0.1em",
      borderRadius: "50%",
      backgroundColor: "var(--error-color)",
    },
    ".cm-tooltip": {
      backgroundColor: "var(--panel-bg)",
      color: "var(--text-color)",
      border: "1px solid var(--border-color)",
    },
    ".cm-diagnostic-error": { borderLeftColor: "var(--error-color)" },
    // The search panel (Mod-F, from searchKeymap).
    ".cm-panels": {
      backgroundColor: "var(--panel-bg)",
      color: "var(--text-color)",
    },
  };
}

const lightTheme = EditorView.theme(chromeSpec(), { dark: false });
const darkTheme = EditorView.theme(chromeSpec(), { dark: true });

// The light scheme keeps CodeMirror's stock highlight style: it was designed
// for a white page and is correct on --bg-color's light value. The dark scheme
// gets its own, because that stock style is a light-page palette -- its
// comment colour (#940, a dark orange-brown) is close to unreadable on navy.
// Every colour below was checked against the dark --bg-color (#1a1a2e) and
// clears 4.5:1; the lowest is `invalid` at 6.2:1.
const darkHighlightStyle = HighlightStyle.define([
  { tag: t.comment, color: "#93a4c4", fontStyle: "italic" },
  { tag: [t.keyword, t.modifier, t.controlKeyword, t.operatorKeyword, t.self, t.null], color: "#ff8aa4" },
  { tag: [t.tagName, t.standard(t.tagName)], color: "#ff8aa4" },
  { tag: [t.string, t.special(t.string), t.attributeValue, t.inserted], color: "#94e2b0" },
  { tag: [t.number, t.bool, t.atom, t.literal], color: "#f0b880" },
  { tag: [t.variableName, t.propertyName, t.labelName, t.definition(t.variableName)], color: "#8ecbff" },
  { tag: [t.function(t.variableName), t.function(t.propertyName), t.macroName], color: "#8ecbff" },
  { tag: [t.typeName, t.className, t.namespace, t.attributeName], color: "#ffd479" },
  { tag: [t.operator, t.regexp, t.escape, t.url, t.link], color: "#ffb38a" },
  { tag: [t.meta, t.processingInstruction, t.punctuation, t.bracket, t.angleBracket, t.separator], color: "#9fb0d0" },
  { tag: [t.invalid, t.deleted], color: "#ff6b7f" },
  { tag: t.heading, fontWeight: "bold" },
  { tag: t.emphasis, fontStyle: "italic" },
  { tag: t.strong, fontWeight: "bold" },
  { tag: t.strikethrough, textDecoration: "line-through" },
]);

function themeFor(scheme) {
  return scheme === "dark"
    ? [darkTheme, syntaxHighlighting(darkHighlightStyle)]
    : [lightTheme, syntaxHighlighting(defaultHighlightStyle)];
}

export function create(parent, opts) {
  const onSave = () => {
    if (opts.onSave) opts.onSave();
    return true;
  };
  // The theme lives in a compartment so `setTheme` can swap it in place.
  // Rebuilding the view on a scheme change would throw away this buffer's
  // undo history, cursor and scroll position.
  const themeComp = new Compartment();
  const state = EditorState.create({
    doc: opts.doc || "",
    extensions: [
      lineNumbers(), highlightActiveLine(), history(), bracketMatching(),
      highlightSelectionMatches(), lintGutter(),
      themeComp.of(themeFor(opts.theme)),
      languageFor(opts.language),
      EditorState.readOnly.of(!!opts.readOnly),
      keymap.of([{ key: "Mod-s", run: onSave }, indentWithTab, ...defaultKeymap, ...historyKeymap, ...searchKeymap]),
      EditorView.updateListener.of((u) => { if (u.docChanged && opts.onChange) opts.onChange(); }),
    ],
  });
  const view = new EditorView({ state, parent });
  return {
    getValue: () => view.state.doc.toString(),
    setValue: (text) => view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: text } }),
    // `issues[].line` is 1-based (produced by the Rust-side error_line()
    // parser, see Issue.line: Option<u32> in src/services/screen_store.rs)
    // and is never 0, so the falsy check in the filter only ever drops
    // entries with no line at all (null/undefined) -- not a real line 0.
    setDiagnostics: (issues) => {
      const diags = issues.filter((i) => i.line).map((i) => {
        const ln = view.state.doc.line(Math.min(i.line, view.state.doc.lines));
        return { from: ln.from, to: ln.to, severity: "error", message: i.message };
      });
      view.dispatch(cmSetDiagnostics(view.state, diags));
    },
    // `scheme` is "dark" or "light"; anything else means light. Called by
    // dev.js for every open buffer when the colour scheme changes.
    setTheme: (scheme) => view.dispatch({ effects: themeComp.reconfigure(themeFor(scheme)) }),
    focus: () => view.focus(),
    destroy: () => view.destroy(),
  };
}

// Exposed on `window.byonkEditor` by the IIFE build (see rollup.config.mjs).
