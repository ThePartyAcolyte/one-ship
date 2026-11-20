import fs from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const rootDir = path.resolve(__dirname, '..');
const xdgDataHome = process.env.XDG_DATA_HOME || (process.env.HOME ? path.join(process.env.HOME, '.local', 'share') : '');
const candidateSources = [
  process.env.ONESHIP_PORTRAITS_SRC,
  path.join(rootDir, 'extras'),
  xdgDataHome ? path.join(xdgDataHome, 'one-ship', 'portraits-src') : null,
].filter(Boolean);
const extrasDir = candidateSources.find((dir) =>
  fs.existsSync(dir) &&
  (fs.existsSync(path.join(dir, 'Pixel-Portraits-Free')) || fs.existsSync(path.join(dir, 'Pixel-Portraits-Addon')))
);
const targetDir = path.join(rootDir, 'public', 'portraits');
const targetAssetsDir = path.join(targetDir, 'assets');

if (!extrasDir) {
  console.log('[Portrait Catalog Builder] No local Pixel-Portraits pack found; procedural SVG/canvas portraits will be used by default.');
  console.log('Set ONESHIP_PORTRAITS_SRC=/path/to/portraits-dir to import a locally owned sprite pack.');
  process.exit(0);
}

const EXCLUDED_KEYWORDS = ['bird', 'frog', 'minotaur', 'orc', 'skeleton', 'wolf', 'tusk'];

function isExcluded(filePath) {
  const lower = filePath.toLowerCase();
  return EXCLUDED_KEYWORDS.some(kw => lower.includes(kw));
}

function ensureDir(dir) {
  if (fs.existsSync(dir)) {
    fs.rmSync(dir, { recursive: true, force: true });
  }
  fs.mkdirSync(dir, { recursive: true });
}

ensureDir(targetAssetsDir);

const manifest = {
  version: "1.0",
  tileSize: { width: 64, height: 96 },
  layers: {
    base: [],
    expression: [],
    beard: [],
    hair: [],
    accessory: []
  }
};

const EXPRESSIONS = [
  { id: "E0", name: "default", suffix: "default" },
  { id: "E1", name: "happy", suffix: "happy" },
  { id: "E2", name: "sad", suffix: "sad" },
  { id: "E3", name: "annoyed", suffix: "annoyed" },
  { id: "E4", name: "angry", suffix: "angry" },
  { id: "E5", name: "surprised", suffix: "surprised" }
];

manifest.layers.expression = EXPRESSIONS;

let assetCounter = 0;
function copyAsset(sourcePath, category, nameHint) {
  if (isExcluded(sourcePath)) return null;
  assetCounter++;
  const ext = path.extname(sourcePath);
  const safeName = `${category}_${assetCounter}${ext}`;
  const destPath = path.join(targetAssetsDir, safeName);
  fs.copyFileSync(sourcePath, destPath);
  return {
    path: `portraits/assets/${safeName}`,
    name: nameHint || safeName
  };
}

function scanDir(dir) {
  if (!fs.existsSync(dir)) return;
  const entries = fs.readdirSync(dir, { withFileTypes: true });
  for (const entry of entries) {
    const fullPath = path.join(dir, entry.name);
    if (isExcluded(fullPath)) continue;

    if (entry.isDirectory()) {
      scanDir(fullPath);
    } else if (entry.isFile() && entry.name.toLowerCase().endsWith('.png')) {
      const lowerName = entry.name.toLowerCase();
      
      // Base skins categorization with strict gender alignment
      if (lowerName.includes('feminine') && (lowerName.includes('default') || lowerName.includes('base'))) {
        if (!lowerName.includes('sheet') && !lowerName.includes('hair')) {
          const copied = copyAsset(fullPath, 'base', entry.name);
          if (copied) {
            const id = `B${String(manifest.layers.base.length + 1).padStart(2, '0')}`;
            manifest.layers.base.push({ id, gender: 'feminine', ...copied });
          }
        }
      } else if (lowerName.includes('masculine') && (lowerName.includes('default') || lowerName.includes('base'))) {
        if (!lowerName.includes('sheet') && !lowerName.includes('hair') && !lowerName.includes('beard')) {
          const copied = copyAsset(fullPath, 'base', entry.name);
          if (copied) {
            const id = `B${String(manifest.layers.base.length + 1).padStart(2, '0')}`;
            manifest.layers.base.push({ id, gender: 'masculine', ...copied });
          }
        }
      } else if (lowerName.match(/human\d\.png/) || lowerName.includes('human_1-default') || lowerName.includes('human_2-default')) {
        const copied = copyAsset(fullPath, 'base', entry.name);
        if (copied) {
          const id = `B${String(manifest.layers.base.length + 1).padStart(2, '0')}`;
          manifest.layers.base.push({ id, gender: 'masculine', ...copied });
        }
      } else if (lowerName.includes('hair')) {
        const copied = copyAsset(fullPath, 'hair', entry.name);
        if (copied) {
          const id = `H${String(manifest.layers.hair.length + 1).padStart(2, '0')}`;
          manifest.layers.hair.push({ id, ...copied });
        }
      } else if (lowerName.includes('beard')) {
        const copied = copyAsset(fullPath, 'beard', entry.name);
        if (copied) {
          const id = `BD${String(manifest.layers.beard.length + 1).padStart(2, '0')}`;
          manifest.layers.beard.push({ id, ...copied });
        }
      } else if (lowerName.includes('glasses') || lowerName.includes('accessory') || lowerName.includes('bandana') || lowerName.includes('hat') || lowerName.includes('chain') || lowerName.includes('paint')) {
        const copied = copyAsset(fullPath, 'accessory', entry.name);
        if (copied) {
          const id = `A${String(manifest.layers.accessory.length + 1).padStart(2, '0')}`;
          manifest.layers.accessory.push({ id, ...copied });
        }
      }
    }
  }
}

scanDir(extrasDir);

// Fallbacks if empty
if (manifest.layers.base.length === 0) {
  manifest.layers.base.push({ id: "B01", gender: "masculine", name: "Default Base", path: "" });
}
if (manifest.layers.hair.length === 0) {
  manifest.layers.hair.push({ id: "H00", name: "Bald / None", path: "" });
} else {
  manifest.layers.hair.unshift({ id: "H00", name: "Bald / None", path: "" });
}
if (manifest.layers.beard.length === 0) {
  manifest.layers.beard.push({ id: "BD00", name: "Clean Shaven", path: "" });
} else {
  manifest.layers.beard.unshift({ id: "BD00", name: "Clean Shaven", path: "" });
}
if (manifest.layers.accessory.length === 0) {
  manifest.layers.accessory.push({ id: "A00", name: "None", path: "" });
} else {
  manifest.layers.accessory.unshift({ id: "A00", name: "None", path: "" });
}

fs.writeFileSync(
  path.join(targetDir, 'manifest.json'),
  JSON.stringify(manifest, null, 2)
);

const feminineCount = manifest.layers.base.filter(b => b.gender === 'feminine').length;
const masculineCount = manifest.layers.base.filter(b => b.gender === 'masculine').length;

console.log(`[Portrait Catalog Builder] Manifest created successfully!`);
console.log(`- Base skins: ${manifest.layers.base.length} (Feminine: ${feminineCount}, Masculine: ${masculineCount})`);
console.log(`- Expressions: ${manifest.layers.expression.length}`);
console.log(`- Hair styles: ${manifest.layers.hair.length}`);
console.log(`- Beard styles: ${manifest.layers.beard.length}`);
console.log(`- Accessories: ${manifest.layers.accessory.length}`);
