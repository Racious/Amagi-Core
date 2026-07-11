# AMAGI Core 產品介紹網站

AMAGI Core 的官方產品介紹站。網站以同一份來源支援兩種發布形式：

- **Sites／Cloudflare Worker**：保留 vinext 的 SSR、metadata 與 Sites 發布能力。
- **GitHub Pages**：由建置後的 SSR HTML 產生無執行腳本的純靜態版本，發布在專案子路徑 `/Amagi-Core/`。

## 本機開發

需求：Node.js 22.13 以上。

```bash
npm ci
npm run dev
```

開發伺服器預設位於 `http://localhost:3000/`。

## 常用指令

```bash
npm run build
```

建置 Sites／Cloudflare Worker 版本到 `dist/`。

```bash
npm run pages:build
```

先建置 Sites 版本，再由 `scripts/build-pages.mjs` 產生專案根目錄的 `website-pages/` 靜態成品。此資料夾由 Git 忽略，GitHub Actions 會在部署時重新產生。

```bash
npm test
```

依序執行 Sites build、GitHub Pages 靜態產生與三組契約測試，驗證：

- SSR 能輸出 AMAGI Core 首頁與正確 metadata。
- starter preview 已完全移除。
- Pages HTML 無 React 執行腳本，圖片與字型資產適用 `/Amagi-Core/` 子路徑。

## 主要檔案

- `app/page.tsx`：產品頁內容。
- `app/globals.css`：視覺設計與響應式規則。
- `app/layout.tsx`：Sites 版 SEO、Open Graph 與自帶字型。
- `scripts/build-pages.mjs`：GitHub Pages 靜態產生器。
- `tests/rendered-html.test.mjs`：雙輸出契約測試。
- `.openai/hosting.json`：Sites 的邏輯資源設定；不得放置 credential。

## GitHub Pages

專案根目錄的 `.github/workflows/pages.yml` 會在 `main` 的 `website/**` 變更時：

1. 安裝依賴。
2. 執行 `npm run pages:build`。
3. 將 `website-pages/` 上傳為 Pages artifact。
4. 部署到 `https://racious.github.io/Amagi-Core/`。

GitHub Repository Settings → Pages → Source 必須選擇 **GitHub Actions**。

## 資產原則

- 導覽與頁尾使用 `public/amagi-core-ui.png`（96px 最佳化版本）。
- 高解析社群分享圖使用 `public/og.png`。
- 字型由 Fontsource 套件自帶，Sites 與 Pages 共用相同編譯 CSS；瀏覽器依 `unicode-range` 載入實際需要的 CJK 子集。
