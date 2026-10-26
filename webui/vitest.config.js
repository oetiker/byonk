import { fileURLToPath } from "node:url";
import { defineConfig } from "vitest/config";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// vite-plugin-svelte 7.x dropped the `hot` plugin option (see
// allowedPluginOptions in its source) — HMR is irrelevant under vitest's
// non-serving jsdom environment anyway, so there is nothing to replace it
// with.
export default defineConfig({
  plugins: [svelte()],
  resolve: {
    conditions: ["browser", "node", "import"],
    // Kept in sync with the identical alias in vite.config.js -- see the
    // comment there. Vitest does not read vite.config.js's resolve block,
    // so this must be declared separately or imports resolved fine by
    // `pnpm build` would fail every test that touches a `$lib/...` import.
    alias: {
      $lib: fileURLToPath(new URL("./src/lib", import.meta.url)),
    },
  },
  test: {
    environment: "jsdom",
    setupFiles: ["./test/setup.js"],
    include: ["test/**/*.test.js"],
  },
});
