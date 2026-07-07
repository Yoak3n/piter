import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";

export default defineConfig({
  plugins: [vue()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    proxy: {
      "/api": {
        target: "http://localhost:1421",
        changeOrigin: true,
      },
      "/ws": {
        target: "ws://localhost:1421",
        ws: true,
      },
    },
  },
});
