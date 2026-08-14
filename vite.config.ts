import { defineConfig, type Plugin } from "vite";
import vue from "@vitejs/plugin-vue";

/**
 * Vite pre-bundles dependencies (optimizeDeps) lazily on the FIRST request.
 * That first request stalls until esbuild finishes — a multi-second white
 * screen on the initial navigation into the admin after `vite` starts. This
 * plugin fires an early background request once the server is listening so
 * the pre-bundle runs while the user is still on the chat view.
 */
function prewarmDevServer(): Plugin {
  return {
    name: "piter-prewarm-devdeps",
    configureServer(server) {
      server.httpServer?.once("listening", () => {
        const port = server.config.server.port ?? 1420;
        fetch(`http://localhost:${port}/`).catch(() => {});
      });
    },
  };
}

export default defineConfig({
  plugins: [vue(), prewarmDevServer()],
  clearScreen: false,
  // Usage tab (echarts) chunk is ~595KB min — verified lower bound after
  // tree-shaking; raise the warning limit instead of splitting it further.
  build: {
    chunkSizeWarningLimit: 700,
  },
  server: {
    port: 1420,
    strictPort: true,
    proxy: {
      "/api": {
        target: "http://localhost:31421",
        changeOrigin: true,
      },
      "/ws": {
        target: "ws://localhost:31421",
        ws: true,
      },
      "/chat-ws": {
        target: "ws://localhost:31421",
        ws: true,
      },
      "/work-ws": {
        target: "ws://localhost:31421",
        ws: true,
      },
    },
  },
  optimizeDeps: {
    // @piter/ui ships source SFCs — exclude from esbuild pre-bundling so the
    // vue plugin compiles them.
    exclude: ["@piter/ui"],
    // echarts/vue-echarts are loaded via dynamic import (Usage tab) — without
    // this they would NOT be pre-bundled and dev would fetch hundreds of
    // individual source modules on first open. lucide-vue-next is a large
    // icon library worth pre-bundling up front too.
    include: ["echarts", "vue-echarts", "lucide-vue-next"],
  },
});
