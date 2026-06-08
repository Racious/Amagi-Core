// 產生 Amagi-Core 全套應用圖標
// 策略：尺寸 < 96px 採用 Minimal A（小尺寸清晰），>= 96px 採用 Bright Core（大圖門面）
// .ico（Windows，主要顯示於標題列/檔案總管小圖）整顆用 Minimal A
// .icns（macOS Dock 大圖）用 Bright Core
import sharp from 'sharp';
import png2icons from 'png2icons';
import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
// 全尺寸統一使用深底 concept 圓角版（黑色四角已去透明）
const SRC_BRIGHT = join(root, 'public/amagi-core-icon-rounded.png');
const SRC_MINIMAL = join(root, 'public/amagi-core-icon-rounded.png');
const ICONS = join(root, 'src-tauri/icons');
const PUBLIC = join(root, 'public');

const SMALL_THRESHOLD = 96;
const pick = (size) => (size < SMALL_THRESHOLD ? SRC_MINIMAL : SRC_BRIGHT);

// 尺寸對照的 PNG 檔案（檔名 -> 邊長）
const pngTargets = {
  '32x32.png': 32,
  '128x128.png': 128,
  '128x128@2x.png': 256,
  'icon.png': 512,
  'Square30x30Logo.png': 30,
  'Square44x44Logo.png': 44,
  'Square71x71Logo.png': 71,
  'Square89x89Logo.png': 89,
  'Square107x107Logo.png': 107,
  'Square142x142Logo.png': 142,
  'Square150x150Logo.png': 150,
  'Square284x284Logo.png': 284,
  'Square310x310Logo.png': 310,
  'StoreLogo.png': 50,
};

async function resize(src, size, out) {
  await sharp(src)
    .resize(size, size, { fit: 'cover' })
    .ensureAlpha() // Tauri 要求 icon 必須為 RGBA
    .png()
    .toFile(out);
}

for (const [name, size] of Object.entries(pngTargets)) {
  const src = pick(size);
  const tag = src === SRC_MINIMAL ? 'Minimal A' : 'Bright Core';
  await resize(src, size, join(ICONS, name));
  console.log(`PNG  ${name.padEnd(22)} ${String(size).padStart(4)}px  <- ${tag}`);
}

// .ico（Windows）：Minimal A，含多尺寸
const minimalBuf = await sharp(SRC_MINIMAL).resize(1024, 1024).ensureAlpha().png().toBuffer();
const ico = png2icons.createICO(minimalBuf, png2icons.BICUBIC2, 0, false);
writeFileSync(join(ICONS, 'icon.ico'), ico);
console.log('ICO  icon.ico              multi   <- Minimal A');

// .icns（macOS）：Bright Core
const brightBuf = await sharp(SRC_BRIGHT).resize(1024, 1024).ensureAlpha().png().toBuffer();
const icns = png2icons.createICNS(brightBuf, png2icons.BICUBIC2, 0);
writeFileSync(join(ICONS, 'icon.icns'), icns);
console.log('ICNS icon.icns             multi   <- Bright Core');

// favicon（瀏覽器分頁，小尺寸）：Minimal A
const favIco = png2icons.createICO(minimalBuf, png2icons.BICUBIC2, 0, false);
writeFileSync(join(PUBLIC, 'favicon.ico'), favIco);
await resize(SRC_MINIMAL, 32, join(PUBLIC, 'favicon.png'));
console.log('FAV  favicon.ico/.png      <- Minimal A');

console.log('\n完成。');
