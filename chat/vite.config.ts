import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import path from "path";

const root = __dirname;

export default defineConfig({
  plugins: [vue()],
  root,
  // SPA base = 挂载前缀（与 work Flutter 的 --base-href=/work/ 统一约定）：
  // gateway 的 spa_fallback 按 /chat 前缀分发到 chat 产物，资源以 /chat/ 开头
  // 才能命中；不再依赖"根路径=chat_dist"的兜底。
  base: "/chat/",
  resolve: {
    alias: {
      "@": path.resolve(root, "src"),
    },
  },
  server: {
    fs: {
      allow: [".."],
    },
    // chat dev server 直连 gateway：REST 走 /api、WS 走 /chat-ws
    //（与 usePiConnection 的 getWsUrl 归一化保持一致；旧 /ws 兼容保留）。
    proxy: {
      "/api": {
        target: "http://localhost:31421",
        changeOrigin: true,
      },
      "/chat-ws": {
        target: "ws://localhost:31421",
        ws: true,
      },
      "/ws": {
        target: "ws://localhost:31421",
        ws: true,
      },
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
