// 產生 Amagi-Core 全套應用圖標
// 來源：public/amagi-core-icon-rounded.png（深底圓角，黑角已透明）
// PNG 各尺寸與 .ico 皆以 sharp 高品質縮放；.ico 用 png-to-ico 組裝（不再二次縮放失真）
import sharp from 'sharp';
import png2icons from 'png2icons';
import pngToIco from 'png-to-ico';
import { writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const SRC = join(root, 'public/amagi-core-icon-rounded.png');
const ICONS = join(root, 'src-tauri/icons');
const PUBLIC = join(root, 'public');

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

// 以 sharp 高品質縮放產生指定尺寸 PNG buffer（Tauri 要求 RGBA）
const pngBuffer = (size) =>
  sharp(SRC).resize(size, size, { fit: 'cover' }).ensureAlpha().png().toBuffer();

for (const [name, size] of Object.entries(pngTargets)) {
  writeFileSync(join(ICONS, name), await pngBuffer(size));
  console.log(`PNG  ${name.padEnd(22)} ${String(size).padStart(4)}px`);
}

// .ico（Windows）：sharp 產各尺寸 -> png-to-ico 組裝，避免 png2icons 小尺寸雜點
const ICO_SIZES = [16, 24, 32, 48, 64, 128, 256];
const icoBuffers = await Promise.all(ICO_SIZES.map(pngBuffer));
writeFileSync(join(ICONS, 'icon.ico'), await pngToIco(icoBuffers));
console.log(`ICO  icon.ico              [${ICO_SIZES.join(',')}]`);

// .icns（macOS）：png2icons（大尺寸為主，Windows 不使用）
const bigBuf = await sharp(SRC).resize(1024, 1024).ensureAlpha().png().toBuffer();
writeFileSync(join(ICONS, 'icon.icns'), png2icons.createICNS(bigBuf, png2icons.BICUBIC2, 0));
console.log('ICNS icon.icns             multi');

// favicon（瀏覽器分頁）：sharp + png-to-ico
const FAV_SIZES = [16, 32, 48];
const favBuffers = await Promise.all(FAV_SIZES.map(pngBuffer));
writeFileSync(join(PUBLIC, 'favicon.ico'), await pngToIco(favBuffers));
writeFileSync(join(PUBLIC, 'favicon.png'), await pngBuffer(32));
console.log(`FAV  favicon.ico/.png      [${FAV_SIZES.join(',')}]`);

console.log('\n完成。');
