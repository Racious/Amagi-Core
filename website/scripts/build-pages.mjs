import { copyFile, mkdir, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const siteRoot = resolve(scriptDir, "..");
const repositoryRoot = resolve(siteRoot, "..");
const outputRoot = resolve(repositoryRoot, "website-pages");
const assetsRoot = resolve(outputRoot, "assets");
const expectedOutputRoot = resolve(repositoryRoot, "website-pages");

if (outputRoot !== expectedOutputRoot || outputRoot === repositoryRoot) {
  throw new Error("Refusing to write outside the expected website-pages directory");
}

const workerPath = resolve(siteRoot, "dist/server/index.js");
const workerUrl = pathToFileURL(workerPath);
workerUrl.searchParams.set("pages-build", Date.now().toString());
const { default: worker } = await import(workerUrl.href);

const response = await worker.fetch(
  new Request("https://racious.github.io/", {
    headers: { accept: "text/html", host: "racious.github.io" },
  }),
  {
    ASSETS: {
      fetch: async () => new Response("Not found", { status: 404 }),
    },
  },
  {
    waitUntil() {},
    passThroughOnException() {},
  },
);

if (!response.ok) {
  throw new Error(`Unable to render the site: HTTP ${response.status}`);
}

const rendered = await response.text();
const bodyMatch = rendered.match(/<body[^>]*>([\s\S]*?)<\/body>/i);

if (!bodyMatch) {
  throw new Error("Rendered site did not contain a body element");
}

const body = bodyMatch[1]
  .replace(/<script\b[^>]*>[\s\S]*?<\/script>/gi, "")
  .replace(/<link\b[^>]*rel=["']stylesheet["'][^>]*>/gi, "")
  .replaceAll('src="/amagi-core-ui.png"', 'src="assets/amagi-core-ui.png"')
  .trim();

const compiledAssetsRoot = resolve(siteRoot, "dist/client/assets");
const compiledCssFiles = (await readdir(compiledAssetsRoot))
  .filter((file) => file.endsWith(".css"))
  .sort();
const compiledFontFiles = (await readdir(compiledAssetsRoot))
  .filter((file) => /\.(?:woff2?|ttf|otf)$/i.test(file))
  .sort();

if (compiledCssFiles.length === 0) {
  throw new Error("Sites build did not emit any compiled CSS assets");
}

const compiledCss = (
  await Promise.all(
    compiledCssFiles.map((file) => readFile(resolve(compiledAssetsRoot, file), "utf8")),
  )
).join("\n");
const staticCss = compiledCss.replace(/url\(\s*(["']?)\/assets\//gi, "url($1./");
const pageUrl = "https://racious.github.io/Amagi-Core/";
const socialImageUrl = `${pageUrl}assets/og.png`;

const html = `<!doctype html>
<html lang="zh-Hant">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>AMAGI Core｜AI 記憶與技能同步管家</title>
    <meta name="description" content="把開發變更轉成可審核、可同步、可跨機延續的 AI 專案記憶與技能。支援 Claude Code 與 Codex。" />
    <meta property="og:type" content="website" />
    <meta property="og:locale" content="zh_TW" />
    <meta property="og:site_name" content="AMAGI Core" />
    <meta property="og:title" content="AMAGI Core｜讓 AI 真正記得你的專案" />
    <meta property="og:description" content="可審核、可同步、可跨機延續的 AI 專案記憶與技能中樞。" />
    <meta property="og:url" content="${pageUrl}" />
    <meta property="og:image" content="${socialImageUrl}" />
    <meta name="twitter:card" content="summary_large_image" />
    <meta name="twitter:title" content="AMAGI Core｜讓 AI 真正記得你的專案" />
    <meta name="twitter:description" content="可審核、可同步、可跨機延續的 AI 專案記憶與技能中樞。" />
    <meta name="twitter:image" content="${socialImageUrl}" />
    <link rel="canonical" href="${pageUrl}" />
    <link rel="icon" href="assets/favicon.png" />
    <link rel="stylesheet" href="assets/styles.css" />
  </head>
  <body>${body}</body>
</html>
`;

await rm(outputRoot, { recursive: true, force: true, maxRetries: 8, retryDelay: 250 });
await mkdir(assetsRoot, { recursive: true });
await Promise.all([
  writeFile(resolve(outputRoot, "index.html"), html, "utf8"),
  writeFile(resolve(outputRoot, ".nojekyll"), "", "utf8"),
  writeFile(resolve(assetsRoot, "styles.css"), staticCss, "utf8"),
  copyFile(resolve(siteRoot, "public/amagi-core-ui.png"), resolve(assetsRoot, "amagi-core-ui.png")),
  copyFile(resolve(siteRoot, "public/favicon.png"), resolve(assetsRoot, "favicon.png")),
  copyFile(resolve(siteRoot, "public/og.png"), resolve(assetsRoot, "og.png")),
  ...compiledFontFiles.map((file) =>
    copyFile(resolve(compiledAssetsRoot, file), resolve(assetsRoot, file)),
  ),
]);

console.log(`GitHub Pages static site generated at ${outputRoot}`);
