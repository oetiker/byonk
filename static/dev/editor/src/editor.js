import { EditorState } from "@codemirror/state";
import { EditorView, keymap, lineNumbers, highlightActiveLine } from "@codemirror/view";
import { defaultKeymap, history, historyKeymap, indentWithTab } from "@codemirror/commands";
import { bracketMatching, StreamLanguage, syntaxHighlighting, defaultHighlightStyle } from "@codemirror/language";
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

export function create(parent, opts) {
  const onSave = () => {
    if (opts.onSave) opts.onSave();
    return true;
  };
  const state = EditorState.create({
    doc: opts.doc || "",
    extensions: [
      lineNumbers(), highlightActiveLine(), history(), bracketMatching(),
      highlightSelectionMatches(), lintGutter(),
      syntaxHighlighting(defaultHighlightStyle),
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
    focus: () => view.focus(),
    destroy: () => view.destroy(),
  };
}

// Exposed on `window.byonkEditor` by the IIFE build (see rollup.config.mjs).
