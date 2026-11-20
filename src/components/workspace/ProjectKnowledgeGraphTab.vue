<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted, nextTick } from 'vue';
import {
  Search, X, RefreshCw, GitFork, FileCode, BookOpen, History,
  CheckSquare, AlertOctagon, ExternalLink, ZoomIn, ZoomOut, Maximize2,
  Network, Sparkles, Layers, Share2, Eye, EyeOff, Plus, Minus, PinOff
} from '@lucide/vue';
import {
  forceSimulation,
  forceLink,
  forceManyBody,
  forceCenter,
  forceCollide,
  forceX,
  forceY,
  type Simulation,
  type SimulationNodeDatum,
  type SimulationLinkDatum,
} from 'd3-force';
import { zoom, zoomIdentity, type ZoomBehavior, type ZoomTransform } from 'd3-zoom';
import { select } from 'd3-selection';
import { drag } from 'd3-drag';
import 'd3-transition';
import { daemonCall } from '../../utils/daemonClient';
import { onDaemonEvent } from '../../utils/daemonEvents';
import { useWorkspaceLayoutStore } from '../../stores/workspaceLayoutStore';
import { useProjectExplorerStore } from '../../stores/projectExplorerStore';
import { AppInput, AppTooltip, AppEmptyState } from '../ui';
import type { ProjectRecord } from '../../types';
import type { KnowledgeGraphNode } from '../../types/generated/KnowledgeGraphNode';
import type { KnowledgeGraphEdge } from '../../types/generated/KnowledgeGraphEdge';
import type { KnowledgeScope } from '../../types/generated/KnowledgeScope';

const props = defineProps<{
  project: ProjectRecord;
}>();

const layoutStore = useWorkspaceLayoutStore();
const explorerStore = useProjectExplorerStore();

const isLoading = ref(false);
const isRebuildingEdges = ref(false);
const isIndexing = ref(false);
const isSearching = ref(false);
const errorMsg = ref<string | null>(null);

const rawNodes = ref<KnowledgeGraphNode[]>([]);
const rawEdges = ref<KnowledgeGraphEdge[]>([]);
const configMinScorePct = ref(70);

// Interactive controls
const searchQuery = ref('');
const selectedScope = ref<'all' | 'specs' | 'journal' | 'tasks' | 'tech_debt' | 'file'>('all');
const selectedEdgeType = ref<'all' | 'touched' | 'documents' | 'governs' | 'affects' | 'relates_to'>('all');
const minWeightPct = ref(70);
const layoutMode = ref<'force' | 'stratified'>('stratified');
const egoDepth = ref<0 | 1 | 2>(0);
const showIsolated = ref(false);
const hideUnmatched = ref(false);

// Semantic search scores: raw chunk id -> score (0..1)
const semanticScores = ref<Record<string, number>>({});
const selectedSimId = ref<string | null>(null);
const hoveredSimId = ref<string | null>(null);

// Expanded group keys (e.g. "grp::code::src/foo.rs" or "grp::specs::docs/spec.md")
const expandedGroups = ref<Set<string>>(new Set());

// Resizable right inspector width
const inspectorWidthPx = ref<number>(
  Number(localStorage.getItem('oneship:graph:inspector_width_px')) || 340
);
const isResizingInspector = ref(false);

// D3 SVG & Simulation state
const svgRef = ref<SVGSVGElement | null>(null);
const zoomLayerRef = ref<SVGGElement | null>(null);
const currentTransform = ref<ZoomTransform>(zoomIdentity);

interface GraphSimNode extends SimulationNodeDatum {
  id: string;
  kind: 'group' | 'leaf';
  entityType: string;
  label: string;
  sublabel: string;
  groupKey: string;
  filePath: string;
  lineStart: number;
  childNodes: KnowledgeGraphNode[];
  rawNode: KnowledgeGraphNode | null;
  width: number;
  height: number;
  degree: number;
  searchScore: number;
  isMatched: boolean;
  isNeighbor: boolean;
  pinned: boolean;
}

interface GraphSimEdge extends SimulationLinkDatum<GraphSimNode> {
  id: string;
  source: string | GraphSimNode;
  target: string | GraphSimNode;
  edgeType: string;
  origin: string;
  weight: number;
  count: number;
  isContainment: boolean;
  curveIndex: number; // 0 = straight/gentle, +1 / -1 = bidirectional separation
}

const simNodes = ref<GraphSimNode[]>([]);
const simEdges = ref<GraphSimEdge[]>([]);
const simTick = ref(0);

let simulation: Simulation<GraphSimNode, GraphSimEdge> | null = null;
let zoomBehavior: ZoomBehavior<SVGSVGElement, unknown> | null = null;

// --- Data Loading & Daemon Sync ---

async function loadGraph() {
  if (!props.project?.id) return;
  isLoading.value = true;
  errorMsg.value = null;
  try {
    const graph = await daemonCall('get_project_knowledge_graph', {
      projectId: props.project.id,
    });
    rawNodes.value = graph.nodes;
    rawEdges.value = graph.edges;
    configMinScorePct.value = graph.minScorePercent || 70;
    if (minWeightPct.value > configMinScorePct.value) {
      minWeightPct.value = configMinScorePct.value;
    }
    await nextTick();
    rebuildSimulationGraph(true);
  } catch (err: unknown) {
    errorMsg.value = err instanceof Error ? err.message : String(err);
  } finally {
    isLoading.value = false;
  }
}

async function handleRebuildEdges() {
  if (!props.project?.id || isRebuildingEdges.value) return;
  isRebuildingEdges.value = true;
  errorMsg.value = null;
  try {
    await daemonCall('rebuild_project_knowledge_graph', {
      projectId: props.project.id,
    });
    await loadGraph();
  } catch (err: unknown) {
    errorMsg.value = err instanceof Error ? err.message : String(err);
  } finally {
    isRebuildingEdges.value = false;
  }
}

async function handleFullIndex() {
  if (!props.project?.id || isIndexing.value) return;
  isIndexing.value = true;
  errorMsg.value = null;
  try {
    await daemonCall('index_project_knowledge', {
      projectId: props.project.id,
      force: true,
    });
    await loadGraph();
  } catch (err: unknown) {
    errorMsg.value = err instanceof Error ? err.message : String(err);
  } finally {
    isIndexing.value = false;
  }
}

// --- Semantic Search & Auto-Expansion (Explicit Submit Only) ---

async function executeSemanticSearch() {
  const q = searchQuery.value.trim();
  if (!q) {
    semanticScores.value = {};
    isSearching.value = false;
    rebuildSimulationGraph(false);
    return;
  }

  isSearching.value = true;
  try {
    const scopeArg: KnowledgeScope = selectedScope.value === 'file' ? 'all' : selectedScope.value;
    const hits = await daemonCall('search_knowledge', {
      query: q,
      scope: scopeArg,
      limit: 25,
      projectId: props.project.id,
      includeRelations: false,
    });
    const scoreMap: Record<string, number> = {};
    const nextExpanded = new Set(expandedGroups.value);

    for (const h of hits) {
      scoreMap[h.id] = h.score;
      const raw = rawNodesById.value.get(h.id);
      if (raw && raw.entityType === 'specs') {
        const gk = getGroupKeyForNode(raw);
        nextExpanded.add(gk);
      }
    }
    semanticScores.value = scoreMap;
    expandedGroups.value = nextExpanded;

    if (hits.length > 0) {
      selectedSimId.value = hits[0].id;
    }
    rebuildSimulationGraph(true);
  } catch (err: unknown) {
    console.error('[KnowledgeGraph] Semantic filter error:', err);
  } finally {
    isSearching.value = false;
  }
}

function clearSemanticSearch() {
  searchQuery.value = '';
  selectedSimId.value = null;
  egoDepth.value = 0;
  semanticScores.value = {};
  rebuildSimulationGraph(false);
}

watch(selectedScope, () => {
  if (searchQuery.value.trim() && Object.keys(semanticScores.value).length > 0) {
    executeSemanticSearch();
  } else {
    rebuildSimulationGraph(false);
  }
});

watch([minWeightPct, selectedEdgeType, showIsolated, hideUnmatched, egoDepth], () => {
  rebuildSimulationGraph(false);
});

watch(layoutMode, () => {
  applyLayoutForces();
});

watch(
  () => props.project?.id,
  () => {
    selectedSimId.value = null;
    hoveredSimId.value = null;
    searchQuery.value = '';
    semanticScores.value = {};
    expandedGroups.value = new Set();
    egoDepth.value = 0;
    loadGraph();
  },
  { immediate: true }
);

// --- Grouping & Compound Graph Construction ---

const rawNodesById = computed<Map<string, KnowledgeGraphNode>>(() => {
  const m = new Map<string, KnowledgeGraphNode>();
  for (const n of rawNodes.value) {
    m.set(n.id, n);
  }
  return m;
});

const activeRawEdges = computed<KnowledgeGraphEdge[]>(() => {
  const minRatio = minWeightPct.value / 100;
  return rawEdges.value.filter((e) => {
    if (e.weight + 0.0001 < minRatio) return false;
    if (selectedEdgeType.value !== 'all' && e.edgeType !== selectedEdgeType.value) return false;
    return rawNodesById.value.has(e.sourceId) && rawNodesById.value.has(e.targetId);
  });
});

function getGroupKeyForNode(node: KnowledgeGraphNode): string {
  if (node.entityType === 'specs') {
    return `grp::${node.entityType}::${node.filePath}`;
  }
  return node.id;
}

function shortFileName(path: string): string {
  const parts = path.split('/');
  if (parts.length <= 2) return path;
  return `${parts[parts.length - 2]}/${parts[parts.length - 1]}`;
}

function shortSymbolTitle(node: KnowledgeGraphNode): string {
  if (node.entityType === 'file') {
    return shortFileName(node.filePath);
  }
  const idx = node.title.indexOf(' (§');
  if (idx !== -1) {
    return node.title.slice(idx + 2, -1);
  }
  return node.title;
}

function toggleGroupExpansion(groupKey: string) {
  const next = new Set(expandedGroups.value);
  if (next.has(groupKey)) {
    next.delete(groupKey);
    if (selectedSimId.value && !next.has(groupKey)) {
      const selRaw = rawNodesById.value.get(selectedSimId.value);
      if (selRaw && getGroupKeyForNode(selRaw) === groupKey) {
        selectedSimId.value = groupKey;
      }
    }
  } else {
    next.add(groupKey);
  }
  expandedGroups.value = next;
  rebuildSimulationGraph(false);
}

function collapseAllGroups() {
  expandedGroups.value = new Set();
  rebuildSimulationGraph(true);
}

function rebuildSimulationGraph(shouldFitAfterSettling: boolean) {
  const prevPositions = new Map<string, { x: number; y: number; vx: number; vy: number; fx: number | null; fy: number | null; pinned: boolean }>();
  for (const n of simNodes.value) {
    if (n.x !== undefined && n.y !== undefined) {
      prevPositions.set(n.id, {
        x: n.x,
        y: n.y,
        vx: n.vx ?? 0,
        vy: n.vy ?? 0,
        fx: n.fx ?? null,
        fy: n.fy ?? null,
        pinned: n.pinned,
      });
    }
  }

  // Group raw nodes by file for code & specs
  const childrenByGroup = new Map<string, KnowledgeGraphNode[]>();
  for (const n of rawNodes.value) {
    const gk = getGroupKeyForNode(n);
    let arr = childrenByGroup.get(gk);
    if (!arr) {
      arr = [];
      childrenByGroup.set(gk, arr);
    }
    arr.push(n);
  }

  // Map each raw node id to its visible representative sim node id
  const rawToVisibleId = new Map<string, string>();
  const candidateNodes = new Map<string, GraphSimNode>();
  const scores = semanticScores.value;
  const hasSearch = Object.keys(scores).length > 0;

  for (const [gk, children] of childrenByGroup.entries()) {
    const first = children[0];
    const isCollapsible = first.entityType === 'specs' && children.length > 1;
    const isExpanded = expandedGroups.value.has(gk);

    if (!isCollapsible) {
      // Single item (journal, task, tech_debt, file, or single-section spec)
      const n = first;
      rawToVisibleId.set(n.id, n.id);
      const score = scores[n.id] ?? 0;
      const label =
        n.entityType === 'file' || n.entityType === 'specs'
          ? shortFileName(n.filePath)
          : n.title;
      candidateNodes.set(n.id, {
        id: n.id,
        kind: 'leaf',
        entityType: n.entityType,
        label: label.length > 26 ? `${label.slice(0, 24)}..` : label,
        sublabel: n.entityType === 'file' ? 'FILE' : n.entityType.toUpperCase(),
        groupKey: gk,
        filePath: n.filePath,
        lineStart: n.lineStart,
        childNodes: children,
        rawNode: n,
        width: 174,
        height: 40,
        degree: 0,
        searchScore: score,
        isMatched: score > 0,
        isNeighbor: false,
        pinned: false,
      });
    } else if (!isExpanded) {
      // Collapsed Super-Node
      let maxScore = 0;
      for (const c of children) {
        rawToVisibleId.set(c.id, gk);
        if ((scores[c.id] ?? 0) > maxScore) maxScore = scores[c.id];
      }
      const fname = shortFileName(first.filePath);
      candidateNodes.set(gk, {
        id: gk,
        kind: 'group',
        entityType: first.entityType,
        label: fname.length > 24 ? `${fname.slice(0, 22)}..` : fname,
        sublabel: `${children.length} sections`,
        groupKey: gk,
        filePath: first.filePath,
        lineStart: 1,
        childNodes: children,
        rawNode: null,
        width: 184,
        height: 44,
        degree: 0,
        searchScore: maxScore,
        isMatched: maxScore > 0,
        isNeighbor: false,
        pinned: false,
      });
    } else {
      // Expanded Super-Node + its individual children
      let maxScore = 0;
      for (const c of children) {
        rawToVisibleId.set(c.id, c.id);
        if ((scores[c.id] ?? 0) > maxScore) maxScore = scores[c.id];
      }
      const fname = shortFileName(first.filePath);
      candidateNodes.set(gk, {
        id: gk,
        kind: 'group',
        entityType: first.entityType,
        label: fname.length > 24 ? `${fname.slice(0, 22)}..` : fname,
        sublabel: `${children.length} expanded`,
        groupKey: gk,
        filePath: first.filePath,
        lineStart: 1,
        childNodes: children,
        rawNode: null,
        width: 184,
        height: 42,
        degree: 0,
        searchScore: maxScore,
        isMatched: maxScore > 0,
        isNeighbor: false,
        pinned: false,
      });

      for (const c of children) {
        const sLabel = shortSymbolTitle(c);
        const score = scores[c.id] ?? 0;
        candidateNodes.set(c.id, {
          id: c.id,
          kind: 'leaf',
          entityType: c.entityType,
          label: sLabel.length > 24 ? `${sLabel.slice(0, 22)}..` : sLabel,
          sublabel: `L${c.lineStart}–${c.lineEnd}`,
          groupKey: gk,
          filePath: c.filePath,
          lineStart: c.lineStart,
          childNodes: [c],
          rawNode: c,
          width: 160,
          height: 32,
          degree: 0,
          searchScore: score,
          isMatched: score > 0,
          isNeighbor: false,
          pinned: false,
        });
      }
    }
  }

  // Aggregate visible edges
  const edgeAggMap = new Map<string, GraphSimEdge>();
  for (const e of activeRawEdges.value) {
    const srcVis = rawToVisibleId.get(e.sourceId);
    const tgtVis = rawToVisibleId.get(e.targetId);
    if (!srcVis || !tgtVis || srcVis === tgtVis) continue;

    const aggKey = `${srcVis}=>${tgtVis}::${e.edgeType}`;
    const existing = edgeAggMap.get(aggKey);
    if (existing) {
      existing.count += 1;
      if (e.weight > existing.weight) existing.weight = e.weight;
    } else {
      edgeAggMap.set(aggKey, {
        id: aggKey,
        source: srcVis,
        target: tgtVis,
        edgeType: e.edgeType,
        origin: e.origin,
        weight: e.weight,
        count: 1,
        isContainment: false,
        curveIndex: 0,
      });
    }
  }

  // Add subtle structural containment links for expanded groups
  for (const gk of expandedGroups.value) {
    const groupNode = candidateNodes.get(gk);
    if (!groupNode) continue;
    for (const child of groupNode.childNodes) {
      if (!candidateNodes.has(child.id)) continue;
      edgeAggMap.set(`contains::${gk}=>${child.id}`, {
        id: `contains::${gk}=>${child.id}`,
        source: gk,
        target: child.id,
        edgeType: 'contains',
        origin: 'ast',
        weight: 1,
        count: 1,
        isContainment: true,
        curveIndex: 0,
      });
    }
  }

  const allEdges = Array.from(edgeAggMap.values());

  // Compute degree
  for (const e of allEdges) {
    if (e.isContainment) continue;
    const s = candidateNodes.get(e.source as string);
    const t = candidateNodes.get(e.target as string);
    if (s) s.degree += 1;
    if (t) t.degree += 1;
  }

  // Mark bidirectional curve offsets so A->B and B->A never overlap
  const pairSeen = new Set<string>();
  for (const e of allEdges) {
    if (e.isContainment) continue;
    const sId = e.source as string;
    const tId = e.target as string;
    const revExists = allEdges.some(
      (other) => !other.isContainment && other.source === tId && other.target === sId
    );
    if (revExists) {
      const sortedKey = sId < tId ? `${sId}|${tId}` : `${tId}|${sId}`;
      if (!pairSeen.has(sortedKey)) {
        pairSeen.add(sortedKey);
        e.curveIndex = 1;
      } else {
        e.curveIndex = 1; // Both curve positively relative to their own direction = opposite sides!
      }
    }
  }

  // Scope & Visibility filtering
  const visibleIds = new Set<string>();
  for (const n of candidateNodes.values()) {
    const scopeOk = selectedScope.value === 'all' || n.entityType === selectedScope.value;
    const hasEdges = n.degree > 0 || expandedGroups.value.has(n.groupKey);
    const keepForSearch = hasSearch && n.isMatched;
    const keepForSelected = selectedSimId.value === n.id;

    if (scopeOk && (showIsolated.value || hasEdges || keepForSearch || keepForSelected)) {
      visibleIds.add(n.id);
    }
  }

  // Ensure endpoints of visible edges with at least 1 endpoint in scope stay visible
  for (const e of allEdges) {
    const sId = e.source as string;
    const tId = e.target as string;
    if (visibleIds.has(sId) && candidateNodes.has(tId)) {
      if (selectedScope.value === 'all' || !e.isContainment) {
        visibleIds.add(tId);
      }
    }
    if (visibleIds.has(tId) && candidateNodes.has(sId)) {
      if (selectedScope.value === 'all' || !e.isContainment) {
        visibleIds.add(sId);
      }
    }
  }

  // Ego-Graph isolation (1-Hop or 2-Hop around selectedSimId)
  if (egoDepth.value > 0 && selectedSimId.value && visibleIds.has(selectedSimId.value)) {
    const egoSet = new Set<string>([selectedSimId.value]);
    for (let hop = 0; hop < egoDepth.value; hop++) {
      const frontier = Array.from(egoSet);
      for (const e of allEdges) {
        const sId = e.source as string;
        const tId = e.target as string;
        if (frontier.includes(sId)) egoSet.add(tId);
        if (frontier.includes(tId)) egoSet.add(sId);
      }
    }
    for (const id of Array.from(visibleIds)) {
      if (!egoSet.has(id)) visibleIds.delete(id);
    }
  }

  // Neighbor highlighting for search or selection
  const focusIds = new Set<string>();
  if (selectedSimId.value && visibleIds.has(selectedSimId.value)) {
    focusIds.add(selectedSimId.value);
  }
  if (hasSearch) {
    for (const id of visibleIds) {
      if (candidateNodes.get(id)?.isMatched) focusIds.add(id);
    }
  }
  if (focusIds.size > 0) {
    for (const e of allEdges) {
      const sId = e.source as string;
      const tId = e.target as string;
      if (focusIds.has(sId)) {
        const tn = candidateNodes.get(tId);
        if (tn && !tn.isMatched) tn.isNeighbor = true;
      }
      if (focusIds.has(tId)) {
        const sn = candidateNodes.get(sId);
        if (sn && !sn.isMatched) sn.isNeighbor = true;
      }
    }
  }

  if (hideUnmatched.value && focusIds.size > 0) {
    for (const id of Array.from(visibleIds)) {
      const cn = candidateNodes.get(id);
      if (!cn) continue;
      if (!focusIds.has(id) && !cn.isNeighbor) {
        visibleIds.delete(id);
      }
    }
  }

  // Build final simulation arrays, preserving coordinates for continuity
  const svgW = svgRef.value?.clientWidth || 1000;
  const svgH = svgRef.value?.clientHeight || 640;

  const finalNodes: GraphSimNode[] = [];
  for (const id of visibleIds) {
    const n = candidateNodes.get(id)!;
    const prev = prevPositions.get(id);
    if (prev) {
      n.x = prev.x;
      n.y = prev.y;
      n.vx = prev.vx;
      n.vy = prev.vy;
      n.fx = prev.fx;
      n.fy = prev.fy;
      n.pinned = prev.pinned;
    } else {
      // Smart initial placement: try parent group position if child, else stratified X
      const parentPrev = n.kind === 'leaf' ? prevPositions.get(n.groupKey) : undefined;
      if (parentPrev) {
        n.x = parentPrev.x + (Math.random() - 0.5) * 90;
        n.y = parentPrev.y + (Math.random() - 0.5) * 90;
      } else {
        const domainX =
          n.entityType === 'file'
            ? svgW * 0.78
            : n.entityType === 'specs'
              ? svgW * 0.16
              : svgW * 0.46;
        n.x = domainX + (Math.random() - 0.5) * 180;
        n.y = svgH * 0.5 + (Math.random() - 0.5) * 260;
      }
    }
    finalNodes.push(n);
  }

  const finalEdges = allEdges.filter(
    (e) => visibleIds.has(e.source as string) && visibleIds.has(e.target as string)
  );

  simNodes.value = finalNodes;
  simEdges.value = finalEdges;

  startOrUpdateSimulation(shouldFitAfterSettling);
}

// --- D3 Force Simulation & Zoom/Drag ---

function applyLayoutForces(restartAlpha = 0.45) {
  if (!simulation) return;
  const svgW = svgRef.value?.clientWidth || 1000;
  const svgH = svgRef.value?.clientHeight || 640;

  if (layoutMode.value === 'stratified') {
    simulation
      .force(
        'x',
        forceX<GraphSimNode>((d) => {
          if (d.entityType === 'specs') return svgW * 0.16;
          if (d.entityType === 'file') return svgW * 0.78;
          return svgW * 0.46;
        }).strength(0.28)
      )
      .force('y', forceY<GraphSimNode>(svgH * 0.5).strength(0.08))
      .force('center', null);
  } else {
    simulation
      .force('x', forceX<GraphSimNode>(svgW * 0.5).strength(0.06))
      .force('y', forceY<GraphSimNode>(svgH * 0.5).strength(0.06))
      .force('center', forceCenter(svgW * 0.5, svgH * 0.5));
  }

  if (restartAlpha > 0) {
    simulation.alpha(restartAlpha).restart();
  }
}

function startOrUpdateSimulation(shouldFitAfterSettling: boolean) {
  if (simulation) {
    simulation.stop();
  }

  simulation = forceSimulation<GraphSimNode>(simNodes.value)
    .force(
      'link',
      forceLink<GraphSimNode, GraphSimEdge>(simEdges.value)
        .id((d) => d.id)
        .distance((l) => {
          if (l.isContainment) return 96;
          return Math.round(210 - Math.min(1, l.weight) * 75);
        })
        .strength((l) => (l.isContainment ? 0.7 : 0.42))
    )
    .force('charge', forceManyBody<GraphSimNode>().strength(-280).distanceMax(420))
    .force(
      'collide',
      forceCollide<GraphSimNode>((d) => (d.kind === 'group' ? 76 : 68))
        .strength(0.7)
        .iterations(2)
    )
    .velocityDecay(0.55)
    .alphaDecay(0.065)
    .alphaMin(0.015)
    .on('tick', () => {
      simTick.value += 1;
    })
    .on('end', () => {
      simulation?.stop();
    });

  // Apply layout forces without starting an un-settled loop first, pre-warm synchronously
  applyLayoutForces(0);
  simulation.stop();
  simulation.alpha(0.8);
  for (let i = 0; i < 120; i++) {
    simulation.tick();
  }
  simTick.value += 1;

  // Allow only a brief gentle settle if a small structural delta occurred, or stay still
  simulation.alpha(0.08).restart();

  nextTick(() => {
    bindNodeDragHandlers();
    if (shouldFitAfterSettling && simNodes.value.length > 0) {
      fitGraphToViewport();
    }
  });
}

function bindNodeDragHandlers() {
  if (!zoomLayerRef.value) return;
  const nodeGroups = select(zoomLayerRef.value).selectAll<SVGGElement, GraphSimNode>('g.sim-node');

  nodeGroups.data(simNodes.value, (d) => d.id);

  const dragBehavior = drag<SVGGElement, GraphSimNode>()
    .on('start', (event, d) => {
      if (!event.active && simulation) simulation.alphaTarget(0.25).restart();
      d.fx = d.x;
      d.fy = d.y;
    })
    .on('drag', (event, d) => {
      d.fx = event.x;
      d.fy = event.y;
      d.pinned = true;
      simTick.value += 1;
    })
    .on('end', (event) => {
      if (!event.active && simulation) simulation.alphaTarget(0);
    });

  nodeGroups.call(dragBehavior);
}

function unpinNode(node: GraphSimNode) {
  node.fx = null;
  node.fy = null;
  node.pinned = false;
  simulation?.alpha(0.3).restart();
}

function initZoom() {
  if (!svgRef.value) return;
  zoomBehavior = zoom<SVGSVGElement, unknown>()
    .scaleExtent([0.18, 2.8])
    .on('zoom', (event) => {
      currentTransform.value = event.transform;
    });

  select(svgRef.value)
    .call(zoomBehavior)
    .on('dblclick.zoom', null);
}

function zoomBy(factor: number) {
  if (!svgRef.value || !zoomBehavior) return;
  select(svgRef.value).transition().duration(220).call(zoomBehavior.scaleBy, factor);
}

function fitGraphToViewport() {
  if (!svgRef.value || !zoomBehavior || simNodes.value.length === 0) return;
  const svgW = svgRef.value.clientWidth;
  const svgH = svgRef.value.clientHeight;
  if (svgW <= 0 || svgH <= 0) return;

  let minX = Infinity;
  let maxX = -Infinity;
  let minY = Infinity;
  let maxY = -Infinity;

  for (const n of simNodes.value) {
    const x = n.x ?? 0;
    const y = n.y ?? 0;
    minX = Math.min(minX, x - n.width / 2);
    maxX = Math.max(maxX, x + n.width / 2);
    minY = Math.min(minY, y - n.height / 2);
    maxY = Math.max(maxY, y + n.height / 2);
  }

  const boundsW = Math.max(120, maxX - minX);
  const boundsH = Math.max(120, maxY - minY);
  const padding = 72;
  const scale = Math.min(
    1.15,
    Math.max(0.22, Math.min((svgW - padding * 2) / boundsW, (svgH - padding * 2) / boundsH))
  );
  const tx = svgW / 2 - ((minX + maxX) / 2) * scale;
  const ty = svgH / 2 - ((minY + maxY) / 2) * scale;

  select(svgRef.value)
    .transition()
    .duration(360)
    .call(zoomBehavior.transform, zoomIdentity.translate(tx, ty).scale(scale));
}

// --- Geometry: Edge Paths & Box Perimeter Clipping ---

function clipRayToBox(
  cx: number,
  cy: number,
  targetX: number,
  targetY: number,
  halfW: number,
  halfH: number
): { x: number; y: number } {
  const dx = targetX - cx;
  const dy = targetY - cy;
  if (dx === 0 && dy === 0) return { x: cx, y: cy };
  const scaleX = Math.abs(dx) > 0 ? halfW / Math.abs(dx) : Infinity;
  const scaleY = Math.abs(dy) > 0 ? halfH / Math.abs(dy) : Infinity;
  const t = Math.min(scaleX, scaleY);
  return {
    x: cx + dx * t,
    y: cy + dy * t,
  };
}

interface RenderedEdgeGeometry {
  edge: GraphSimEdge;
  d: string;
  labelX: number;
  labelY: number;
  isHighlighted: boolean;
  isDimmed: boolean;
  strokeWidth: number;
  opacity: number;
}

const renderedEdges = computed<RenderedEdgeGeometry[]>(() => {
  // Touch simTick so Vue updates SVG paths each physics tick
  void simTick.value;

  const activeFocusId = hoveredSimId.value || selectedSimId.value;
  const hasSearch = Object.keys(semanticScores.value).length > 0;
  const hasAnyFocus = activeFocusId !== null || hasSearch;

  const list: RenderedEdgeGeometry[] = [];
  for (const edge of simEdges.value) {
    const src = edge.source as GraphSimNode;
    const tgt = edge.target as GraphSimNode;
    if (
      src.x === undefined ||
      src.y === undefined ||
      tgt.x === undefined ||
      tgt.y === undefined
    ) {
      continue;
    }

    const dx = tgt.x - src.x;
    const dy = tgt.y - src.y;
    const dist = Math.hypot(dx, dy) || 1;
    const nx = -dy / dist;
    const ny = dx / dist;

    // Curve offset: bidirectional edges separate cleanly; normal edges get a subtle organic arc
    const curveOffset = edge.isContainment
      ? 0
      : edge.curveIndex !== 0
        ? Math.min(54, Math.max(24, dist * 0.16))
        : Math.min(26, dist * 0.07);

    const ctrlX = (src.x + tgt.x) / 2 + nx * curveOffset;
    const ctrlY = (src.y + tgt.y) / 2 + ny * curveOffset;

    const p1 = clipRayToBox(src.x, src.y, ctrlX, ctrlY, src.width / 2 + 2, src.height / 2 + 2);
    const p2 = clipRayToBox(tgt.x, tgt.y, ctrlX, ctrlY, tgt.width / 2 + 6, tgt.height / 2 + 6);

    const d = `M ${p1.x.toFixed(1)} ${p1.y.toFixed(1)} Q ${ctrlX.toFixed(1)} ${ctrlY.toFixed(1)} ${p2.x.toFixed(1)} ${p2.y.toFixed(1)}`;
    // Midpoint on quadratic Bezier t = 0.5
    const labelX = 0.25 * p1.x + 0.5 * ctrlX + 0.25 * p2.x;
    const labelY = 0.25 * p1.y + 0.5 * ctrlY + 0.25 * p2.y;

    const touchesSelected =
      activeFocusId !== null && (src.id === activeFocusId || tgt.id === activeFocusId);
    const touchesSearch = hasSearch && (src.isMatched || tgt.isMatched);
    const isHighlighted = !edge.isContainment && (touchesSelected || touchesSearch);
    const isDimmed = hasAnyFocus && !isHighlighted;

    const normWeight = Math.max(0, Math.min(1, (edge.weight - 0.65) / 0.35));
    const strokeWidth = edge.isContainment
      ? 1
      : isHighlighted
        ? 1.6 + normWeight * 1.8
        : 1.0 + normWeight * 1.0;

    const opacity = edge.isContainment
      ? 0.28
      : !hasAnyFocus
        ? 0.45 + normWeight * 0.35
        : isHighlighted
          ? 0.92
          : 0.10;

    list.push({
      edge,
      d,
      labelX,
      labelY,
      isHighlighted,
      isDimmed,
      strokeWidth,
      opacity,
    });
  }
  return list;
});

function isNodeDimmed(node: GraphSimNode): boolean {
  const activeFocusId = hoveredSimId.value || selectedSimId.value;
  const hasSearch = Object.keys(semanticScores.value).length > 0;
  if (!activeFocusId && !hasSearch) return false;
  if (node.id === activeFocusId || node.isMatched || node.isNeighbor) return false;
  if (activeFocusId) {
    const connected = simEdges.value.some((e) => {
      const sId = (e.source as GraphSimNode).id ?? (e.source as string);
      const tId = (e.target as GraphSimNode).id ?? (e.target as string);
      return (sId === activeFocusId && tId === node.id) || (tId === activeFocusId && sId === node.id);
    });
    if (connected) return false;
  }
  return true;
}

// --- Selection & Inspector ---

const selectedSimNode = computed<GraphSimNode | null>(() => {
  if (!selectedSimId.value) return null;
  return simNodes.value.find((n) => n.id === selectedSimId.value) || null;
});

interface InspectorConnection {
  direction: 'incoming' | 'outgoing';
  edgeType: string;
  origin: string;
  weight: number;
  count: number;
  peerId: string;
  peerLabel: string;
  peerEntityType: string;
  peerFilePath: string;
  peerLineStart: number;
}

const selectedConnections = computed<InspectorConnection[]>(() => {
  const sel = selectedSimNode.value;
  if (!sel) return [];

  const out: InspectorConnection[] = [];
  for (const e of simEdges.value) {
    if (e.isContainment) continue;
    const src = e.source as GraphSimNode;
    const tgt = e.target as GraphSimNode;
    if (src.id === sel.id) {
      out.push({
        direction: 'outgoing',
        edgeType: e.edgeType,
        origin: e.origin,
        weight: e.weight,
        count: e.count,
        peerId: tgt.id,
        peerLabel: tgt.label,
        peerEntityType: tgt.entityType,
        peerFilePath: tgt.filePath,
        peerLineStart: tgt.lineStart,
      });
    } else if (tgt.id === sel.id) {
      out.push({
        direction: 'incoming',
        edgeType: e.edgeType,
        origin: e.origin,
        weight: e.weight,
        count: e.count,
        peerId: src.id,
        peerLabel: src.label,
        peerEntityType: src.entityType,
        peerFilePath: src.filePath,
        peerLineStart: src.lineStart,
      });
    }
  }
  return out.sort((a, b) => b.weight - a.weight);
});

const incomingConnections = computed(() =>
  selectedConnections.value.filter((c) => c.direction === 'incoming')
);
const outgoingConnections = computed(() =>
  selectedConnections.value.filter((c) => c.direction === 'outgoing')
);

function handleNodeClick(node: GraphSimNode) {
  selectedSimId.value = selectedSimId.value === node.id ? null : node.id;
}

function handleNodeDblClick(node: GraphSimNode) {
  if (node.kind === 'group') {
    toggleGroupExpansion(node.groupKey);
  } else if (node.pinned) {
    unpinNode(node);
  }
}

async function openTargetInExplorer(filePath: string) {
  if (!filePath || !props.project?.path) return;
  if (
    filePath.startsWith('journal://') ||
    filePath === 'TODO.md' ||
    filePath === 'TECHNICAL_DEBT.md'
  ) {
    layoutStore.setWorkspaceTab('devlog');
    return;
  }
  layoutStore.setWorkspaceTab('files');
  await explorerStore.selectProjectDocument(props.project.path, filePath, props.project.id);
}

// --- Visual Helpers ---

function getEdgeColor(edgeType: string): string {
  switch (edgeType) {
    case 'touched':
      return 'var(--accent-color)';
    case 'documents':
    case 'governs':
    case 'references':
      return 'var(--status-ok-color)';
    case 'affects':
    case 'relates_to':
      return 'var(--status-warn-color)';
    default:
      return 'var(--dim-color)';
  }
}

function getMarkerEnd(edgeType: string): string {
  switch (edgeType) {
    case 'touched':
      return 'url(#arrow-touched)';
    case 'documents':
    case 'governs':
    case 'references':
      return 'url(#arrow-governs)';
    case 'affects':
    case 'relates_to':
      return 'url(#arrow-affects)';
    default:
      return '';
  }
}

function getEntityColor(entityType: string): string {
  switch (entityType) {
    case 'file':
      return 'var(--accent-color)';
    case 'specs':
      return 'var(--status-ok-color)';
    case 'tasks':
    case 'tech_debt':
      return 'var(--status-warn-color)';
    default:
      return 'var(--dim-color)';
  }
}

function getEntityIcon(entityType: string) {
  switch (entityType) {
    case 'file':
      return FileCode;
    case 'specs':
      return BookOpen;
    case 'journal':
      return History;
    case 'tasks':
      return CheckSquare;
    case 'tech_debt':
      return AlertOctagon;
    default:
      return GitFork;
  }
}

function formatPct(weightOrScore: number): string {
  return `${Math.round(weightOrScore * 100)}%`;
}

function startInspectorResize(e: MouseEvent) {
  e.preventDefault();
  isResizingInspector.value = true;
  const startX = e.clientX;
  const startWidth = inspectorWidthPx.value;

  const onMove = (moveEvent: MouseEvent) => {
    const delta = startX - moveEvent.clientX;
    inspectorWidthPx.value = Math.min(540, Math.max(260, startWidth + delta));
  };

  const onUp = () => {
    isResizingInspector.value = false;
    localStorage.setItem('oneship:graph:inspector_width_px', String(inspectorWidthPx.value));
    window.removeEventListener('mousemove', onMove);
    window.removeEventListener('mouseup', onUp);
  };

  window.addEventListener('mousemove', onMove);
  window.addEventListener('mouseup', onUp);
}

let unlistenProjectsUpdated: (() => void) | null = null;

watch(svgRef, (el) => {
  if (el) {
    initZoom();
  }
});

onMounted(() => {
  if (svgRef.value) {
    initZoom();
  }
  unlistenProjectsUpdated = onDaemonEvent('projects-updated', () => {
    if (!isLoading.value && !isIndexing.value) {
      loadGraph();
    }
  });
});

onUnmounted(() => {
  if (simulation) simulation.stop();
  if (unlistenProjectsUpdated) unlistenProjectsUpdated();
});
</script>

<template>
  <div class="flex flex-col flex-1 min-h-0 h-full bg-[var(--bg-color)] text-[var(--fg-color)] font-mono select-none overflow-hidden">
    <!-- Top Surface Toolbar (Spec 13 §3.13: h-9 ui-surface-title) -->
    <div class="h-9 ui-surface-title px-3 border-b border-[var(--border-color)] flex items-center justify-between gap-2.5 shrink-0 overflow-x-auto">
      <!-- Left: Semantic Spotlight Search (Submit on Enter or Button) -->
      <form
        @submit.prevent="executeSemanticSearch"
        class="flex items-center gap-1 min-w-[220px] max-w-sm flex-1"
      >
        <AppInput
          v-model="searchQuery"
          type="text"
          class="!h-6 !text-[10px] flex-1"
          placeholder="Semantic query (Enter to search)..."
        />
        <AppTooltip content="Run semantic spotlight search">
          <button
            type="submit"
            :disabled="isSearching"
            class="ui-icon-btn !h-6 !w-6 !border !border-[var(--border-color)] hover:!border-[var(--accent-color)]"
          >
            <Search class="w-3 h-3 text-[var(--accent-color)]" :class="{ 'animate-pulse': isSearching }" />
          </button>
        </AppTooltip>
        <AppTooltip v-if="searchQuery || selectedSimId || Object.keys(semanticScores).length > 0" content="Clear semantic search and selection">
          <button
            type="button"
            @click="clearSemanticSearch"
            class="ui-icon-btn !h-6 !w-6"
          >
            <X class="w-3 h-3" />
          </button>
        </AppTooltip>
      </form>

      <!-- Center: Scope, Edge Kind, Layout & Weight Threshold -->
      <div class="flex items-center gap-2 shrink-0">
        <!-- Scope Filter Pills -->
        <div class="flex items-center gap-1">
          <button
            v-for="sc in (['all', 'specs', 'journal', 'tasks', 'tech_debt', 'file'] as const)"
            :key="sc"
            type="button"
            @click="selectedScope = sc"
            class="px-1.5 py-0.5 text-[8px] font-bold uppercase border transition-colors cursor-pointer"
            :class="
              selectedScope === sc
                ? 'border-[var(--accent-color)] text-[var(--accent-color)] bg-[var(--accent-color)]/10'
                : 'border-[var(--border-color)] text-[var(--dim-color)] hover:text-[var(--fg-color)]'
            "
          >
            {{ sc === 'tech_debt' ? 'DEBT' : sc === 'file' ? 'FILES' : sc }}
          </button>
        </div>

        <span class="text-[var(--border-color)]">|</span>

        <!-- Edge Kind Pills -->
        <div class="flex items-center gap-1">
          <AppTooltip content="Show all relation types">
            <button
              type="button"
              @click="selectedEdgeType = 'all'"
              class="px-1.5 py-0.5 text-[8px] font-bold uppercase border transition-colors cursor-pointer"
              :class="selectedEdgeType === 'all' ? 'border-[var(--fg-color)] text-[var(--fg-color)]' : 'border-[var(--border-color)] text-[var(--dim-color)]'"
            >
              All
            </button>
          </AppTooltip>
          <AppTooltip content="Causal Git hunk / files_touched links from Minutes to Files">
            <button
              type="button"
              @click="selectedEdgeType = 'touched'"
              class="px-1.5 py-0.5 text-[8px] font-bold uppercase border transition-colors cursor-pointer"
              :class="selectedEdgeType === 'touched' ? 'border-[var(--accent-color)] text-[var(--accent-color)] bg-[var(--accent-color)]/10' : 'border-[var(--border-color)] text-[var(--dim-color)]'"
            >
              Touched
            </button>
          </AppTooltip>
          <AppTooltip content="Semantic & explicit Minute <-> Spec links">
            <button
              type="button"
              @click="selectedEdgeType = 'documents'"
              class="px-1.5 py-0.5 text-[8px] font-bold uppercase border transition-colors cursor-pointer"
              :class="selectedEdgeType === 'documents' ? 'border-[var(--status-ok-color)] text-[var(--status-ok-color)] bg-[var(--status-ok-color)]/10' : 'border-[var(--border-color)] text-[var(--dim-color)]'"
            >
              Docs
            </button>
          </AppTooltip>
          <AppTooltip content="Tasks & Tech Debt anchors to Files">
            <button
              type="button"
              @click="selectedEdgeType = 'affects'"
              class="px-1.5 py-0.5 text-[8px] font-bold uppercase border transition-colors cursor-pointer"
              :class="selectedEdgeType === 'affects' ? 'border-[var(--status-warn-color)] text-[var(--status-warn-color)] bg-[var(--status-warn-color)]/10' : 'border-[var(--border-color)] text-[var(--dim-color)]'"
            >
              Affects
            </button>
          </AppTooltip>
        </div>

        <span class="text-[var(--border-color)]">|</span>

        <!-- Layout Mode Toggle -->
        <AppTooltip content="Switch D3 physics layout: Stratified (Specs -> Minutes/Debt -> Files) or Organic Force">
          <button
            type="button"
            @click="layoutMode = layoutMode === 'stratified' ? 'force' : 'stratified'"
            class="h-6 px-2 text-[8px] font-bold uppercase border border-[var(--border-color)] text-[var(--fg-color)] hover:border-[var(--accent-color)] flex items-center gap-1 cursor-pointer"
          >
            <Layers v-if="layoutMode === 'stratified'" class="w-3 h-3 text-[var(--accent-color)]" />
            <Share2 v-else class="w-3 h-3 text-[var(--status-ok-color)]" />
            <span>{{ layoutMode === 'stratified' ? 'Flow' : 'Force' }}</span>
          </button>
        </AppTooltip>

        <!-- Live Min Weight Slider -->
        <AppTooltip :content="`Filter edges below ${minWeightPct}% similarity weight`">
          <div class="flex items-center gap-1.5 px-1.5 py-0.5 border border-[var(--border-color)] bg-[var(--bg-color)]">
            <span class="text-[8px] uppercase text-[var(--dim-color)]">Min</span>
            <input
              v-model.number="minWeightPct"
              type="range"
              min="50"
              max="100"
              step="1"
              class="w-14 h-1 accent-[var(--accent-color)] cursor-pointer"
            />
            <span class="text-[9px] font-bold text-[var(--accent-color)] w-7 text-right">{{ minWeightPct }}%</span>
          </div>
        </AppTooltip>

        <AppTooltip :content="hideUnmatched ? 'Showing matched subgraph only' : 'Dimming unmatched nodes'">
          <button
            type="button"
            @click="hideUnmatched = !hideUnmatched"
            class="h-6 px-1.5 text-[8px] font-bold uppercase border flex items-center gap-1 cursor-pointer"
            :class="hideUnmatched ? 'border-[var(--accent-color)] text-[var(--accent-color)] bg-[var(--accent-color)]/10' : 'border-[var(--border-color)] text-[var(--dim-color)]'"
          >
            <EyeOff v-if="hideUnmatched" class="w-3 h-3" />
            <Eye v-else class="w-3 h-3" />
          </button>
        </AppTooltip>
      </div>

      <!-- Right: Graph Actions -->
      <div class="flex items-center gap-1.5 shrink-0">
        <span class="text-[9px] text-[var(--dim-color)] mr-1">
          <strong class="text-[var(--fg-color)]">{{ simNodes.length }}</strong>n ·
          <strong class="text-[var(--accent-color)]">{{ simEdges.filter((e) => !e.isContainment).length }}</strong>e
        </span>

        <AppTooltip content="Recalculate graph edges from stored vectors (< 300 ms)">
          <button
            type="button"
            :disabled="isRebuildingEdges || isIndexing"
            @click="handleRebuildEdges"
            class="h-6 px-2 text-[8px] font-bold uppercase border border-[var(--border-color)] text-[var(--fg-color)] hover:border-[var(--accent-color)] hover:text-[var(--accent-color)] flex items-center gap-1 cursor-pointer disabled:opacity-40"
          >
            <Sparkles class="w-3 h-3" :class="{ 'animate-spin': isRebuildingEdges }" />
            <span>{{ isRebuildingEdges ? 'Rebuilding...' : 'Rebuild' }}</span>
          </button>
        </AppTooltip>

        <AppTooltip content="Run incremental AST & semantic vector indexation">
          <button
            type="button"
            :disabled="isIndexing || isRebuildingEdges"
            @click="handleFullIndex"
            class="h-6 px-2 text-[8px] font-bold uppercase border border-[var(--border-color)] text-[var(--fg-color)] hover:border-[var(--accent-color)] hover:text-[var(--accent-color)] flex items-center gap-1 cursor-pointer disabled:opacity-40"
          >
            <RefreshCw class="w-3 h-3" :class="{ 'animate-spin': isIndexing }" />
            <span>{{ isIndexing ? 'Indexing...' : 'Index' }}</span>
          </button>
        </AppTooltip>
      </div>
    </div>

    <!-- Error Banner -->
    <div
      v-if="errorMsg"
      class="px-3 py-1.5 bg-[var(--status-err-bg)] border-b border-[var(--status-err-border)] text-[var(--status-err-color)] text-[10px] flex items-center justify-between"
    >
      <span>{{ errorMsg }}</span>
      <button type="button" @click="errorMsg = null" class="cursor-pointer"><X class="w-3 h-3" /></button>
    </div>

    <!-- Empty Index State -->
    <div v-if="!isLoading && rawNodes.length === 0" class="flex-1 flex items-center justify-center p-8">
      <AppEmptyState
        :icon="Network"
        title="Knowledge Graph Not Indexed Yet"
        description="Index this project to extract multi-language Tree-sitter AST symbols, specification sections, session journals, and weighted semantic graph edges."
      >
        <button
          type="button"
          :disabled="isIndexing"
          @click="handleFullIndex"
          class="mt-3 h-6 px-3 text-[9px] font-bold uppercase border border-[var(--accent-color)] text-[var(--accent-color)] hover:bg-[var(--accent-color)]/10 flex items-center gap-1.5 cursor-pointer"
        >
          <RefreshCw class="w-3 h-3" :class="{ 'animate-spin': isIndexing }" />
          <span>{{ isIndexing ? 'Indexing Project Knowledge...' : 'Index Project Knowledge' }}</span>
        </button>
      </AppEmptyState>
    </div>

    <!-- Main D3 Force Canvas + Right Inspector -->
    <div v-else class="flex-1 flex min-h-0 overflow-hidden">
      <!-- D3 Spatial SVG Canvas -->
      <div class="relative flex-1 min-w-0 min-h-0 overflow-hidden bg-[var(--bg-color)]">
        <!-- Subtle architectural dot grid background -->
        <div
          class="pointer-events-none absolute inset-0 opacity-20"
          style="background-image: radial-gradient(var(--border-color) 1px, transparent 1px); background-size: 20px 20px;"
        ></div>

        <!-- Stratified Column Hints (when in Flow mode) -->
        <div
          v-if="layoutMode === 'stratified'"
          class="pointer-events-none absolute top-2 left-3 right-3 flex justify-between px-6 text-[8px] font-bold uppercase tracking-widest text-[var(--dim-color)] opacity-60 z-10"
        >
          <span>Specs &amp; Governance</span>
          <span>Code Modules &amp; Symbols</span>
        </div>

        <!-- Floating Camera & Subgraph Controls (Bottom-Left) -->
        <div class="absolute bottom-3 left-3 z-20 flex items-center gap-1 bg-[var(--surface-bg)] border border-[var(--border-color)] p-1 shadow-md">
          <AppTooltip content="Zoom In">
            <button type="button" @click="zoomBy(1.25)" class="ui-icon-btn !h-6 !w-6">
              <ZoomIn class="w-3 h-3" />
            </button>
          </AppTooltip>
          <AppTooltip content="Zoom Out">
            <button type="button" @click="zoomBy(0.8)" class="ui-icon-btn !h-6 !w-6">
              <ZoomOut class="w-3 h-3" />
            </button>
          </AppTooltip>
          <AppTooltip content="Fit graph to viewport">
            <button type="button" @click="fitGraphToViewport()" class="ui-icon-btn !h-6 !w-6">
              <Maximize2 class="w-3 h-3" />
            </button>
          </AppTooltip>
          <span class="text-[var(--border-color)] mx-0.5">|</span>
          <AppTooltip content="Collapse all expanded files/specs back to macro view">
            <button
              type="button"
              @click="collapseAllGroups"
              class="h-6 px-2 text-[8px] font-bold uppercase text-[var(--dim-color)] hover:text-[var(--fg-color)] cursor-pointer"
            >
              Collapse ({{ expandedGroups.size }})
            </button>
          </AppTooltip>
          <AppTooltip content="Toggle isolated nodes with zero edges">
            <button
              type="button"
              @click="showIsolated = !showIsolated"
              class="h-6 px-2 text-[8px] font-bold uppercase border cursor-pointer"
              :class="showIsolated ? 'border-[var(--accent-color)] text-[var(--accent-color)]' : 'border-transparent text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
            >
              Isolated
            </button>
          </AppTooltip>
        </div>

        <!-- Legend Overlay (Bottom-Right of Canvas) -->
        <div class="pointer-events-none absolute bottom-3 right-3 z-20 flex items-center gap-3 bg-[var(--surface-bg)]/90 border border-[var(--border-color)] px-2.5 py-1 text-[8px] uppercase text-[var(--dim-color)]">
          <span class="flex items-center gap-1">
            <span class="w-3 h-0.5 inline-block bg-[var(--status-ok-color)]"></span> Docs / Governs
          </span>
          <span class="flex items-center gap-1">
            <span class="w-3 h-0.5 inline-block bg-[var(--status-warn-color)]"></span> Affects / Relates
          </span>
          <span class="flex items-center gap-1">
            <span class="w-3 h-0.5 inline-block bg-[var(--accent-color)]"></span> Touched
          </span>
          <span class="opacity-70">Dbl-Click: Expand · Drag: Pin</span>
        </div>

        <svg ref="svgRef" class="w-full h-full cursor-grab active:cursor-grabbing">
          <defs>
            <marker
              id="arrow-governs"
              viewBox="0 -5 10 10"
              refX="8"
              refY="0"
              markerWidth="6"
              markerHeight="6"
              orient="auto"
            >
              <path d="M0,-4L9,0L0,4Z" fill="var(--status-ok-color)" />
            </marker>
            <marker
              id="arrow-affects"
              viewBox="0 -5 10 10"
              refX="8"
              refY="0"
              markerWidth="6"
              markerHeight="6"
              orient="auto"
            >
              <path d="M0,-4L9,0L0,4Z" fill="var(--status-warn-color)" />
            </marker>
            <marker
              id="arrow-touched"
              viewBox="0 -5 10 10"
              refX="8"
              refY="0"
              markerWidth="6"
              markerHeight="6"
              orient="auto"
            >
              <path d="M0,-4L9,0L0,4Z" fill="var(--accent-color)" />
            </marker>
          </defs>

          <g ref="zoomLayerRef" :transform="currentTransform.toString()">
            <!-- Directed Curved Edges -->
            <g class="edges-layer">
              <template v-for="re in renderedEdges" :key="re.edge.id">
                <path
                  :d="re.d"
                  fill="none"
                  :stroke="re.edge.isContainment ? 'var(--border-color)' : getEdgeColor(re.edge.edgeType)"
                  :stroke-width="re.strokeWidth"
                  :stroke-opacity="re.opacity"
                  :stroke-dasharray="
                    re.edge.isContainment
                      ? '2,3'
                      : re.isHighlighted
                        ? '6,4'
                        : re.edge.origin === 'semantic'
                          ? '4,3'
                          : undefined
                  "
                  :marker-end="re.edge.isContainment ? undefined : getMarkerEnd(re.edge.edgeType)"
                  :class="{ 'graph-edge-flow': re.isHighlighted && !re.edge.isContainment }"
                />

                <!-- Edge Weight Pill on Highlighted Links -->
                <g
                  v-if="re.isHighlighted && !re.edge.isContainment"
                  :transform="`translate(${re.labelX}, ${re.labelY})`"
                  class="pointer-events-none"
                >
                  <rect
                    x="-28"
                    y="-8"
                    width="56"
                    height="15"
                    rx="2"
                    fill="var(--bg-color)"
                    :stroke="getEdgeColor(re.edge.edgeType)"
                    stroke-width="0.9"
                  />
                  <text
                    text-anchor="middle"
                    dominant-baseline="central"
                    y="-0.5"
                    font-size="7.5"
                    font-weight="bold"
                    :fill="getEdgeColor(re.edge.edgeType)"
                  >
                    {{ re.edge.edgeType }} {{ formatPct(re.edge.weight) }}{{ re.edge.count > 1 ? ` ×${re.edge.count}` : '' }}
                  </text>
                </g>
              </template>
            </g>

            <!-- Compound & Leaf Nodes -->
            <g class="nodes-layer">
              <g
                v-for="node in simNodes"
                :key="node.id"
                class="sim-node cursor-pointer"
                :transform="`translate(${(node.x ?? 0) - node.width / 2}, ${(node.y ?? 0) - node.height / 2})`"
                :style="{ opacity: isNodeDimmed(node) ? 0.22 : 1 }"
                @click.stop="handleNodeClick(node)"
                @dblclick.stop="handleNodeDblClick(node)"
                @mouseenter="hoveredSimId = node.id"
                @mouseleave="hoveredSimId = null"
              >
                <!-- Node Card Container -->
                <rect
                  :width="node.width"
                  :height="node.height"
                  rx="2"
                  :fill="selectedSimId === node.id ? 'var(--surface-bg)' : 'var(--bg-color)'"
                  :stroke="
                    selectedSimId === node.id
                      ? 'var(--accent-color)'
                      : node.isMatched
                        ? 'var(--status-ok-color)'
                        : node.kind === 'group'
                          ? getEntityColor(node.entityType)
                          : 'var(--border-color)'
                  "
                  :stroke-width="selectedSimId === node.id || node.isMatched ? 1.8 : 1"
                />

                <!-- Left Domain Accent Bar -->
                <rect
                  x="0"
                  y="0"
                  width="3"
                  :height="node.height"
                  :fill="getEntityColor(node.entityType)"
                />

                <!-- Primary Title -->
                <text
                  x="10"
                  :y="node.kind === 'group' ? 16 : 15"
                  font-size="9.5"
                  font-weight="bold"
                  fill="var(--fg-color)"
                >
                  {{ node.label }}
                </text>

                <!-- Sublabel / Count / Line Range -->
                <text
                  x="10"
                  :y="node.kind === 'group' ? 31 : 27"
                  font-size="8"
                  fill="var(--dim-color)"
                >
                  {{ node.sublabel }}
                </text>

                <!-- Expand/Collapse Button on Compound Super-Nodes -->
                <g
                  v-if="node.kind === 'group'"
                  :transform="`translate(${node.width - 22}, 9)`"
                  @click.stop="toggleGroupExpansion(node.groupKey)"
                  class="cursor-pointer"
                >
                  <rect
                    width="15"
                    height="15"
                    rx="2"
                    fill="var(--surface-bg)"
                    stroke="var(--border-color)"
                    stroke-width="0.9"
                  />
                  <text
                    x="7.5"
                    y="8.5"
                    text-anchor="middle"
                    dominant-baseline="central"
                    font-size="10"
                    font-weight="bold"
                    fill="var(--accent-color)"
                  >
                    {{ expandedGroups.has(node.groupKey) ? '−' : '+' }}
                  </text>
                </g>

                <!-- Search Score Badge -->
                <g
                  v-if="node.searchScore > 0"
                  :transform="`translate(${node.width - 32}, -7)`"
                >
                  <rect
                    width="32"
                    height="13"
                    rx="2"
                    fill="var(--status-ok-color)"
                  />
                  <text
                    x="16"
                    y="7"
                    text-anchor="middle"
                    dominant-baseline="central"
                    font-size="7.5"
                    font-weight="bold"
                    fill="var(--bg-color)"
                  >
                    {{ formatPct(node.searchScore) }}
                  </text>
                </g>

                <!-- Pin Indicator -->
                <circle
                  v-if="node.pinned"
                  :cx="node.width - 6"
                  :cy="node.height - 6"
                  r="2.5"
                  fill="var(--status-warn-color)"
                />
              </g>
            </g>
          </g>
        </svg>
      </div>

      <!-- Resizer Handle -->
      <div
        v-if="selectedSimNode"
        @mousedown="startInspectorResize"
        class="w-1 bg-[var(--border-color)] hover:bg-[var(--accent-color)] cursor-col-resize shrink-0 transition-colors"
      ></div>

      <!-- Right Panel: 1-Hop Node Inspector & Ego-Graph Controls -->
      <aside
        v-if="selectedSimNode"
        :style="{ width: `${inspectorWidthPx}px` }"
        class="flex flex-col min-h-0 bg-[var(--bg-color)] shrink-0 overflow-hidden"
      >
        <div class="h-8 px-3 border-b border-[var(--border-color)] bg-[var(--surface-bg)] flex items-center justify-between shrink-0">
          <div class="flex items-center gap-1.5 min-w-0">
            <component :is="getEntityIcon(selectedSimNode.entityType)" class="w-3.5 h-3.5 text-[var(--accent-color)] shrink-0" />
            <span class="text-[9px] font-bold uppercase tracking-wider truncate">
              {{ selectedSimNode.entityType }} · {{ selectedSimNode.kind === 'group' ? 'Module' : 'Symbol' }}
            </span>
          </div>
          <div class="flex items-center gap-1">
            <AppTooltip v-if="selectedSimNode.pinned" content="Release pinned node position">
              <button type="button" @click="unpinNode(selectedSimNode)" class="ui-icon-btn !h-5 !w-5">
                <PinOff class="w-3 h-3 text-[var(--status-warn-color)]" />
              </button>
            </AppTooltip>
            <button type="button" @click="selectedSimId = null; egoDepth = 0" class="ui-icon-btn !h-5 !w-5">
              <X class="w-3 h-3" />
            </button>
          </div>
        </div>

        <div class="flex-1 overflow-y-auto p-3 space-y-3 text-[10px]">
          <!-- Header & Ego Isolation Controls -->
          <div class="space-y-1.5 pb-2 border-b border-[var(--border-color)]">
            <div class="font-bold text-[11px] text-[var(--fg-color)] break-words">
              {{ selectedSimNode.label }}
            </div>
            <div class="text-[9px] text-[var(--dim-color)] break-all">
              {{ selectedSimNode.filePath }}
              <span v-if="selectedSimNode.rawNode"> · L{{ selectedSimNode.rawNode.lineStart }}–{{ selectedSimNode.rawNode.lineEnd }}</span>
            </div>

            <div class="flex flex-wrap items-center gap-1.5 pt-1">
              <button
                type="button"
                @click="openTargetInExplorer(selectedSimNode.filePath)"
                class="h-6 px-2 text-[8px] font-bold uppercase border border-[var(--accent-color)] text-[var(--accent-color)] hover:bg-[var(--accent-color)]/10 flex items-center gap-1 cursor-pointer"
              >
                <ExternalLink class="w-3 h-3" />
                <span>Open</span>
              </button>

              <button
                v-if="selectedSimNode.kind === 'group'"
                type="button"
                @click="toggleGroupExpansion(selectedSimNode.groupKey)"
                class="h-6 px-2 text-[8px] font-bold uppercase border border-[var(--border-color)] text-[var(--fg-color)] hover:border-[var(--accent-color)] flex items-center gap-1 cursor-pointer"
              >
                <Minus v-if="expandedGroups.has(selectedSimNode.groupKey)" class="w-3 h-3" />
                <Plus v-else class="w-3 h-3" />
                <span>{{ expandedGroups.has(selectedSimNode.groupKey) ? 'Collapse' : 'Expand' }}</span>
              </button>

              <!-- Ego-Graph Neighborhood Isolation -->
              <div class="flex items-center border border-[var(--border-color)]">
                <button
                  v-for="d in ([0, 1, 2] as const)"
                  :key="d"
                  type="button"
                  @click="egoDepth = d"
                  class="h-6 px-1.5 text-[8px] font-bold uppercase cursor-pointer"
                  :class="egoDepth === d ? 'bg-[var(--accent-color)] text-[var(--bg-color)]' : 'text-[var(--dim-color)] hover:text-[var(--fg-color)]'"
                >
                  {{ d === 0 ? 'All' : `${d}-Hop` }}
                </button>
              </div>
            </div>
          </div>

          <!-- Content Preview (for leaf nodes) -->
          <div
            v-if="selectedSimNode.rawNode?.contentPreview"
            class="p-2 bg-[var(--surface-bg)] border border-[var(--border-color)] text-[9px] text-[var(--dim-color)] whitespace-pre-wrap break-words max-h-28 overflow-y-auto"
          >
            {{ selectedSimNode.rawNode.contentPreview }}
          </div>

          <!-- Child Symbols (for Compound Super-Nodes) -->
          <div v-if="selectedSimNode.kind === 'group'" class="space-y-1">
            <div class="text-[8px] font-bold uppercase tracking-wider text-[var(--dim-color)]">
              Contained Chunks ({{ selectedSimNode.childNodes.length }})
            </div>
            <div class="max-h-36 overflow-y-auto border border-[var(--border-color)] divide-y divide-[var(--border-color)]">
              <div
                v-for="child in selectedSimNode.childNodes"
                :key="child.id"
                class="px-2 py-1 flex items-center justify-between gap-2 text-[9px] hover:bg-[var(--surface-bg)] cursor-pointer"
                @click="expandedGroups.add(selectedSimNode.groupKey); rebuildSimulationGraph(false); selectedSimId = child.id"
              >
                <span class="truncate text-[var(--fg-color)]">{{ shortSymbolTitle(child) }}</span>
                <span class="text-[8px] text-[var(--dim-color)] shrink-0">L{{ child.lineStart }}</span>
              </div>
            </div>
          </div>

          <!-- Incoming Relations -->
          <div class="space-y-1.5">
            <div class="text-[8px] font-bold uppercase tracking-wider text-[var(--dim-color)]">
              ◀ Incoming ({{ incomingConnections.length }})
            </div>
            <div v-if="incomingConnections.length === 0" class="text-[9px] text-[var(--dim-color)] italic">
              No incoming edges above {{ minWeightPct }}%.
            </div>
            <div v-else class="space-y-1">
              <div
                v-for="conn in incomingConnections"
                :key="`in-${conn.peerId}-${conn.edgeType}`"
                @click="selectedSimId = conn.peerId"
                class="p-1.5 border border-[var(--border-color)] hover:border-[var(--accent-color)] cursor-pointer space-y-0.5"
              >
                <div class="flex items-center justify-between gap-2">
                  <span
                    class="text-[8px] font-bold uppercase"
                    :style="{ color: getEdgeColor(conn.edgeType) }"
                  >
                    {{ conn.edgeType }} · {{ conn.origin }}
                  </span>
                  <span class="text-[9px] font-bold text-[var(--fg-color)]">
                    {{ formatPct(conn.weight) }}{{ conn.count > 1 ? ` (×${conn.count})` : '' }}
                  </span>
                </div>
                <div class="text-[9px] text-[var(--fg-color)] truncate">{{ conn.peerLabel }}</div>
                <div class="text-[8px] text-[var(--dim-color)] truncate">{{ conn.peerFilePath }}</div>
              </div>
            </div>
          </div>

          <!-- Outgoing Relations -->
          <div class="space-y-1.5">
            <div class="text-[8px] font-bold uppercase tracking-wider text-[var(--dim-color)]">
              ▶ Outgoing ({{ outgoingConnections.length }})
            </div>
            <div v-if="outgoingConnections.length === 0" class="text-[9px] text-[var(--dim-color)] italic">
              No outgoing edges above {{ minWeightPct }}%.
            </div>
            <div v-else class="space-y-1">
              <div
                v-for="conn in outgoingConnections"
                :key="`out-${conn.peerId}-${conn.edgeType}`"
                @click="selectedSimId = conn.peerId"
                class="p-1.5 border border-[var(--border-color)] hover:border-[var(--accent-color)] cursor-pointer space-y-0.5"
              >
                <div class="flex items-center justify-between gap-2">
                  <span
                    class="text-[8px] font-bold uppercase"
                    :style="{ color: getEdgeColor(conn.edgeType) }"
                  >
                    {{ conn.edgeType }} · {{ conn.origin }}
                  </span>
                  <span class="text-[9px] font-bold text-[var(--fg-color)]">
                    {{ formatPct(conn.weight) }}{{ conn.count > 1 ? ` (×${conn.count})` : '' }}
                  </span>
                </div>
                <div class="text-[9px] text-[var(--fg-color)] truncate">{{ conn.peerLabel }}</div>
                <div class="text-[8px] text-[var(--dim-color)] truncate">{{ conn.peerFilePath }}</div>
              </div>
            </div>
          </div>
        </div>
      </aside>
    </div>
  </div>
</template>

<style scoped>
@keyframes graphEdgeFlow {
  to {
    stroke-dashoffset: -20;
  }
}

.graph-edge-flow {
  animation: graphEdgeFlow 0.85s linear infinite;
}
</style>
