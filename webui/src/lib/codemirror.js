// CodeMirror setup, ported from static/dev/editor/src/editor.js.
//
// Differences from that original:
//   1. Exports a factory (`createEditor`) instead of attaching to `window` --
//      this app has no IIFE bundle step to expose a global from.
//   2. Read-only is enforced by TWO compartments, not one. `EditorState.readOnly`
//      blocks state transactions from editing commands, but it does not touch
//      the DOM: the contentDOM stays `contenteditable`, so a caret still shows
//      and some native browser editing (e.g. paste, IME composition) still
//      lands. `EditorView.editable` controls that DOM attribute. Both are
//      needed to make typing genuinely insert nothing (spec §8.6).
//   3. The theme `Compartment` is untouched -- it stays runtime-reconfigurable
//      via `setTheme`, and chrome colours keep reading the page's own CSS
//      custom properties rather than a second palette.
import { EditorState, Compartment } from "@codemirror/state";
import { EditorView, keymap, lineNumbers, highlightActiveLine } from "@codemirror/view";
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import {
  bracketMatching,
  StreamLanguage,
  syntaxHighlighting,
  defaultHighlightStyle,
  HighlightStyle,
} from "@codemirror/language";
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
// from the page's own CSS custom properties, so gutters, the active line,
// selections and the lint marker follow the scheme without a second palette
// living here. Returns a fresh object per call; both themes get their own so
// neither can be affected by the other's construction.
function chromeSpec() {
  return {
    "&": {
      color: "var(--foreground)",
      backgroundColor: "var(--background)",
    },
    ".cm-content": { caretColor: "var(--primary)" },
    "&.cm-focused .cm-cursor": { borderLeftColor: "var(--primary)" },
    // All three selectors are needed: CodeMirror draws its own selection
    // layer when focused, keeps a dimmed one when not, and falls back to the
    // native ::selection where the layer is disabled.
    "&.cm-focused .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection": {
      backgroundColor: "var(--selection)",
    },
    ".cm-selectionMatch": { backgroundColor: "var(--match)" },
    "&.cm-focused .cm-matchingBracket": {
      backgroundColor: "var(--match)",
      outline: "1px solid var(--primary)",
    },
    ".cm-gutters": {
      backgroundColor: "var(--muted)",
      color: "var(--muted-foreground)",
      border: "none",
      borderRight: "1px solid var(--border)",
    },
    ".cm-activeLine": { backgroundColor: "var(--accent)" },
    ".cm-activeLineGutter": {
      backgroundColor: "var(--accent)",
      color: "var(--foreground)",
    },
    // @codemirror/lint ships the error marker as a fixed-colour SVG in a
    // `content:` url. `content: normal` drops that replaced content so the
    // marker can be drawn from --destructive instead.
    ".cm-lint-marker-error": {
      content: "normal",
      width: "0.8em",
      height: "0.8em",
      margin: "0.1em",
      borderRadius: "50%",
      backgroundColor: "var(--destructive)",
    },
    ".cm-tooltip": {
      backgroundColor: "var(--muted)",
      color: "var(--foreground)",
      border: "1px solid var(--border)",
    },
    ".cm-diagnostic-error": { borderLeftColor: "var(--destructive)" },
    // The search panel (Mod-F, from searchKeymap).
    ".cm-panels": {
      backgroundColor: "var(--muted)",
      color: "var(--foreground)",
    },
  };
}

const lightTheme = EditorView.theme(chromeSpec(), { dark: false });
const darkTheme = EditorView.theme(chromeSpec(), { dark: true });

// The light scheme keeps CodeMirror's stock highlight style: it was designed
// for a white page and is correct on --background's light value. The dark scheme
// gets its own, because that stock style is a light-page palette -- its
// comment colour (#940, a dark orange-brown) is close to unreadable on navy.
// Every colour below was checked against the dark --background (#0f1622) and
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

export function createEditor(parent, opts = {}) {
  const onSave = () => {
    if (opts.onSave) opts.onSave();
    return true;
  };
  // The theme lives in a compartment so `setTheme` can swap it in place.
  // Rebuilding the view on a scheme change would throw away this buffer's
  // undo history, cursor and scroll position. `language`, `readOnly` and
  // `editable` get the same treatment so `setDoc` can reconfigure a tab swap
  // without rebuilding the view either.
  const themeComp = new Compartment();
  const languageComp = new Compartment();
  const readOnlyComp = new Compartment();
  const editableComp = new Compartment();

  const state = EditorState.create({
    doc: opts.doc || "",
    extensions: [
      lineNumbers(),
      highlightActiveLine(),
      history(),
      bracketMatching(),
      highlightSelectionMatches(),
      lintGutter(),
      themeComp.of(themeFor(opts.theme)),
      languageComp.of(languageFor(opts.language)),
      readOnlyComp.of(EditorState.readOnly.of(!!opts.readOnly)),
      editableComp.of(EditorView.editable.of(!opts.readOnly)),
      keymap.of([{ key: "Mod-s", run: onSave }, indentWithTab, ...defaultKeymap, ...historyKeymap, ...searchKeymap]),
      EditorView.updateListener.of((u) => {
        if (u.docChanged && opts.onChange) opts.onChange(u.state.doc.toString());
        // The status bar's cursor position (spec §1, Task 10). Fires on a
        // caret move AND on a doc change (typing moves the caret too, and
        // `setDoc`'s full-document replace below counts as one) so a tab
        // swap's own dispatch reports the new buffer's position without a
        // separate initial call.
        if ((u.docChanged || u.selectionSet) && opts.onCursor) {
          const pos = u.state.selection.main.head;
          const line = u.state.doc.lineAt(pos);
          opts.onCursor(line.number, pos - line.from + 1);
        }
      }),
    ],
  });
  const view = new EditorView({ state, parent });

  return {
    view,
    // Replaces the whole document and reconfigures language/read-only
    // together, for a tab swap -- not a rebuild, so this stays cheap and
    // doesn't reset the view's DOM node identity.
    setDoc: (text, language, readOnly) => {
      view.dispatch({
        changes: { from: 0, to: view.state.doc.length, insert: text ?? "" },
        effects: [
          languageComp.reconfigure(languageFor(language)),
          readOnlyComp.reconfigure(EditorState.readOnly.of(!!readOnly)),
          editableComp.reconfigure(EditorView.editable.of(!readOnly)),
        ],
      });
    },
    // `issues[].line` is 1-based (produced by the Rust-side error_line()
    // parser, see Issue.line: Option<u32> in src/services/screen_store.rs)
    // and is never 0, so the falsy check in the filter only ever drops
    // entries with no line at all (null/undefined) -- not a real line 0.
    setDiagnostics: (issues) => {
      const diags = (issues ?? [])
        .filter((i) => i.line)
        .map((i) => {
          const ln = view.state.doc.line(Math.min(i.line, view.state.doc.lines));
          return { from: ln.from, to: ln.to, severity: "error", message: i.message };
        });
      view.dispatch(cmSetDiagnostics(view.state, diags));
    },
    // `scheme` is "dark" or "light"; anything else means light.
    setTheme: (scheme) => view.dispatch({ effects: themeComp.reconfigure(themeFor(scheme)) }),
    destroy: () => view.destroy(),
  };
}
