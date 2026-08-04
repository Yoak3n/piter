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
  // @piter/ui ships source SFCs — let the vue plugin compile them instead of
  // letting esbuild pre-bundle (esbuild cannot parse .vue).
  optimizeDeps: {
    exclude: ["@piter/ui"],
  },
  build: {
    outDir: 'dist',
    emptyOutDir: true,
  },
});
