import resolve from "@rollup/plugin-node-resolve";
import terser from "@rollup/plugin-terser";

export default {
  input: "src/editor.js",
  output: { file: "dist/editor.bundle.js", format: "iife", name: "byonkEditor" },
  plugins: [resolve(), terser()],
};
