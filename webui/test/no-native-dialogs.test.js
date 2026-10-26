import { it, expect } from "vitest";
const sources = import.meta.glob("../src/**/*.{svelte,js}", { query: "?raw", import: "default", eager: true });
// Strip comments until nothing changes, so a removal cannot leave a new
// comment opener behind (e.g. `<!<!---->--`).
const stripOnce = (t) => t.replace(/<!--[\s\S]*?-->/g, "").replace(/\/\*[\s\S]*?\*\//g, "").replace(/\/\/.*$/gm, "");
const strip = (t) => {
  for (let prev = null; prev !== t; ) [prev, t] = [t, stripOnce(t)];
  return t;
};
it("the UI never opens a native prompt() or confirm()", () => {
  const offenders = Object.entries(sources)
    .filter(([, text]) => /(^|[^\w.])(window\.)?(prompt|confirm)\s*\(/m.test(strip(text)))
    .map(([path]) => path);
  expect(offenders).toEqual([]);
});
