import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import tailwindcss from "@tailwindcss/vite";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [vue(), tailwindcss()],

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      // 4. 忽略 app 自己生成的 agent 檔／.amagi 軌跡——否則 dogfood 同步 Amagi-Core 自身目錄時，
      //    寫出 AGENTS.md/CLAUDE.md 會被 vite 監看到而觸發整頁重載（沖掉同步成功訊息等狀態）。
      ignored: ["**/src-tauri/**", "**/AGENTS.md", "**/CLAUDE.md", "**/.amagi/**"],
    },
  },
}));
