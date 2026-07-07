import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import path from "path";

const root = __dirname;

export default defineConfig({
  plugins: [vue()],
  root,
  base: "/",
  resolve: {
    alias: {
      "@": path.resolve(root, "src"),
    },
  },
  server: {
    fs: {
      allow: [".."],
    },
  },
  build: {
    outDir: "dist",
    emptyOutDir: true,
  },
});
