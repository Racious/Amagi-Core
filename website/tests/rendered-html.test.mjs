import assert from "node:assert/strict";
import { access, readFile, readdir } from "node:fs/promises";
import test from "node:test";

const templateRoot = new URL("../", import.meta.url);

async function render() {
  const workerUrl = new URL("../dist/server/index.js", import.meta.url);
  workerUrl.searchParams.set("test", `${process.pid}-${Date.now()}`);
  const { default: worker } = await import(workerUrl.href);

  return worker.fetch(
    new Request("https://amagi-core.example/", {
      headers: { accept: "text/html", host: "amagi-core.example" },
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
}

test("server-renders the AMAGI Core landing page", async () => {
  const response = await render();
  assert.equal(response.status, 200);
  assert.match(response.headers.get("content-type") ?? "", /^text\/html\b/i);

  const html = await response.text();
  assert.match(html, /<html lang="zh-Hant">/i);
  assert.match(html, /<title>AMAGI Core｜AI 記憶與技能同步管家<\/title>/i);
  assert.match(html, /讓 AI 真正記得/);
  assert.match(html, /核心能力/);
  assert.match(html, /安全設計/);
  assert.match(html, /https:\/\/github\.com\/Racious\/Amagi-Core/);
  assert.match(html, /https:\/\/github\.com\/Racious\/Amagi-Core\/releases\/latest/);
});

test("ships production metadata and removes the starter preview", async () => {
  const [page, layout, packageJson] = await Promise.all([
    readFile(new URL("../app/page.tsx", import.meta.url), "utf8"),
    readFile(new URL("../app/layout.tsx", import.meta.url), "utf8"),
    readFile(new URL("../package.json", import.meta.url), "utf8"),
  ]);

  assert.match(layout, /metadataBase: new URL\(origin\)/);
  assert.match(layout, /images: \["\/og\.png"\]/);
  assert.doesNotMatch(page, /SkeletonPreview|codex-preview/);
  assert.doesNotMatch(layout, /Starter Project|codex-preview/);
  assert.doesNotMatch(packageJson, /react-loading-skeleton/);

  await access(new URL("../public/amagi-core-ui.png", import.meta.url));
  await access(new URL("../public/favicon.png", import.meta.url));
  await access(new URL("../public/og.png", import.meta.url));
  await assert.rejects(access(new URL("../app/_sites-preview", import.meta.url)));
  await assert.rejects(access(new URL("public/_sites-preview", templateRoot)));
});

test("generates a self-contained GitHub Pages contract", async () => {
  const compiledAssetsRoot = new URL("../dist/client/assets/", import.meta.url);
  const compiledCssFiles = (await readdir(compiledAssetsRoot))
    .filter((file) => file.endsWith(".css"))
    .sort();
  const [html, css, compiledCssParts] = await Promise.all([
    readFile(new URL("../../website-pages/index.html", import.meta.url), "utf8"),
    readFile(new URL("../../website-pages/assets/styles.css", import.meta.url), "utf8"),
    Promise.all(compiledCssFiles.map((file) => readFile(new URL(file, compiledAssetsRoot), "utf8"))),
  ]);

  assert.match(html, /<html lang="zh-Hant">/i);
  assert.match(html, /href="assets\/styles\.css"/i);
  assert.match(html, /src="assets\/amagi-core-ui\.png"/i);
  assert.match(html, /https:\/\/racious\.github\.io\/Amagi-Core\/assets\/og\.png/i);
  assert.match(html, /https:\/\/github\.com\/Racious\/Amagi-Core\/releases\/latest/);
  assert.doesNotMatch(html, /<script\b/i);
  assert.doesNotMatch(html, /src="\//i);
  assert.doesNotMatch(css, /@import\s+["']tailwindcss["']/i);
  assert.equal(css, compiledCssParts.join("\n").replace(/url\(\s*(["']?)\/assets\//gi, "url($1./"));
  assert.doesNotMatch(css, /url\(\s*["']?\/assets\//i);
  assert.match(css, /font-family:\s*(?:["'])?Noto Serif TC Variable(?:["'])?/i);
  assert.match(css, /font-family:\s*(?:["'])?Space Grotesk Variable(?:["'])?/i);

  const localFontUrls = [...css.matchAll(/url\((?:["'])?\.\/([^"')]+\.(?:woff2?|ttf|otf))(?:["'])?\)/gi)]
    .map((match) => match[1]);
  assert.ok(localFontUrls.length > 0);
  await Promise.all(
    [...new Set(localFontUrls)].map((file) =>
      access(new URL(`../../website-pages/assets/${file}`, import.meta.url)),
    ),
  );

  await access(new URL("../../website-pages/.nojekyll", import.meta.url));
  await access(new URL("../../website-pages/assets/amagi-core-ui.png", import.meta.url));
  await access(new URL("../../website-pages/assets/favicon.png", import.meta.url));
  await access(new URL("../../website-pages/assets/og.png", import.meta.url));
});
