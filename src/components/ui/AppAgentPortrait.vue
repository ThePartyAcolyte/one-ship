<script setup lang="ts">
import { ref, computed, watch, onMounted } from 'vue';

const props = withDefaults(defineProps<{
  dna?: string | null;
  agentId?: string | null;
  size?: number; // Target square size in pixels, default 48 (1:1 bottom-anchored crop of 64x96 tile)
}>(), {
  size: 48,
});

interface LayerAsset {
  id: string;
  path: string;
  name: string;
  gender?: string;
}

interface Manifest {
  version: string;
  tileSize: { width: number; height: number };
  layers: {
    base: LayerAsset[];
    expression: LayerAsset[];
    beard: LayerAsset[];
    hair: LayerAsset[];
    accessory: LayerAsset[];
  };
}

// Global cached manifest and loaded images cache
let globalManifest: Manifest | null = null;
let manifestPromise: Promise<Manifest | null> | null = null;
const imageCache = new Map<string, HTMLImageElement>();

async function fetchManifest(): Promise<Manifest | null> {
  if (globalManifest) return globalManifest;
  if (!manifestPromise) {
    manifestPromise = fetch('/portraits/manifest.json')
      .then(res => res.ok ? res.json() : null)
      .catch(() => null);
  }
  globalManifest = await manifestPromise;
  return globalManifest;
}

function loadImage(src: string): Promise<HTMLImageElement | null> {
  if (imageCache.has(src)) {
    return Promise.resolve(imageCache.get(src)!);
  }
  return new Promise((resolve) => {
    const img = new Image();
    img.crossOrigin = 'anonymous';
    img.onload = () => {
      imageCache.set(src, img);
      resolve(img);
    };
    img.onerror = () => resolve(null);
    img.src = src.startsWith('/') ? src : `/${src}`;
  });
}

function generateFallbackDna(id?: string | null): string {
  if (!id) return 'B01:E0:H00:BD00:A00';
  let hash = 0;
  for (let i = 0; i < id.length; i++) {
    hash = (hash << 5) - hash + id.charCodeAt(i);
    hash |= 0;
  }
  const posHash = Math.abs(hash);
  const baseIdx = (posHash % 14) + 1;
  const exprIdx = (posHash >> 4) % 6;
  const pad = (n: number) => String(n).padStart(2, '0');
  const hairIdx = (posHash >> 8) % 22;
  const beardIdx = (posHash >> 12) % 6;
  const accIdx = (posHash >> 16) % 17;

  return `B${pad(baseIdx)}:E${exprIdx}:H${pad(hairIdx)}:BD${pad(beardIdx)}:A${pad(accIdx)}`;
}

function getDnaHash(str: string): number {
  let hash = 0;
  for (let i = 0; i < str.length; i++) {
    hash = (hash << 5) - hash + str.charCodeAt(i);
    hash |= 0;
  }
  return Math.abs(hash);
}

const effectiveDna = computed(() => {
  if (props.dna && props.dna.includes(':')) return props.dna;
  return generateFallbackDna(props.agentId);
});

const canvasRef = ref<HTMLCanvasElement | null>(null);
const isReady = ref(false);

const width = computed(() => props.size);
const height = computed(() => props.size);

const TILE_WIDTH_PX = 64;
const TILE_HEIGHT_PX = 96;
const CROP_SIZE_PX = 64;
const CROP_TOP_OFFSET_PX = TILE_HEIGHT_PX - CROP_SIZE_PX; // Anchor square crop to bottom of 64x96 tile

interface LayerItem {
  path: string;
  explicitFrame?: number;
}

const FEMININE_BASE_NUMS = new Set([5, 6, 11, 12]);
const SKIN_PALETTES = [
  ['#f2d3b1', '#d9ad84'],
  ['#e4b892', '#c38f67'],
  ['#c98c5c', '#9f663c'],
  ['#9b6440', '#754629'],
  ['#6b442b', '#4d2e1b'],
  ['#efd2c6', '#cca293'],
];
const HAIR_PALETTES = [
  '#1f232b', '#3a2b22', '#6b4226', '#9e6234', '#c99b4a',
  '#8c2f24', '#5c6678', '#cbd5e1', '#2b5b6c', '#5b3c70',
];
const SHIRT_PALETTES = [
  ['#2b4c6f', '#1d344d'],
  ['#335c49', '#223f32'],
  ['#6e3438', '#4b2225'],
  ['#51436e', '#362c4a'],
  ['#7a5429', '#54391b'],
  ['#3a4654', '#262f3a'],
];
const BACKDROP_PALETTES = ['#151b26', '#18211f', '#211924', '#1d1c18', '#161c24'];

function parseNumericToken(part: string, prefix: string): number {
  const clean = part.startsWith(prefix) ? part.slice(prefix.length) : part;
  const num = parseInt(clean, 10);
  return Number.isFinite(num) ? Math.abs(num) : 0;
}

/**
 * Deterministic MIT-clean procedural 16x16 pixel portrait rendered onto the 64x64 canvas
 * from `AgentPortraitDna` whenever no local sprite pack (`/portraits/manifest.json`) is present.
 */
function renderProceduralPortrait(ctx: CanvasRenderingContext2D, dna: string, agentId: string | null | undefined) {
  const scale = 4; // 16 * 4 = 64px crisp pixel grid
  const px = (x: number, y: number, w = 1, h = 1) => {
    ctx.fillRect(x * scale, y * scale, w * scale, h * scale);
  };

  const parts = dna.split(':');
  const baseNum = parseNumericToken(parts[0] || 'B01', 'B');
  const exprNum = parseNumericToken(parts[1] || 'E0', 'E');
  const hairNum = parseNumericToken(parts[2] || 'H00', 'H');
  const beardNum = parseNumericToken(parts[3] || 'BD00', 'BD');
  const accNum = parseNumericToken(parts[4] || 'A00', 'A');
  const seed = getDnaHash(`${agentId || ''}:${dna}`);
  const isFeminine = FEMININE_BASE_NUMS.has(baseNum);

  const [skin, skinShade] = SKIN_PALETTES[(baseNum + seed) % SKIN_PALETTES.length];
  const hairColor = HAIR_PALETTES[(hairNum + (seed >> 3)) % HAIR_PALETTES.length];
  const [shirt, shirtShade] = SHIRT_PALETTES[(baseNum + accNum + (seed >> 5)) % SHIRT_PALETTES.length];
  const bg = BACKDROP_PALETTES[seed % BACKDROP_PALETTES.length];

  // Backdrop
  ctx.fillStyle = bg;
  px(0, 0, 16, 16);

  // Back hair silhouette for longer feminine styles
  if (isFeminine && hairNum !== 0 && hairNum % 3 !== 0) {
    ctx.fillStyle = hairColor;
    px(3, 4, 10, 9);
  }

  // Shoulders & collar
  ctx.fillStyle = shirtShade;
  px(2, 13, 12, 3);
  ctx.fillStyle = shirt;
  px(3, 13, 10, 3);
  ctx.fillStyle = skinShade;
  px(6, 11, 4, 3);

  // Face / head block
  const faceX = isFeminine ? 5 : 4;
  const faceW = isFeminine ? 6 : 8;
  ctx.fillStyle = skinShade;
  px(faceX, 4, faceW, 8);
  ctx.fillStyle = skin;
  px(faceX, 4, faceW, 7);
  // Ears
  ctx.fillStyle = skinShade;
  px(faceX - 1, 7, 1, 2);
  px(faceX + faceW, 7, 1, 2);

  // Eyes (row 7)
  const leftEyeX = isFeminine ? 6 : 5;
  const rightEyeX = isFeminine ? 9 : 10;
  ctx.fillStyle = '#0f172a';
  if (exprNum === 5) {
    // Surprised wide eyes
    px(leftEyeX, 6, 1, 2);
    px(rightEyeX, 6, 1, 2);
  } else {
    px(leftEyeX, 7, 1, 1);
    px(rightEyeX, 7, 1, 1);
  }

  // Brows (row 6) for annoyed/angry/surprised
  ctx.fillStyle = hairColor;
  if (exprNum === 3 || exprNum === 4) {
    px(leftEyeX, 6, 2, 1);
    px(rightEyeX - 1, 6, 2, 1);
  } else if (exprNum === 5) {
    px(leftEyeX, 5, 1, 1);
    px(rightEyeX, 5, 1, 1);
  }

  // Mouth / Expression (row 9-10)
  ctx.fillStyle = exprNum === 1 ? '#f8fafc' : '#5b3428';
  if (exprNum === 1) {
    // Happy
    px(7, 9, 2, 1);
  } else if (exprNum === 2 || exprNum === 4) {
    // Sad / Angry frown
    px(7, 10, 2, 1);
  } else if (exprNum === 5) {
    // Surprised
    px(7, 9, 1, 2);
  } else {
    // Default / Annoyed
    px(7, 9, 2, 1);
  }

  // Beard (suppressed on feminine bases)
  if (beardNum !== 0 && !isFeminine) {
    ctx.fillStyle = hairColor;
    if (beardNum % 3 === 1) {
      // Goatee
      px(6, 10, 4, 2);
    } else if (beardNum % 3 === 2) {
      // Mustache + chin
      px(6, 9, 4, 1);
      px(7, 11, 2, 1);
    } else {
      // Full beard
      px(4, 9, 1, 3);
      px(11, 9, 1, 3);
      px(5, 10, 6, 2);
    }
  }

  // Hair styles (H00 = bald)
  if (hairNum !== 0) {
    ctx.fillStyle = hairColor;
    const style = hairNum % 5;
    px(4, 2, 8, 2);
    if (style === 1) {
      // Side sweep
      px(3, 3, 2, 4);
      px(10, 3, 2, 2);
    } else if (style === 2) {
      // Mohawk / crest
      px(6, 1, 4, 2);
      px(4, 3, 8, 1);
    } else if (style === 3) {
      // Short crop
      px(4, 3, 8, 2);
    } else if (style === 4) {
      // Curly / wide top
      px(3, 2, 10, 3);
      px(3, 5, 1, 2);
      px(12, 5, 1, 2);
    } else {
      // Classic parted
      px(4, 3, 3, 2);
      px(8, 3, 4, 2);
    }
  }

  // Accessories (A00 = none)
  if (accNum !== 0) {
    const kind = accNum % 4;
    if (kind === 1) {
      // Glasses
      ctx.fillStyle = '#38bdf8';
      px(leftEyeX - 1, 7, 2, 1);
      px(rightEyeX, 7, 2, 1);
      ctx.fillStyle = '#94a3b8';
      px(leftEyeX + 1, 7, rightEyeX - leftEyeX - 1, 1);
    } else if (kind === 2) {
      // Tech visor
      ctx.fillStyle = '#22d3ee';
      px(faceX, 7, faceW, 1);
    } else if (kind === 3) {
      // Headset / comms earpiece
      ctx.fillStyle = '#f59e0b';
      px(faceX - 1, 6, 1, 3);
      px(faceX - 1, 9, 2, 1);
    } else {
      // Bandana / headband
      ctx.fillStyle = '#ef4444';
      px(4, 4, 8, 1);
    }
  }
}

async function renderPortrait() {
  if (!canvasRef.value) return;

  const canvas = canvasRef.value;
  const ctx = canvas.getContext('2d');
  if (!ctx) return;

  ctx.imageSmoothingEnabled = false;
  ctx.clearRect(0, 0, CROP_SIZE_PX, CROP_SIZE_PX);

  const manifest = await fetchManifest();
  if (!manifest || !manifest.layers.base.length) {
    renderProceduralPortrait(ctx, effectiveDna.value, props.agentId);
    isReady.value = true;
    return;
  }

  // Parse DNA components
  const parts = effectiveDna.value.split(':');
  const baseId = parts[0] || 'B01';
  const exprId = parts[1] || 'E0';
  const hairId = parts[2] || 'H00';
  const beardId = parts[3] || 'BD00';
  const accId = parts[4] || 'A00';

  const layersToDraw: LayerItem[] = [];

  // 1. Base Layer
  const baseAsset = manifest.layers.base.find(b => b.id === baseId) || manifest.layers.base[0];
  if (baseAsset?.path) layersToDraw.push({ path: baseAsset.path });

  // 2. Expression Layer (if overlay available)
  if (exprId !== 'E0') {
    const exprAsset = manifest.layers.expression?.find(e => e.id === exprId);
    if (exprAsset?.path) layersToDraw.push({ path: exprAsset.path });
  }

  // 2. Beard Layer (Suppress on feminine base faces)
  const isFeminineBase = baseAsset?.gender === 'feminine';
  if (beardId !== 'BD00' && !isFeminineBase) {
    const beardAsset = manifest.layers.beard.find(b => b.id === beardId);
    if (beardAsset?.path) layersToDraw.push({ path: beardAsset.path });
  }

  // 3. Hair Layer
  if (hairId !== 'H00') {
    const [hairBaseId, hairFrameStr] = hairId.split(/[-.]/);
    const hairAsset = manifest.layers.hair.find(h => h.id === hairBaseId || h.id === hairId);
    if (hairAsset?.path) {
      const parsedFrame = hairFrameStr ? parseInt(hairFrameStr, 10) : undefined;
      const explicitFrame = (parsedFrame !== undefined && !isNaN(parsedFrame)) ? parsedFrame : undefined;
      layersToDraw.push({ path: hairAsset.path, explicitFrame });
    }
  }

  // 4. Accessory Layer
  if (accId !== 'A00') {
    const accAsset = manifest.layers.accessory.find(a => a.id === accId);
    if (accAsset?.path) layersToDraw.push({ path: accAsset.path });
  }

  // Load and draw layers sequentially
  for (const layer of layersToDraw) {
    const img = await loadImage(layer.path);
    if (img && ctx) {
      const imgWidth = img.naturalWidth || img.width;
      const imgHeight = img.naturalHeight || img.height;
      const cols = Math.max(1, Math.floor(imgWidth / TILE_WIDTH_PX));
      const rows = Math.max(1, Math.floor(imgHeight / TILE_HEIGHT_PX));
      const totalFrames = cols * rows;

      if (totalFrames > 1) {
        let frameIndex = 0;
        if (layer.explicitFrame !== undefined) {
          frameIndex = Math.abs(layer.explicitFrame) % totalFrames;
        } else {
          const hashVal = getDnaHash(`${props.agentId || ''}:${effectiveDna.value}:${layer.path}`);
          frameIndex = hashVal % totalFrames;
        }
        const col = frameIndex % cols;
        const row = Math.floor(frameIndex / cols);
        const sx = col * TILE_WIDTH_PX;
        const sy = row * TILE_HEIGHT_PX;
        ctx.drawImage(
          img,
          sx,
          sy + CROP_TOP_OFFSET_PX,
          CROP_SIZE_PX,
          CROP_SIZE_PX,
          0,
          0,
          CROP_SIZE_PX,
          CROP_SIZE_PX,
        );
      } else {
        ctx.drawImage(
          img,
          0,
          CROP_TOP_OFFSET_PX,
          CROP_SIZE_PX,
          CROP_SIZE_PX,
          0,
          0,
          CROP_SIZE_PX,
          CROP_SIZE_PX,
        );
      }
    }
  }

  isReady.value = true;
}

watch([effectiveDna, () => props.size], () => {
  renderPortrait();
}, { immediate: true });

onMounted(() => {
  renderPortrait();
});
</script>

<template>
  <div 
    class="inline-block relative shrink-0 border border-[var(--border-color)]/40 ui-surface-list overflow-hidden select-none"
    :style="{ width: `${width}px`, height: `${height}px` }"
  >
    <canvas
      ref="canvasRef"
      width="64"
      height="64"
      class="w-full h-full object-cover block"
      style="image-rendering: pixelated; image-rendering: crisp-edges;"
    />
  </div>
</template>
