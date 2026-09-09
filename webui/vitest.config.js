import { defineConfig } from "vitest/config";
import { svelte } from "@sveltejs/vite-plugin-svelte";

// vite-plugin-svelte 7.x dropped the `hot` plugin option (see
// allowedPluginOptions in its source) — HMR is irrelevant under vitest's
// non-serving jsdom environment anyway, so there is nothing to replace it
// with.
export default defineConfig({
  plugins: [svelte()],
  test: {
    environment: "jsdom",
    setupFiles: ["./test/setup.js"],
    include: ["test/**/*.test.js"],
  },
});
