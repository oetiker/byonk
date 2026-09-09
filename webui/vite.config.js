import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import tailwindcss from "@tailwindcss/vite";

// The app is served from /dev/ in production and from / by the dev server, and
// under Home Assistant Ingress from a prefix the server never sees. `base: ""`
// makes every emitted asset reference relative to the document, which is the
// only form that survives all three. Anything root-absolute ejects the user
// out of the HA iframe.
//
// Output names are fixed with no content hash because src/api/dev.rs embeds
// these three paths with include_str! at compile time.
export default defineConfig({
  base: "",
  plugins: [svelte(), tailwindcss()],
  build: {
    outDir: "dist",
    emptyOutDir: true,
    rollupOptions: {
      output: {
        entryFileNames: "app.js",
        chunkFileNames: "app.js",
        assetFileNames: (info) =>
          info.names?.[0]?.endsWith(".css") ? "app.css" : "[name][extname]",
        manualChunks: undefined,
      },
    },
  },
  server: {
    // The dev server talks to a byonk running `serve` on :3000. The app's own
    // fetches are relative, so from a document at / they land on /api/...;
    // rewrite those onto /dev/api/... and authenticate with the admin bearer
    // token, which `admit()` accepts (src/api/dev_auth.rs). Export
    // BYONK_ADMIN_TOKEN before `pnpm dev`.
    proxy: {
      "/api": {
        target: "http://127.0.0.1:3000",
        changeOrigin: false,
        rewrite: (p) => p.replace(/^\/api/, "/dev/api"),
        configure: (proxy) => {
          proxy.on("proxyReq", (proxyReq) => {
            const t = process.env.BYONK_ADMIN_TOKEN;
            if (t) proxyReq.setHeader("authorization", `Bearer ${t}`);
          });
        },
      },
    },
  },
});
