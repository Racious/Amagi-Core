// 去除 concept 圖圓角外的純黑四角 -> 透明，並裁掉透明邊距讓方塊本體占滿畫布
import sharp from 'sharp';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const SRC = join(root, 'public/amagi-core-icon-concept.png');
const OUT = join(root, 'public/amagi-core-icon-rounded.png');

const TOL = 42;       // 純黑容差
const PAD_RATIO = 0.03; // 裁切後保留的呼吸邊距

const { data: px, info } = await sharp(SRC).ensureAlpha().raw().toBuffer({ resolveWithObject: true });
const { width: w, height: h, channels: c } = info;

// 四角平均當純黑參考
const corners = [[0, 0], [w - 1, 0], [0, h - 1], [w - 1, h - 1]];
let kr = 0, kg = 0, kb = 0;
for (const [x, y] of corners) { const o = (y * w + x) * c; kr += px[o]; kg += px[o + 1]; kb += px[o + 2]; }
kr /= 4; kg /= 4; kb /= 4;

const isBlack = (i) => {
  const o = i * c;
  const dr = px[o] - kr, dg = px[o + 1] - kg, db = px[o + 2] - kb;
  return Math.sqrt(dr * dr + dg * dg + db * db) <= TOL;
};

// 邊緣 floodfill 去黑角
const visited = new Uint8Array(w * h);
const stack = [];
for (let x = 0; x < w; x++) { stack.push(x); stack.push((h - 1) * w + x); }
for (let y = 0; y < h; y++) { stack.push(y * w); stack.push(y * w + (w - 1)); }
while (stack.length) {
  const i = stack.pop();
  if (visited[i]) continue;
  visited[i] = 1;
  if (!isBlack(i)) continue;
  px[i * c + 3] = 0;
  const x = i % w, y = (i / w) | 0;
  if (x > 0) stack.push(i - 1);
  if (x < w - 1) stack.push(i + 1);
  if (y > 0) stack.push(i - w);
  if (y < h - 1) stack.push(i + w);
}

// 計算非透明像素 bounding box
let minX = w, minY = h, maxX = 0, maxY = 0;
for (let y = 0; y < h; y++) {
  for (let x = 0; x < w; x++) {
    if (px[(y * w + x) * c + 3] > 16) {
      if (x < minX) minX = x; if (x > maxX) maxX = x;
      if (y < minY) minY = y; if (y > maxY) maxY = y;
    }
  }
}
// 取正方形外接框並加呼吸邊距
const cx = (minX + maxX) / 2, cy = (minY + maxY) / 2;
const side = Math.max(maxX - minX, maxY - minY);
const half = Math.round(side / 2 * (1 + PAD_RATIO * 2));
let left = Math.max(0, Math.round(cx - half));
let top = Math.max(0, Math.round(cy - half));
let size = Math.min(half * 2, w - left, h - top);
console.log(`bbox ${minX},${minY} -> ${maxX},${maxY}  方塊邊長≈${side}px (原畫布 ${w}px)`);
console.log(`裁切 left=${left} top=${top} size=${size}px`);

const cropped = await sharp(px, { raw: { width: w, height: h, channels: c } })
  .extract({ left, top, width: size, height: size })
  .png()
  .toBuffer();
await sharp(cropped).toFile(OUT);

// 淺底預覽
await sharp({ create: { width: size, height: size, channels: 4, background: { r: 220, g: 222, b: 226, alpha: 1 } } })
  .composite([{ input: cropped }]).png().toFile(join(root, 'public/icon-concepts/_rounded-preview-light.png'));
console.log('完成 ->', OUT);
