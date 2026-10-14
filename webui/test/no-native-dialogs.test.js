import { it, expect } from "vitest";
const sources = import.meta.glob("../src/**/*.{svelte,js}", { query: "?raw", import: "default", eager: true });
const strip = (t) => t.replace(/<!--[\s\S]*?-->/g, "").replace(/\/\*[\s\S]*?\*\//g, "").replace(/\/\/.*$/gm, "");
it("the UI never opens a native prompt() or confirm()", () => {
  const offenders = Object.entries(sources)
    .filter(([, text]) => /(^|[^\w.])(window\.)?(prompt|confirm)\s*\(/m.test(strip(text)))
    .map(([path]) => path);
  expect(offenders).toEqual([]);
});
