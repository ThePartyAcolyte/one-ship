//! Transversal causal & semantic knowledge service powered by local BGE-M3 (prose-only),
//! LanceDB, deterministic Tree-sitter AST symbols, and Git diff hunks (Spec 22 §5).

pub mod chunker;
pub mod engine;
pub mod errors;
pub mod store;
pub mod throttler;
pub mod types;

#[cfg(test)]
mod tests;

pub use errors::SemanticError;
pub use types::{
    FileAnchoredItem, FileCausalMinute, FileLineInsights, FileLineRangeContent, FileRelatedDoc,
    FileSymbolItem, GetFileLineInsightsArgs, GetFileLineRangeContentArgs, GetFileSymbolsArgs,
    IndexProjectArgs, IndexingStats, KnowledgeEdgeRecord, KnowledgeGraphEdge, KnowledgeGraphNode,
    KnowledgeRelation, KnowledgeSearchResult, KnowledgeScope, ProjectIndexingStatus,
    ProjectKnowledgeGraph, ResolveSymbolDefinitionArgs, SearchKnowledgeArgs, SemanticAuditArgs,
    SymbolDefinitionLocation, VectorEntityRecord,
};

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::time::{Duration, Instant};

use serde_json::json;
use sha2::{Digest, Sha256};

/// Live runtime state tracked in memory per project for `get_indexing_status`.
#[derive(Debug, Clone, Default)]
struct ProjectIndexingRuntimeState {
    is_indexing: bool,
    fully_indexed: bool,
    last_duration_ms: Option<u64>,
    debounce_generation: u64,
}

/// Global poison-resistant registry of project indexing states.
static INDEXING_STATES: OnceLock<RwLock<HashMap<i64, ProjectIndexingRuntimeState>>> = OnceLock::new();

fn indexing_states_lock() -> &'static RwLock<HashMap<i64, ProjectIndexingRuntimeState>> {
    INDEXING_STATES.get_or_init(|| RwLock::new(HashMap::new()))
}

fn read_indexing_states() -> RwLockReadGuard<'static, HashMap<i64, ProjectIndexingRuntimeState>> {
    indexing_states_lock()
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn write_indexing_states() -> RwLockWriteGuard<'static, HashMap<i64, ProjectIndexingRuntimeState>> {
    indexing_states_lock()
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// RAII guard that ensures `is_indexing` is always cleared even on early return or panic.
struct ActiveIndexingGuard {
    project_id: i64,
    completed_duration_ms: Option<u64>,
}

impl ActiveIndexingGuard {
    fn try_acquire(project_id: i64) -> Result<Self, SemanticError> {
        let mut guard = write_indexing_states();
        let entry = guard.entry(project_id).or_default();
        if entry.is_indexing {
            return Err(SemanticError::Throttled(format!(
                "Semantic indexation is already running for project {}",
                project_id
            )));
        }
        entry.is_indexing = true;
        Ok(Self {
            project_id,
            completed_duration_ms: None,
        })
    }

    fn complete(&mut self, duration_ms: u64) {
        self.completed_duration_ms = Some(duration_ms);
    }
}

impl Drop for ActiveIndexingGuard {
    fn drop(&mut self) {
        let mut guard = write_indexing_states();
        let entry = guard.entry(self.project_id).or_default();
        entry.is_indexing = false;
        if let Some(dur) = self.completed_duration_ms {
            entry.last_duration_ms = Some(dur);
            entry.fully_indexed = true;
        } else {
            entry.fully_indexed = false;
        }
    }
}

/// Resolves `(auto_index_enabled, min_score_percent, file_debounce_ms, entity_debounce_ms)`
/// for a project path from the configuration cascade.
pub fn resolve_project_semantic_config(project_path: Option<&str>) -> (bool, usize, u64, u64) {
    let enabled = crate::config::resolve_effective_bool(project_path, None, "semantic", "enabled");
    let min_score_percent = crate::config::resolve_effective_usize(
        project_path,
        None,
        "semantic",
        "semantic_edge_min_score_percent",
    )
    .clamp(1, 100);
    let file_debounce_ms = crate::config::resolve_effective_u64(
        project_path,
        None,
        "semantic",
        "file_watch_debounce_ms",
    )
    .max(500);
    let entity_debounce_ms = crate::config::resolve_effective_u64(
        project_path,
        None,
        "semantic",
        "entity_watch_debounce_ms",
    )
    .max(250);

    (
        enabled,
        min_score_percent,
        file_debounce_ms,
        entity_debounce_ms,
    )
}

/// Schedules a debounced background incremental indexation run after workspace file changes.
pub fn schedule_background_indexing_for_path(project_path: &str) {
    let path_owned = project_path.to_string();
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        return;
    };
    handle.spawn(async move {
        let project_opt = tokio::task::spawn_blocking(move || {
            let projects = crate::db::db_get_projects().ok()?;
            projects.into_iter().find(|p| p.path == path_owned)
        })
        .await
        .ok()
        .flatten();

        let Some(project) = project_opt else {
            return;
        };
        let project_id = project.id as i64;
        let (enabled, _, file_debounce_ms, _) =
            resolve_project_semantic_config(Some(project.path.as_str()));
        if !enabled {
            return;
        }
        schedule_background_project_indexing(project_id, file_debounce_ms);
    });
}

/// Schedules a debounced background incremental indexation run after SQLite journal, task, or tech-debt mutations.
pub fn schedule_background_indexing_for_entity(project_id: i64) {
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        return;
    };
    handle.spawn(async move {
        let project_path_opt = tokio::task::spawn_blocking(move || {
            let pid = usize::try_from(project_id).ok()?;
            crate::db::db_get_project_by_id(pid).ok().flatten().map(|p| p.path)
        })
        .await
        .ok()
        .flatten();

        let (enabled, _, _, entity_debounce_ms) =
            resolve_project_semantic_config(project_path_opt.as_deref());
        if !enabled {
            return;
        }
        schedule_background_project_indexing(project_id, entity_debounce_ms);
    });
}

/// Coalesces rapid triggers for `project_id` over `debounce_ms` and runs incremental background indexing
/// if the project has already been initialized in LanceDB.
pub fn schedule_background_project_indexing(project_id: i64, debounce_ms: u64) {
    let Ok(handle) = tokio::runtime::Handle::try_current() else {
        return;
    };

    let generation = {
        let mut states = write_indexing_states();
        let entry = states.entry(project_id).or_default();
        entry.debounce_generation = entry.debounce_generation.wrapping_add(1);
        entry.debounce_generation
    };

    handle.spawn(async move {
        tokio::time::sleep(Duration::from_millis(debounce_ms)).await;

        let is_latest = {
            let states = read_indexing_states();
            states
                .get(&project_id)
                .is_some_and(|s| s.debounce_generation == generation && !s.is_indexing)
        };
        if !is_latest {
            return;
        }

        // Only auto-index projects that have already been indexed at least once
        let existing_count = store::count_project_records(project_id).await.unwrap_or(0);
        let symbol_count = tokio::task::spawn_blocking(move || {
            crate::db::db_count_project_code_symbols(project_id).unwrap_or(0)
        })
        .await
        .unwrap_or(0);
        if existing_count == 0 && symbol_count == 0 {
            return;
        }

        if let Err(e) = index_project_knowledge(project_id, false).await {
            crate::logger::log(
                "DEBUG",
                "semantic",
                &format!("Background auto-indexing skipped/deferred for project {}: {}", project_id, e),
            );
        }
    });
}

/// Computes cosine similarity in `[0.0, 1.0]` between two dense vectors.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0f32;
    let mut norm_a = 0.0f32;
    let mut norm_b = 0.0f32;
    for (&x, &y) in a.iter().zip(b.iter()) {
        dot += x * y;
        norm_a += x * x;
        norm_b += y * y;
    }
    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom <= f32::EPSILON {
        0.0
    } else {
        (dot / denom).clamp(0.0, 1.0)
    }
}

/// Parses a `files_touched` summary string (comma, semicolon, or newline separated) into file-level hunks (`line_start = 0`).
pub fn parse_files_touched_hunks(files_touched: &str) -> Vec<crate::kopeng::git::DiffFileHunk> {
    let mut seen = HashSet::new();
    let mut hunks = Vec::new();
    for raw in files_touched.split(|c| matches!(c, ',' | ';' | '\n' | '\r')) {
        let cleaned = raw
            .trim()
            .trim_matches(&['`', '"', '\''][..])
            .trim_start_matches("./");
        if cleaned.is_empty() || cleaned.contains(' ') || !seen.insert(cleaned.to_string()) {
            continue;
        }
        hunks.push(crate::kopeng::git::DiffFileHunk {
            file_path: cleaned.to_string(),
            line_start: 0,
            line_end: 0,
            change_kind: "modified".to_string(),
        });
    }
    hunks
}

/// Deterministic Tree-sitter AST symbol and file search over SQLite (`project_code_symbols`).
fn search_code_symbols_deterministic(
    query: &str,
    project_id: Option<i64>,
    limit: usize,
) -> Result<Vec<KnowledgeSearchResult>, SemanticError> {
    let q_lower = query.trim().to_ascii_lowercase();
    if q_lower.is_empty() || limit == 0 {
        return Ok(Vec::new());
    }

    let symbols = crate::db::db_list_project_code_symbols(project_id)?;
    let mut scored: Vec<(f32, KnowledgeSearchResult)> = Vec::new();

    for sym in symbols {
        let sym_lower = sym.symbol_name.to_ascii_lowercase();
        let short_lower = sym.short_name.to_ascii_lowercase();
        let path_lower = sym.file_path.to_ascii_lowercase();
        let fname_lower = Path::new(&path_lower)
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();

        let score = if short_lower == q_lower || sym_lower == q_lower {
            1.0
        } else if fname_lower == q_lower || path_lower == q_lower {
            0.95
        } else if short_lower.starts_with(&q_lower) || sym_lower.starts_with(&q_lower) {
            0.88
        } else if short_lower.contains(&q_lower) || sym_lower.contains(&q_lower) {
            0.78
        } else if path_lower.contains(&q_lower) {
            0.72
        } else {
            continue;
        };

        scored.push((
            score,
            KnowledgeSearchResult {
                id: format!("file::{}::{}", sym.project_id, sym.file_path),
                project_id: sym.project_id,
                entity_type: "code".to_string(),
                entity_id: format!("{}::{}#L{}", sym.file_path, sym.symbol_name, sym.line_start),
                title: sym.title.clone(),
                content_preview: format!(
                    "{} (L{}-L{}) in {}",
                    sym.symbol_name, sym.line_start, sym.line_end, sym.file_path
                ),
                file_path: Some(sym.file_path),
                line_start: Some(sym.line_start),
                line_end: Some(sym.line_end),
                score,
                relations: Vec::new(),
            },
        ));
    }

    scored.sort_by(|a, b| {
        b.0.total_cmp(&a.0)
            .then_with(|| a.1.entity_id.cmp(&b.1.entity_id))
    });
    let mut seen_entity = HashSet::new();
    let mut results = Vec::with_capacity(limit);
    for (_, item) in scored {
        if seen_entity.insert(item.entity_id.clone()) {
            results.push(item);
            if results.len() >= limit {
                break;
            }
        }
    }
    Ok(results)
}

/// Executes hybrid knowledge search: BGE-M3 semantic search across prose (`specs`, `journal`, `tasks`, `tech_debt`)
/// combined with deterministic AST symbol lookup (`code`), resolving live line numbers and 1-hop graph relations.
pub async fn search_knowledge(
    args: SearchKnowledgeArgs,
) -> Result<Vec<KnowledgeSearchResult>, SemanticError> {
    let query_trimmed = args.query.trim();
    if query_trimmed.is_empty() {
        return Err(SemanticError::InvalidRequest("Search query cannot be empty".to_string()));
    }

    let limit = args.limit.unwrap_or(5).clamp(1, 50);
    let include_relations = args.include_relations.unwrap_or(true);
    let default_max_relations = crate::config::resolve_effective_usize(
        None,
        None,
        "semantic",
        "max_relations_per_result_count",
    )
    .clamp(1, 50);
    let max_relations = args
        .max_relations_per_result
        .unwrap_or(default_max_relations)
        .clamp(1, 50);
    let min_rel_score_ratio = args
        .min_relation_score_percent
        .map(|pct| (pct.clamp(1, 100) as f64) / 100.0)
        .unwrap_or(0.0);
    let filter_project_id = args.project_id;
    let scope = args.scope.unwrap_or(KnowledgeScope::All);

    crate::logger::log(
        "DEBUG",
        "semantic",
        &format!(
            "search_knowledge: query_len={}, scope={:?}, project_id={:?}, limit={}, include_relations={}",
            query_trimmed.len(),
            scope,
            filter_project_id,
            limit,
            include_relations
        ),
    );

    let mut results: Vec<KnowledgeSearchResult> = Vec::new();

    if scope != KnowledgeScope::Code {
        let query_text = query_trimmed.to_string();
        let query_vector = tokio::task::spawn_blocking(move || engine::embed_single(&query_text))
            .await
            .map_err(|e| SemanticError::EmbeddingFailed(format!("Tokio worker panic: {}", e)))??;

        let vector_scope = if scope == KnowledgeScope::All { None } else { Some(scope) };
        let vector_hits = store::search_knowledge(&query_vector, vector_scope, filter_project_id, limit).await?;
        results.extend(vector_hits);
    }

    if matches!(scope, KnowledgeScope::All | KnowledgeScope::Code) {
        let query_code = query_trimmed.to_string();
        let code_hits = tokio::task::spawn_blocking(move || {
            search_code_symbols_deterministic(&query_code, filter_project_id, limit)
        })
        .await
        .map_err(|e| SemanticError::Io(std::io::Error::other(format!("Code symbol search failed: {}", e))))??;
        results.extend(code_hits);
    }

    results.sort_by(|a, b| b.score.total_cmp(&a.score));
    let mut seen_keys = HashSet::with_capacity(results.len());
    results.retain(|r| seen_keys.insert(format!("{}::{}", r.id, r.entity_id)));
    results.truncate(limit);

    // 3. JIT Line Resolution & 1-Hop Knowledge Graph Expansion in `spawn_blocking`
    let results = tokio::task::spawn_blocking(move || {
        let mut project_roots: HashMap<i64, PathBuf> = HashMap::new();

        for item in &mut results {
            if item.entity_type == "code" {
                continue;
            }
            let Some(ref fpath) = item.file_path else {
                continue;
            };
            if fpath.starts_with("journal://") {
                continue;
            }
            let Some(old_start) = item.line_start else {
                continue;
            };

            let root_opt = project_roots
                .entry(item.project_id)
                .or_insert_with(|| {
                    usize::try_from(item.project_id)
                        .ok()
                        .and_then(|pid| crate::db::db_get_project_by_id(pid).ok().flatten())
                        .map(|p| PathBuf::from(p.path))
                        .unwrap_or_default()
                })
                .clone();

            let candidate = Path::new(fpath);
            let resolved_path = if candidate.is_absolute() {
                candidate.to_path_buf()
            } else if !root_opt.as_os_str().is_empty() {
                root_opt.join(candidate)
            } else {
                candidate.to_path_buf()
            };

            if !resolved_path.exists() || !resolved_path.is_file() {
                continue;
            }

            let anchor = item
                .content_preview
                .lines()
                .map(str::trim)
                .find(|l| !l.is_empty());

            if let Some(anchor_line) = anchor {
                if let Ok(file_content) = std::fs::read_to_string(&resolved_path) {
                    if let Some(pos) = file_content.lines().position(|l| l.trim() == anchor_line) {
                        let new_start = (pos + 1) as u32;
                        let span = item
                            .line_end
                            .unwrap_or(old_start)
                            .saturating_sub(old_start);
                        item.line_start = Some(new_start);
                        item.line_end = Some(new_start + span);
                    }
                }
            }
        }

        if include_relations && !results.is_empty() {
            let seed_ids: Vec<String> = results.iter().map(|r| r.id.clone()).collect();
            if let Ok(rel_map) = crate::db::db_get_knowledge_relations_for_ids(
                filter_project_id,
                &seed_ids,
                max_relations,
                min_rel_score_ratio,
            ) {
                for item in &mut results {
                    if let Some(rels) = rel_map.get(&item.id) {
                        item.relations = rels.clone();
                    }
                }
            }
        }

        results
    })
    .await
    .map_err(|e| SemanticError::Io(std::io::Error::other(format!("JIT line & graph resolution task failed: {}", e))))?;

    Ok(results)
}

/// Returns the current vector index status, active flag, category breakdown, graph edges count, and last duration.
pub async fn get_indexing_status(project_id: i64) -> Result<ProjectIndexingStatus, SemanticError> {
    let (is_indexing, last_duration_ms) = {
        let states = read_indexing_states();
        states
            .get(&project_id)
            .map(|s| (s.is_indexing, s.last_duration_ms))
            .unwrap_or((false, None))
    };

    let (total_records_count, _vec_code_count, spec_chunks_count, entity_chunks_count) =
        store::count_project_records_breakdown(project_id).await?;

    let (code_chunks_count, graph_edges_count, auto_index_enabled, min_score_percent) =
        tokio::task::spawn_blocking(move || {
            let symbols = crate::db::db_count_project_code_symbols(project_id).unwrap_or(0);
            let edges = crate::db::db_count_project_knowledge_edges(project_id).unwrap_or(0);
            let project_path = usize::try_from(project_id)
                .ok()
                .and_then(|pid| crate::db::db_get_project_by_id(pid).ok().flatten())
                .map(|p| p.path);
            let (enabled, min_pct, _, _) =
                resolve_project_semantic_config(project_path.as_deref());
            (symbols, edges, enabled, min_pct)
        })
        .await
        .unwrap_or((0, 0, true, 70));

    Ok(ProjectIndexingStatus {
        project_id,
        is_indexing,
        total_records_count,
        code_chunks_count,
        spec_chunks_count,
        entity_chunks_count,
        graph_edges_count,
        auto_index_enabled,
        min_score_percent,
        last_duration_ms,
    })
}

/// Atomically indexes a single journal record into LanceDB and schedules a debounced graph refresh.
pub fn index_single_journal_entry(
    project_id: i64,
    journal_id: i64,
    agent_id: String,
    title: String,
    summary: String,
    decisions: Option<String>,
    files_touched: Option<String>,
    technical_debt: Option<String>,
) {
    tokio::spawn(async move {
        let content = format!(
            "Agent: {}\nSummary: {}\nDecisions: {}\nFiles Touched: {}\nTechnical Debt Notes: {}",
            agent_id,
            summary,
            decisions.unwrap_or_default(),
            files_touched.unwrap_or_default(),
            technical_debt.unwrap_or_default()
        );
        let file_sha256 = chunker::hash_text(&content);
        let refs = chunker::extract_text_references(&content);
        let sym = format!("journal_{}", journal_id);
        let chunk = chunker::RawChunk::new(
            format!("journal::{}::{}", project_id, journal_id),
            "journal".to_string(),
            sym.clone(),
            sym,
            format!("Journal: {}", title),
            content,
            format!("journal://{}", journal_id),
            file_sha256,
            1,
            1,
            refs,
        );
        let records_res = tokio::task::spawn_blocking(move || {
            chunker::vectorize_chunks(project_id, vec![chunk])
        })
        .await;
        match records_res {
            Ok(Ok(records)) => {
                if let Err(e) = store::upsert_records(records).await {
                    crate::logger::log(
                        "WARN",
                        "semantic",
                        &format!("Failed to index journal record {}: {}", journal_id, e),
                    );
                } else {
                    schedule_background_indexing_for_entity(project_id);
                }
            }
            Ok(Err(e)) => {
                crate::logger::log(
                    "WARN",
                    "semantic",
                    &format!("Failed to vectorize journal record {}: {}", journal_id, e),
                );
            }
            Err(e) => {
                crate::logger::log(
                    "WARN",
                    "semantic",
                    &format!("Vectorization task panicked for journal record {}: {}", journal_id, e),
                );
            }
        }
    });
}

/// Maximum number of semantic `documents` edges retained per journal minute.
const MAX_SEMANTIC_SPECS_PER_JOURNAL_COUNT: usize = 5;
/// Maximum number of semantic `relates_to` journal edges retained per debt/todo item.
const MAX_SEMANTIC_JOURNALS_PER_ITEM_COUNT: usize = 3;

/// Direct SQLite anchor metadata for `tech_debt` and `tasks` entities.
#[derive(Debug, Clone)]
pub(crate) struct EntityAnchorMeta {
    pub node_id: String,
    pub entity_type: String,
    pub title: String,
    pub file_path: Option<String>,
    pub line_start: Option<u32>,
    pub journal_id: Option<i64>,
}

/// Outcome of the synchronous workspace scan, two-tier SHA-256 diff pass, and deterministic AST symbol sync.
struct IncrementalScanOutcome {
    project_name: String,
    project_path: String,
    chunks_to_embed: Vec<chunker::RawChunk>,
    reused_records_to_upsert: Vec<VectorEntityRecord>,
    unchanged_retained_records: Vec<VectorEntityRecord>,
    refs_by_id: HashMap<String, Vec<String>>,
    orphan_ids_to_delete: Vec<String>,
    known_project_files: HashSet<String>,
    indexed_specs_count: usize,
    indexed_symbols_count: usize,
    indexed_entities_count: usize,
    skipped_files_count: usize,
    reused_chunks_count: usize,
}

/// Metadata summary for a prose node participating in the project knowledge graph.
#[derive(Debug, Clone)]
pub(crate) struct GraphNodeMeta {
    pub id: String,
    pub entity_type: String,
    pub title: String,
    pub file_path: String,
    pub line_start: u32,
    pub refs: Vec<String>,
    pub vector: Vec<f32>,
}

/// Builds clean causal (`touched`, `anchor`, `conversation`), exact-path (`explicit_ref`), and
/// prose-only semantic (`documents`, `relates_to`) knowledge graph edges without noisy AST `calls` or `doc -> code` pairs.
pub(crate) fn build_project_knowledge_edges(
    project_id: i64,
    nodes: &[GraphNodeMeta],
    hunks: &[crate::db::JournalFileHunkRecord],
    anchors: &[EntityAnchorMeta],
    known_project_files: &HashSet<String>,
    min_score_percent: usize,
) -> Vec<KnowledgeEdgeRecord> {
    let mut node_by_id: HashMap<&str, &GraphNodeMeta> = HashMap::with_capacity(nodes.len());
    let mut spec_first_idx_by_file: HashMap<&str, usize> = HashMap::new();
    let mut spec_indices: Vec<usize> = Vec::new();
    let mut journal_indices: Vec<usize> = Vec::new();

    for (idx, node) in nodes.iter().enumerate() {
        node_by_id.insert(node.id.as_str(), node);
        match node.entity_type.as_str() {
            "specs" => {
                spec_indices.push(idx);
                spec_first_idx_by_file
                    .entry(node.file_path.as_str())
                    .or_insert(idx);
            }
            "journal" => journal_indices.push(idx),
            _ => {}
        }
    }

    // Map unique file basenames -> full relative path (`None` if ambiguous across directories).
    let mut unique_basenames: HashMap<String, Option<String>> = HashMap::new();
    for path in known_project_files {
        if let Some(fname) = Path::new(path).file_name().and_then(|s| s.to_str()) {
            unique_basenames
                .entry(fname.to_string())
                .and_modify(|slot| {
                    if slot.as_deref() != Some(path.as_str()) {
                        *slot = None;
                    }
                })
                .or_insert_with(|| Some(path.clone()));
        }
    }

    let resolve_exact_file = |raw_token: &str| -> Option<String> {
        let cleaned = raw_token.trim().trim_start_matches("./");
        if cleaned.is_empty() {
            return None;
        }
        if known_project_files.contains(cleaned) {
            return Some(cleaned.to_string());
        }
        if !cleaned.contains('/') {
            if let Some(Some(full)) = unique_basenames.get(cleaned) {
                return Some(full.clone());
            }
        }
        None
    };

    let mut edges = Vec::new();
    let mut seen_pairs: HashSet<(String, String, String)> = HashSet::new();

    let push_edge = |edges: &mut Vec<KnowledgeEdgeRecord>,
                     seen: &mut HashSet<(String, String, String)>,
                     edge: KnowledgeEdgeRecord| {
        let key = (edge.source_id.clone(), edge.target_id.clone(), edge.edge_type.clone());
        if seen.insert(key) {
            edges.push(edge);
        }
    };

    // Pass 1: Causal Git commit/snapshot hunks and `files_touched` (`journal --(touched)--> file`)
    let mut best_hunk_by_pair: HashMap<(i64, &str), (u32, &str)> = HashMap::new();
    for h in hunks {
        let key = (h.journal_id, h.file_path.as_str());
        best_hunk_by_pair
            .entry(key)
            .and_modify(|existing| {
                if existing.0 == 0 && h.line_start > 0 {
                    *existing = (h.line_start, h.source_kind.as_str());
                }
            })
            .or_insert((h.line_start, h.source_kind.as_str()));
    }

    for ((journal_id, file_path), (line_start, source_kind)) in best_hunk_by_pair {
        let j_id = format!("journal::{}::{}", project_id, journal_id);
        let Some(j_node) = node_by_id.get(j_id.as_str()) else {
            continue;
        };
        let origin = if line_start > 0 && source_kind != "files_touched" {
            "git_hunk"
        } else {
            "files_touched"
        };
        push_edge(
            &mut edges,
            &mut seen_pairs,
            KnowledgeEdgeRecord {
                project_id,
                source_id: j_node.id.clone(),
                source_entity_type: "journal".to_string(),
                source_title: j_node.title.clone(),
                source_file_path: Some(j_node.file_path.clone()),
                source_line_start: Some(j_node.line_start),
                target_id: format!("file::{}::{}", project_id, file_path),
                target_entity_type: "file".to_string(),
                target_title: file_path.to_string(),
                target_file_path: Some(file_path.to_string()),
                target_line_start: (line_start > 0).then_some(line_start),
                edge_type: "touched".to_string(),
                origin: origin.to_string(),
                weight: 1.0,
            },
        );
    }

    // Pass 2: Direct SQLite anchors on `tech_debt` and `tasks` (`file_path`, `journal_id`)
    for anchor in anchors {
        if let Some(ref fpath) = anchor.file_path {
            let norm = fpath.trim().trim_start_matches("./");
            if !norm.is_empty() {
                push_edge(
                    &mut edges,
                    &mut seen_pairs,
                    KnowledgeEdgeRecord {
                        project_id,
                        source_id: anchor.node_id.clone(),
                        source_entity_type: anchor.entity_type.clone(),
                        source_title: anchor.title.clone(),
                        source_file_path: Some(norm.to_string()),
                        source_line_start: anchor.line_start,
                        target_id: format!("file::{}::{}", project_id, norm),
                        target_entity_type: "file".to_string(),
                        target_title: norm.to_string(),
                        target_file_path: Some(norm.to_string()),
                        target_line_start: anchor.line_start,
                        edge_type: "affects".to_string(),
                        origin: "anchor".to_string(),
                        weight: 1.0,
                    },
                );
            }
        }
        if let Some(jid) = anchor.journal_id {
            let j_id = format!("journal::{}::{}", project_id, jid);
            if let Some(j_node) = node_by_id.get(j_id.as_str()) {
                push_edge(
                    &mut edges,
                    &mut seen_pairs,
                    KnowledgeEdgeRecord {
                        project_id,
                        source_id: anchor.node_id.clone(),
                        source_entity_type: anchor.entity_type.clone(),
                        source_title: anchor.title.clone(),
                        source_file_path: anchor.file_path.clone(),
                        source_line_start: anchor.line_start,
                        target_id: j_node.id.clone(),
                        target_entity_type: "journal".to_string(),
                        target_title: j_node.title.clone(),
                        target_file_path: Some(j_node.file_path.clone()),
                        target_line_start: Some(j_node.line_start),
                        edge_type: "relates_to".to_string(),
                        origin: "conversation".to_string(),
                        weight: 1.0,
                    },
                );
            }
        }
    }

    // Pass 3: Exact file-path explicit references (`specs`/`journal`/`tech_debt`/`tasks` -> `specs` or `file`)
    for source in nodes {
        if source.refs.is_empty() {
            continue;
        }
        for token in &source.refs {
            let Some(resolved_path) = resolve_exact_file(token) else {
                continue;
            };
            if resolved_path == source.file_path {
                continue;
            }

            if let Some(&spec_idx) = spec_first_idx_by_file.get(resolved_path.as_str()) {
                let target = &nodes[spec_idx];
                if target.id == source.id {
                    continue;
                }
                let edge_type = match source.entity_type.as_str() {
                    "specs" => "references",
                    "journal" => "documents",
                    _ => "relates_to",
                };
                push_edge(
                    &mut edges,
                    &mut seen_pairs,
                    KnowledgeEdgeRecord {
                        project_id,
                        source_id: source.id.clone(),
                        source_entity_type: source.entity_type.clone(),
                        source_title: source.title.clone(),
                        source_file_path: Some(source.file_path.clone()),
                        source_line_start: Some(source.line_start),
                        target_id: target.id.clone(),
                        target_entity_type: target.entity_type.clone(),
                        target_title: target.title.clone(),
                        target_file_path: Some(target.file_path.clone()),
                        target_line_start: Some(target.line_start),
                        edge_type: edge_type.to_string(),
                        origin: "explicit_ref".to_string(),
                        weight: 1.0,
                    },
                );
            } else {
                let edge_type = match source.entity_type.as_str() {
                    "specs" => "governs",
                    "journal" => "touched",
                    _ => "affects",
                };
                push_edge(
                    &mut edges,
                    &mut seen_pairs,
                    KnowledgeEdgeRecord {
                        project_id,
                        source_id: source.id.clone(),
                        source_entity_type: source.entity_type.clone(),
                        source_title: source.title.clone(),
                        source_file_path: Some(source.file_path.clone()),
                        source_line_start: Some(source.line_start),
                        target_id: format!("file::{}::{}", project_id, resolved_path),
                        target_entity_type: "file".to_string(),
                        target_title: resolved_path.clone(),
                        target_file_path: Some(resolved_path),
                        target_line_start: None,
                        edge_type: edge_type.to_string(),
                        origin: "explicit_ref".to_string(),
                        weight: 1.0,
                    },
                );
            }
        }
    }

    // Pass 4: Prose-only semantic links (`journal -> specs` and `tech_debt/tasks -> journal`)
    let threshold_ratio = (min_score_percent.clamp(1, 100) as f32) / 100.0;
    if !spec_indices.is_empty() {
        for &j_idx in &journal_indices {
            let source = &nodes[j_idx];
            if source.vector.is_empty() {
                continue;
            }
            let mut candidates: Vec<(usize, f32)> = Vec::new();
            for &s_idx in &spec_indices {
                let target = &nodes[s_idx];
                if target.vector.is_empty() {
                    continue;
                }
                let sim = cosine_similarity(&source.vector, &target.vector);
                if sim >= threshold_ratio {
                    candidates.push((s_idx, sim));
                }
            }
            candidates.sort_by(|a, b| b.1.total_cmp(&a.1));
            for (s_idx, sim) in candidates.into_iter().take(MAX_SEMANTIC_SPECS_PER_JOURNAL_COUNT) {
                let target = &nodes[s_idx];
                push_edge(
                    &mut edges,
                    &mut seen_pairs,
                    KnowledgeEdgeRecord {
                        project_id,
                        source_id: source.id.clone(),
                        source_entity_type: "journal".to_string(),
                        source_title: source.title.clone(),
                        source_file_path: Some(source.file_path.clone()),
                        source_line_start: Some(source.line_start),
                        target_id: target.id.clone(),
                        target_entity_type: "specs".to_string(),
                        target_title: target.title.clone(),
                        target_file_path: Some(target.file_path.clone()),
                        target_line_start: Some(target.line_start),
                        edge_type: "documents".to_string(),
                        origin: "semantic".to_string(),
                        weight: f64::from(sim),
                    },
                );
            }
        }
    }

    if !journal_indices.is_empty() {
        for source in nodes {
            if !matches!(source.entity_type.as_str(), "tech_debt" | "tasks") || source.vector.is_empty() {
                continue;
            }
            let mut candidates: Vec<(usize, f32)> = Vec::new();
            for &j_idx in &journal_indices {
                let target = &nodes[j_idx];
                if target.vector.is_empty() {
                    continue;
                }
                let sim = cosine_similarity(&source.vector, &target.vector);
                if sim >= threshold_ratio {
                    candidates.push((j_idx, sim));
                }
            }
            candidates.sort_by(|a, b| b.1.total_cmp(&a.1));
            for (j_idx, sim) in candidates.into_iter().take(MAX_SEMANTIC_JOURNALS_PER_ITEM_COUNT) {
                let target = &nodes[j_idx];
                push_edge(
                    &mut edges,
                    &mut seen_pairs,
                    KnowledgeEdgeRecord {
                        project_id,
                        source_id: source.id.clone(),
                        source_entity_type: source.entity_type.clone(),
                        source_title: source.title.clone(),
                        source_file_path: Some(source.file_path.clone()),
                        source_line_start: Some(source.line_start),
                        target_id: target.id.clone(),
                        target_entity_type: "journal".to_string(),
                        target_title: target.title.clone(),
                        target_file_path: Some(target.file_path.clone()),
                        target_line_start: Some(target.line_start),
                        edge_type: "relates_to".to_string(),
                        origin: "semantic".to_string(),
                        weight: f64::from(sim),
                    },
                );
            }
        }
    }

    edges
}

/// Loads `EntityAnchorMeta` for all `tasks` and `tech_debt` rows of `project_id`.
fn load_project_entity_anchors(project_id: i64) -> Result<Vec<EntityAnchorMeta>, SemanticError> {
    let mut anchors = Vec::new();
    for todo in crate::db::db_get_project_todos(project_id, None)? {
        let tid = todo.id.unwrap_or(0);
        if todo.file_path.is_some() || todo.journal_id.is_some() {
            anchors.push(EntityAnchorMeta {
                node_id: format!("tasks::{}::{}", project_id, tid),
                entity_type: "tasks".to_string(),
                title: format!("Task: {}", todo.title),
                file_path: todo.file_path,
                line_start: todo.line_start,
                journal_id: todo.journal_id,
            });
        }
    }
    for debt in crate::db::db_get_project_technical_debt(project_id, None)? {
        let did = debt.id.unwrap_or(0);
        if debt.file_path.is_some() || debt.journal_id.is_some() {
            anchors.push(EntityAnchorMeta {
                node_id: format!("tech_debt::{}::{}", project_id, did),
                entity_type: "tech_debt".to_string(),
                title: format!("Tech Debt: {}", debt.title),
                file_path: debt.file_path,
                line_start: debt.line_start,
                journal_id: debt.journal_id,
            });
        }
    }
    Ok(anchors)
}

/// Recursively scans and incrementally indexes a project's prose knowledge (`specs`, `journal`, `tasks`, `tech_debt`),
/// deterministic AST symbols (`project_code_symbols`), historical Git hunks, and causal knowledge graph edges.
pub async fn index_project_knowledge(
    project_id: i64,
    force: bool,
) -> Result<IndexingStats, SemanticError> {
    let mut active_guard = ActiveIndexingGuard::try_acquire(project_id)?;
    let start_time = Instant::now();

    // 1. Fetch existing prose records from LanceDB for two-tier SHA-256 incremental diffing
    let existing_records = store::fetch_project_records(project_id).await?;
    let was_fully_indexed = {
        let states = read_indexing_states();
        states.get(&project_id).is_some_and(|s| s.fully_indexed)
    };

    // 2. Scan docs, deterministic AST symbols, SQLite entities, and backfill causal Git hunks
    let scan_outcome = tokio::task::spawn_blocking(move || -> Result<IncrementalScanOutcome, SemanticError> {
        if force {
            if !throttler::can_run_manual_indexing() {
                return Err(SemanticError::Throttled(
                    "Host memory is critically low; indexation aborted to prevent OOM".to_string(),
                ));
            }
        } else if !throttler::can_run_background_indexing() {
            return Err(SemanticError::Throttled(
                "Background indexing paused due to active internal tasks or host resource constraints".to_string(),
            ));
        }

        let project_id_usize = usize::try_from(project_id)
            .map_err(|_| SemanticError::InvalidRequest("Invalid project ID".to_string()))?;
        let project = crate::db::db_get_project_by_id(project_id_usize)?
            .ok_or_else(|| SemanticError::InvalidRequest(format!("Project ID {} not found", project_id)))?;

        let root_path = Path::new(&project.path);
        if !root_path.exists() {
            return Err(SemanticError::InvalidRequest(format!(
                "Project path '{:?}' does not exist on disk",
                root_path
            )));
        }

        crate::logger::log(
            "INFO",
            "semantic",
            &format!(
                "Starting incremental causal & prose indexation for project '{}' at {:?} (existing prose records: {}, was_fully_indexed: {})",
                project.name,
                root_path,
                existing_records.len(),
                was_fully_indexed
            ),
        );

        let mut existing_by_file: HashMap<String, Vec<&VectorEntityRecord>> = HashMap::new();
        if was_fully_indexed {
            for rec in existing_records.values() {
                if rec.entity_type != "code" {
                    existing_by_file
                        .entry(rec.file_path.clone())
                        .or_default()
                        .push(rec);
                }
            }
        }

        let project_path_str = project.path.as_str();
        let ignored_patterns = crate::config::resolve_effective_string_list(
            Some(project_path_str),
            None,
            "semantic",
            "ignored_patterns",
        );
        let included_extensions = crate::config::resolve_effective_string_list(
            Some(project_path_str),
            None,
            "semantic",
            "included_extensions",
        );
        let extra_paths = crate::config::resolve_effective_string_list(
            Some(project_path_str),
            None,
            "semantic",
            "extra_paths",
        );

        let mut scanned_doc_dirs = vec![root_path.join("docs").join("specs")];
        for extra in &extra_paths {
            let ep = root_path.join(extra);
            if ep.exists() && ep.is_dir() && !scanned_doc_dirs.contains(&ep) {
                scanned_doc_dirs.push(ep);
            }
        }

        let max_depth_count = crate::config::resolve_effective_usize(None, None, "system", "max_dir_walk_depth_count").max(1);
        let max_nodes_count = crate::config::resolve_effective_usize(None, None, "system", "max_dir_walk_nodes_count").max(100);
        let max_file_size_bytes = crate::config::resolve_effective_usize(
            Some(project_path_str),
            None,
            "semantic",
            "max_file_size_bytes",
        ) as u64;
        let mut visited_nodes_count = 0usize;
        let mut visited_files: HashSet<String> = HashSet::new();
        let mut known_project_files: HashSet<String> = HashSet::new();

        let mut raw_chunks = Vec::new();
        let mut retained_ids: HashSet<String> = HashSet::new();
        let mut indexed_specs_count = 0usize;
        let mut skipped_files_count = 0usize;

        // 4. Scan Architectural Specifications & Docs (`.md`, `.txt`, `.pdf`) for BGE-M3
        for dir in &scanned_doc_dirs {
            scan_docs_directory(
                project_id,
                root_path,
                dir,
                &ignored_patterns,
                max_file_size_bytes,
                &existing_by_file,
                &mut visited_files,
                &mut retained_ids,
                &mut raw_chunks,
                &mut indexed_specs_count,
                &mut skipped_files_count,
                0,
                max_depth_count,
                &mut visited_nodes_count,
                max_nodes_count,
            );
        }
        known_project_files.extend(visited_files.iter().cloned());

        // 5. Scan Source Code Files strictly for deterministic SQLite AST Symbols (`project_code_symbols`, zero BGE-M3 vectors)
        let existing_symbol_hashes = crate::db::db_get_project_code_symbol_hashes(project_id)?;
        let mut visited_code_files: HashSet<String> = HashSet::new();
        let mut changed_code_files: Vec<String> = Vec::new();
        let mut new_code_symbols: Vec<crate::db::CodeSymbolRecord> = Vec::new();
        let mut code_nodes_count = 0usize;
        scan_code_symbols_directory(
            project_id,
            root_path,
            root_path,
            &scanned_doc_dirs,
            &ignored_patterns,
            &included_extensions,
            max_file_size_bytes,
            &existing_symbol_hashes,
            &mut visited_code_files,
            &mut known_project_files,
            &mut changed_code_files,
            &mut new_code_symbols,
            0,
            max_depth_count,
            &mut code_nodes_count,
            max_nodes_count,
        );
        let deleted_code_files: Vec<String> = existing_symbol_hashes
            .keys()
            .filter(|fp| !visited_code_files.contains(*fp))
            .cloned()
            .collect();
        crate::db::db_sync_project_code_symbols(
            project_id,
            &changed_code_files,
            &new_code_symbols,
            &deleted_code_files,
        )?;
        let indexed_symbols_count = crate::db::db_count_project_code_symbols(project_id)?;

        // 6. Collect Database Entities (Journal, Tasks & Technical Debt) + Backfill missing `journal_file_hunks`
        let mut indexed_entities_count = 0usize;
        let existing_hunk_journals = crate::db::db_get_journal_ids_with_hunks(project_id)?;
        let has_git_repo = root_path.join(".git").exists();
        let journals = crate::db::db_get_project_session_journals(project_id, None, 1000)?;

        for j in journals {
            indexed_entities_count += 1;
            let jid = j.id.unwrap_or(0);
            let decisions_str = j.decisions.unwrap_or_default();
            let files_str = j.files_touched.unwrap_or_default();
            let debt_str = j.technical_debt.unwrap_or_default();

            if jid > 0 && !existing_hunk_journals.contains(&jid) {
                let mut backfilled = false;
                if has_git_repo {
                    if let Some(ref commit_hash) = j.commit_hash {
                        if let Ok(hunks) = crate::kopeng::git::extract_commit_diff_hunks(root_path, commit_hash) {
                            if !hunks.is_empty() {
                                crate::db::db_insert_journal_file_hunks(project_id, jid, &hunks, "git_commit")?;
                                backfilled = true;
                            }
                        }
                    }
                }
                if !backfilled && !files_str.trim().is_empty() {
                    let file_hunks = parse_files_touched_hunks(&files_str);
                    if !file_hunks.is_empty() {
                        crate::db::db_insert_journal_file_hunks(project_id, jid, &file_hunks, "files_touched")?;
                    }
                }
            }

            let content = format!(
                "Agent: {}\nSummary: {}\nDecisions: {}\nFiles Touched: {}\nTechnical Debt: {}",
                j.agent_id, j.summary, decisions_str, files_str, debt_str
            );
            let fhash = chunker::hash_text(&content);
            let refs = chunker::extract_text_references(&content);
            let sym = format!("journal_{}", jid);
            raw_chunks.push(chunker::RawChunk::new(
                format!("journal::{}::{}", project_id, jid),
                "journal".to_string(),
                sym.clone(),
                sym,
                format!("Journal: {}", j.title),
                content,
                format!("journal://{}", jid),
                fhash,
                1,
                1,
                refs,
            ));
        }

        for todo in crate::db::db_get_project_todos(project_id, None)? {
            indexed_entities_count += 1;
            let tid = todo.id.unwrap_or(0);
            let desc = todo.description.unwrap_or_default();
            let anchor_file = todo.file_path.as_deref().unwrap_or("");
            let content = format!(
                "Status: {}\nPriority: {}\nFile: {}\nDetails: {}",
                todo.status, todo.priority, anchor_file, desc
            );
            let fhash = chunker::hash_text(&content);
            let refs = chunker::extract_text_references(&content);
            let sym = format!("todo_{}", tid);
            raw_chunks.push(chunker::RawChunk::new(
                format!("tasks::{}::{}", project_id, tid),
                "tasks".to_string(),
                sym.clone(),
                sym,
                format!("Task: {}", todo.title),
                content,
                todo.file_path.unwrap_or_else(|| "TODO.md".to_string()),
                fhash,
                todo.line_start.unwrap_or(1),
                todo.line_end.unwrap_or(1),
                refs,
            ));
        }

        for debt in crate::db::db_get_project_technical_debt(project_id, None)? {
            indexed_entities_count += 1;
            let did = debt.id.unwrap_or(0);
            let remediation = debt.remediation_plan.unwrap_or_default();
            let anchor_file = debt.file_path.as_deref().unwrap_or("");
            let content = format!(
                "Status: {}\nSeverity: {}\nFile: {}\nImpact: {}\nRemediation: {}",
                debt.status, debt.severity, anchor_file, debt.impact_summary, remediation
            );
            let fhash = chunker::hash_text(&content);
            let refs = chunker::extract_text_references(&content);
            let sym = format!("debt_{}", did);
            raw_chunks.push(chunker::RawChunk::new(
                format!("tech_debt::{}::{}", project_id, did),
                "tech_debt".to_string(),
                sym.clone(),
                sym,
                format!("Tech Debt: {}", debt.title),
                content,
                debt.file_path.unwrap_or_else(|| "TECHNICAL_DEBT.md".to_string()),
                fhash,
                debt.line_start.unwrap_or(1),
                debt.line_end.unwrap_or(1),
                refs,
            ));
        }

        let mut unchanged_retained_records: Vec<VectorEntityRecord> = Vec::with_capacity(retained_ids.len());
        for kept_id in &retained_ids {
            if let Some(rec) = existing_records.get(kept_id) {
                unchanged_retained_records.push(rec.clone());
            }
        }

        let mut refs_by_id: HashMap<String, Vec<String>> = HashMap::with_capacity(raw_chunks.len());
        let mut chunks_to_embed = Vec::new();
        let mut reused_records_to_upsert = Vec::new();
        let mut reused_chunks_count = 0usize;

        for chunk in raw_chunks {
            retained_ids.insert(chunk.id.clone());
            refs_by_id.insert(chunk.id.clone(), chunk.referenced_identifiers.clone());

            if let Some(existing) = existing_records.get(&chunk.id) {
                if existing.content_sha256 == chunk.content_sha256 && !existing.vector.is_empty() {
                    reused_chunks_count += 1;
                    if existing.line_start != chunk.line_start
                        || existing.line_end != chunk.line_end
                        || existing.file_sha256 != chunk.file_sha256
                        || existing.title != chunk.title
                        || existing.symbol_name != chunk.symbol_name
                        || existing.file_path != chunk.file_path
                    {
                        reused_records_to_upsert.push(chunker::build_vector_record(
                            project_id,
                            chunk,
                            existing.vector.clone(),
                        ));
                    } else {
                        unchanged_retained_records.push(existing.clone());
                    }
                    continue;
                }
            }

            chunks_to_embed.push(chunk);
        }

        let orphan_ids_to_delete: Vec<String> = existing_records
            .keys()
            .filter(|id| !retained_ids.contains(*id))
            .cloned()
            .collect();

        Ok(IncrementalScanOutcome {
            project_name: project.name,
            project_path: project.path,
            chunks_to_embed,
            reused_records_to_upsert,
            unchanged_retained_records,
            refs_by_id,
            orphan_ids_to_delete,
            known_project_files,
            indexed_specs_count,
            indexed_symbols_count,
            indexed_entities_count,
            skipped_files_count,
            reused_chunks_count,
        })
    })
    .await
    .map_err(|e| SemanticError::Io(std::io::Error::other(format!("Semantic scan task failed: {}", e))))??;

    // 7. Delete orphaned records from deleted files or legacy code records
    if !scan_outcome.orphan_ids_to_delete.is_empty() {
        store::delete_records_by_ids(project_id, &scan_outcome.orphan_ids_to_delete).await?;
    }

    // 8. Persist reused metadata and vectorize new/modified prose chunks
    let batch_size_count = crate::config::resolve_effective_usize(
        None,
        None,
        "semantic",
        "embedding_batch_size_count",
    )
    .max(1);
    let batch_sleep_ms = crate::config::resolve_effective_u64(
        None,
        None,
        "semantic",
        "embedding_batch_sleep_ms",
    );
    let embedded_count = scan_outcome.chunks_to_embed.len();
    let mut records_to_upsert = scan_outcome.reused_records_to_upsert;
    if !records_to_upsert.is_empty() {
        store::upsert_records(records_to_upsert.clone()).await?;
    }
    records_to_upsert.reserve(embedded_count);

    for chunk_batch in scan_outcome.chunks_to_embed.chunks(batch_size_count) {
        if !force && !throttler::can_run_background_indexing() {
            return Err(SemanticError::Throttled(
                "Background indexing paused mid-run due to active internal tasks or host load".to_string(),
            ));
        }

        let batch_vec = chunk_batch.to_vec();
        let records = tokio::task::spawn_blocking(move || {
            chunker::vectorize_chunks(project_id, batch_vec)
        })
        .await
        .map_err(|e| SemanticError::EmbeddingFailed(format!("Tokio embedding worker panic: {}", e)))??;

        store::upsert_records(records.clone()).await?;
        records_to_upsert.extend(records);

        if batch_sleep_ms > 0 {
            tokio::time::sleep(Duration::from_millis(batch_sleep_ms)).await;
        }
    }

    // 10. Build and persist the Causal Knowledge Graph (Git hunks + anchors + exact refs + prose semantic links)
    let project_path_for_graph = scan_outcome.project_path.clone();
    let unchanged_for_graph = scan_outcome.unchanged_retained_records;
    let mut refs_by_id = scan_outcome.refs_by_id;
    let known_files = scan_outcome.known_project_files;

    let edges_count = tokio::task::spawn_blocking(move || -> Result<usize, SemanticError> {
        let (_, min_score_percent, _, _) =
            resolve_project_semantic_config(Some(project_path_for_graph.as_str()));

        let mut graph_nodes: Vec<GraphNodeMeta> =
            Vec::with_capacity(unchanged_for_graph.len() + records_to_upsert.len());

        for rec in unchanged_for_graph.into_iter().chain(records_to_upsert.into_iter()) {
            let refs = refs_by_id
                .remove(&rec.id)
                .unwrap_or_else(|| chunker::extract_text_references(&rec.content_preview));
            graph_nodes.push(GraphNodeMeta {
                id: rec.id,
                entity_type: rec.entity_type,
                title: rec.title,
                file_path: rec.file_path,
                line_start: rec.line_start,
                refs,
                vector: rec.vector,
            });
        }

        let hunks = crate::db::db_get_journal_hunks_for_project(project_id)?;
        let anchors = load_project_entity_anchors(project_id)?;
        let edges = build_project_knowledge_edges(
            project_id,
            &graph_nodes,
            &hunks,
            &anchors,
            &known_files,
            min_score_percent,
        );
        let count = edges.len();
        crate::db::db_replace_project_knowledge_edges(project_id, &edges)?;
        Ok(count)
    })
    .await
    .map_err(|e| SemanticError::Io(std::io::Error::other(format!("Graph build task failed: {}", e))))??;

    let duration_ms = start_time.elapsed().as_millis() as u64;
    active_guard.complete(duration_ms);

    crate::logger::log(
        "INFO",
        "semantic",
        &format!(
            "Indexed project '{}' ({} specs, {} symbols, {} entities, {} graph edges; embedded={} new/modified, reused={}, skipped_files={}, purged_orphans={}) in {} ms",
            scan_outcome.project_name,
            scan_outcome.indexed_specs_count,
            scan_outcome.indexed_symbols_count,
            scan_outcome.indexed_entities_count,
            edges_count,
            embedded_count,
            scan_outcome.reused_chunks_count,
            scan_outcome.skipped_files_count,
            scan_outcome.orphan_ids_to_delete.len(),
            duration_ms
        ),
    );

    // Notify Overview and active listeners of updated project knowledge state
    crate::db::notify_projects_updated();

    Ok(IndexingStats {
        project_id,
        indexed_symbols_count: scan_outcome.indexed_symbols_count,
        indexed_specs_count: scan_outcome.indexed_specs_count,
        indexed_entities_count: scan_outcome.indexed_entities_count,
        duration_ms,
    })
}

/// Executes a comprehensive semantic search and knowledge graph evaluation report for external CLI auditing.
pub async fn audit_semantic_knowledge(
    args: SemanticAuditArgs,
) -> Result<serde_json::Value, SemanticError> {
    let project_id = args.project_id;
    let reindex_stats = if args.reindex.unwrap_or(false) {
        Some(index_project_knowledge(project_id, true).await?)
    } else {
        None
    };

    let status = get_indexing_status(project_id).await?;
    let all_records = store::fetch_project_records(project_id).await?;
    let candidate_min_pct = args.candidate_min_score_percent.unwrap_or(60).clamp(1, 100);
    let candidate_min_ratio = (candidate_min_pct as f32) / 100.0;
    let active_min_pct = status.min_score_percent;

    // Compute live prose cross-domain similarity candidates (`journal <-> specs`) and SQLite edge audit
    let (edge_audit, semantic_candidates) = tokio::task::spawn_blocking(move || -> Result<(serde_json::Value, Vec<serde_json::Value>), SemanticError> {
        let edge_audit = crate::db::db_audit_project_knowledge_edges(project_id, 40)?;

        let records_vec: Vec<&VectorEntityRecord> = all_records.values().collect();
        let spec_recs: Vec<&VectorEntityRecord> = records_vec
            .iter()
            .copied()
            .filter(|r| r.entity_type == "specs" && !r.vector.is_empty())
            .collect();
        let journal_recs: Vec<&VectorEntityRecord> = records_vec
            .iter()
            .copied()
            .filter(|r| r.entity_type == "journal" && !r.vector.is_empty())
            .collect();

        let mut candidates: Vec<(f32, serde_json::Value)> = Vec::new();
        for j in &journal_recs {
            for spec in &spec_recs {
                let sim = cosine_similarity(&j.vector, &spec.vector);
                if sim >= candidate_min_ratio {
                    let score_pct = (sim * 1000.0).round() / 10.0;
                    candidates.push((
                        sim,
                        json!({
                            "scorePercent": score_pct,
                            "aboveActiveThreshold": (sim * 100.0) >= (active_min_pct as f32),
                            "journalTitle": j.title,
                            "specTitle": spec.title,
                            "specFile": spec.file_path,
                            "specLineStart": spec.line_start,
                        }),
                    ));
                }
            }
        }

        candidates.sort_by(|a, b| b.0.total_cmp(&a.0));
        let top_candidates: Vec<serde_json::Value> =
            candidates.into_iter().take(40).map(|(_, v)| v).collect();

        Ok((edge_audit, top_candidates))
    })
    .await
    .map_err(|e| SemanticError::Io(std::io::Error::other(format!("Audit worker failed: {}", e))))??;

    // Evaluate test queries if provided
    let limit = args.limit.unwrap_or(5).clamp(1, 25);
    let mut query_evaluations = Vec::new();
    if let Some(queries) = args.queries {
        for q in queries {
            if q.trim().is_empty() {
                continue;
            }
            let hits = search_knowledge(SearchKnowledgeArgs {
                query: q.clone(),
                scope: Some(KnowledgeScope::All),
                limit: Some(limit),
                project_id: Some(project_id),
                include_relations: Some(true),
                max_relations_per_result: Some(6),
                min_relation_score_percent: None,
            })
            .await?;

            let formatted_hits: Vec<serde_json::Value> = hits
                .into_iter()
                .map(|h| {
                    json!({
                        "id": h.id,
                        "entityType": h.entity_type,
                        "title": h.title,
                        "scorePercent": (h.score * 1000.0).round() / 10.0,
                        "filePath": h.file_path,
                        "lineStart": h.line_start,
                        "lineEnd": h.line_end,
                        "relationsCount": h.relations.len(),
                        "relations": h.relations,
                    })
                })
                .collect();

            query_evaluations.push(json!({
                "query": q,
                "resultsCount": formatted_hits.len(),
                "results": formatted_hits,
            }));
        }
    }

    Ok(json!({
        "status": status,
        "reindexStats": reindex_stats,
        "graphAudit": edge_audit,
        "crossDomainCandidates": {
            "candidateMinScorePercent": candidate_min_pct,
            "activeMinScorePercent": active_min_pct,
            "topPairs": semantic_candidates,
        },
        "queryEvaluations": query_evaluations,
    }))
}

/// Rebuilds `knowledge_edges` in SQLite from existing LanceDB prose vectors, `journal_file_hunks`, and SQLite anchors (< 300 ms).
pub async fn rebuild_project_knowledge_graph(project_id: i64) -> Result<usize, SemanticError> {
    let all_records = store::fetch_project_records(project_id).await?;
    let edges_count = tokio::task::spawn_blocking(move || -> Result<usize, SemanticError> {
        let project = usize::try_from(project_id)
            .ok()
            .and_then(|pid| crate::db::db_get_project_by_id(pid).ok().flatten());
        let project_path = project.as_ref().map(|p| p.path.as_str());
        let (_, min_score_percent, _, _) = resolve_project_semantic_config(project_path);

        let mut known_project_files = HashSet::new();
        if let Some(proj) = project.as_ref() {
            let root = Path::new(&proj.path);
            if root.exists() {
                let ignored = crate::config::resolve_effective_string_list(
                    Some(proj.path.as_str()),
                    None,
                    "semantic",
                    "ignored_patterns",
                );
                let max_depth = crate::config::resolve_effective_usize(None, None, "system", "max_dir_walk_depth_count").max(1);
                let max_nodes = crate::config::resolve_effective_usize(None, None, "system", "max_dir_walk_nodes_count").max(100);
                let mut visited = 0usize;
                collect_known_files_walk(root, root, &ignored, 0, max_depth, &mut visited, max_nodes, &mut known_project_files);
            }
        }

        let mut graph_nodes: Vec<GraphNodeMeta> = Vec::with_capacity(all_records.len());
        for rec in all_records.into_values() {
            if rec.entity_type == "code" {
                continue;
            }
            let refs = chunker::extract_text_references(&rec.content_preview);
            graph_nodes.push(GraphNodeMeta {
                id: rec.id,
                entity_type: rec.entity_type,
                title: rec.title,
                file_path: rec.file_path,
                line_start: rec.line_start,
                refs,
                vector: rec.vector,
            });
        }

        let hunks = crate::db::db_get_journal_hunks_for_project(project_id)?;
        let anchors = load_project_entity_anchors(project_id)?;
        let edges = build_project_knowledge_edges(
            project_id,
            &graph_nodes,
            &hunks,
            &anchors,
            &known_project_files,
            min_score_percent,
        );
        let count = edges.len();
        crate::db::db_replace_project_knowledge_edges(project_id, &edges)?;
        Ok(count)
    })
    .await
    .map_err(|e| SemanticError::Io(std::io::Error::other(format!("Rebuild graph worker failed: {}", e))))??;

    crate::db::notify_projects_updated();
    Ok(edges_count)
}

fn collect_known_files_walk(
    root: &Path,
    dir: &Path,
    ignored_patterns: &[String],
    depth_count: usize,
    max_depth_count: usize,
    visited_nodes_count: &mut usize,
    max_nodes_count: usize,
    out: &mut HashSet<String>,
) {
    if depth_count > max_depth_count || *visited_nodes_count >= max_nodes_count || !dir.exists() || !dir.is_dir() {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if *visited_nodes_count >= max_nodes_count {
            break;
        }
        *visited_nodes_count += 1;
        let path = entry.path();
        let rel = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().to_string();
        if is_path_ignored(&rel, ignored_patterns) {
            continue;
        }
        if path.is_dir() {
            collect_known_files_walk(root, &path, ignored_patterns, depth_count + 1, max_depth_count, visited_nodes_count, max_nodes_count, out);
        } else if path.is_file() {
            out.insert(rel);
        }
    }
}

/// Returns all indexed prose nodes, synthesized touched-file nodes, and weighted causal edges for `ProjectKnowledgeGraphTab.vue`.
pub async fn get_project_knowledge_graph(
    project_id: i64,
) -> Result<ProjectKnowledgeGraph, SemanticError> {
    let all_records = store::fetch_project_records(project_id).await?;
    tokio::task::spawn_blocking(move || -> Result<ProjectKnowledgeGraph, SemanticError> {
        let project_path = usize::try_from(project_id)
            .ok()
            .and_then(|pid| crate::db::db_get_project_by_id(pid).ok().flatten())
            .map(|p| p.path);
        let (_, min_score_percent, _, _) =
            resolve_project_semantic_config(project_path.as_deref());

        let mut nodes: Vec<KnowledgeGraphNode> = all_records
            .into_values()
            .filter(|r| r.entity_type != "code")
            .map(|r| KnowledgeGraphNode {
                id: r.id,
                project_id: r.project_id,
                entity_type: r.entity_type,
                entity_id: r.entity_id,
                symbol_name: r.symbol_name,
                title: r.title,
                content_preview: r.content_preview,
                file_path: r.file_path,
                line_start: r.line_start,
                line_end: r.line_end,
            })
            .collect();

        let mut existing_ids: HashSet<String> = nodes.iter().map(|n| n.id.clone()).collect();
        let edges = crate::db::db_get_project_knowledge_edges(project_id)?;

        // Synthesize lightweight `file` nodes for every workspace file referenced in causal edges
        for edge in &edges {
            if edge.target_entity_type == "file" && existing_ids.insert(edge.target_id.clone()) {
                let rel_path = edge
                    .target_file_path
                    .clone()
                    .unwrap_or_else(|| edge.target_title.clone());
                nodes.push(KnowledgeGraphNode {
                    id: edge.target_id.clone(),
                    project_id,
                    entity_type: "file".to_string(),
                    entity_id: rel_path.clone(),
                    symbol_name: String::new(),
                    title: rel_path.clone(),
                    content_preview: String::new(),
                    file_path: rel_path,
                    line_start: edge.target_line_start.unwrap_or(1),
                    line_end: edge.target_line_start.unwrap_or(1),
                });
            }
        }

        nodes.sort_by(|a, b| {
            a.entity_type
                .cmp(&b.entity_type)
                .then_with(|| a.file_path.cmp(&b.file_path))
                .then_with(|| a.line_start.cmp(&b.line_start))
        });

        Ok(ProjectKnowledgeGraph {
            project_id,
            min_score_percent,
            nodes,
            edges,
        })
    })
    .await
    .map_err(|e| SemanticError::Io(std::io::Error::other(format!("Fetch graph worker failed: {}", e))))?
}

/// Resolves deterministic line-level causal context (minutes/hunks, anchored debt/todos, and related specs)
/// for `(project_id, rel_path, line)` without running vector inference.
pub async fn get_file_line_insights(
    args: GetFileLineInsightsArgs,
) -> Result<FileLineInsights, SemanticError> {
    let rel_path = args.rel_path.trim().trim_start_matches("./").to_string();
    if rel_path.is_empty() {
        return Err(SemanticError::InvalidRequest("rel_path cannot be empty".to_string()));
    }
    let target_line = args.line.max(1);
    let project_id = args.project_id;

    let prose_count = store::count_project_records(project_id).await?;

    let (indexed, symbol_name, line_start, line_end, minutes, debt_and_todos, mut related_docs) =
        tokio::task::spawn_blocking(move || -> Result<_, SemanticError> {
            let symbol_count = crate::db::db_count_project_code_symbols(project_id)?;
            let indexed = prose_count > 0 || symbol_count > 0;

            let ext = Path::new(&rel_path)
                .extension()
                .and_then(|s| s.to_str())
                .map(|s| s.to_ascii_lowercase());
            let is_markdown = matches!(ext.as_deref(), Some("md") | Some("markdown"));

            let mut active_symbol: Option<String> = None;
            let mut active_start: Option<u32> = None;
            let mut active_end: Option<u32> = None;

            if is_markdown {
                if let Ok(pid) = usize::try_from(project_id) {
                    if let Ok(Some(proj)) = crate::db::db_get_project_by_id(pid) {
                        if let Ok(doc) = crate::kopeng::documents::read_project_document(proj.path, rel_path.clone()) {
                            let headings = chunker::extract_markdown_headings(&doc.content);
                            let mut best: Option<(u8, String, u32, u32)> = None;
                            for h in headings {
                                if target_line >= h.2 && target_line <= h.3 {
                                    let span = h.3.saturating_sub(h.2);
                                    let replace = match best {
                                        None => true,
                                        Some((_, _, bs, be)) => span <= be.saturating_sub(bs),
                                    };
                                    if replace {
                                        best = Some(h);
                                    }
                                }
                            }
                            if let Some((_, title, ls, le)) = best {
                                active_symbol = Some(title);
                                active_start = Some(ls);
                                active_end = Some(le);
                            }
                        }
                    }
                }
            } else if let Some(sym) = crate::db::db_get_file_enclosing_symbol(project_id, &rel_path, target_line)? {
                active_symbol = Some(sym.symbol_name);
                active_start = Some(sym.line_start);
                active_end = Some(sym.line_end);
            }

            let minutes = crate::db::db_get_causal_minutes_for_file_line(project_id, &rel_path, target_line, 12)?;
            let debt_and_todos = crate::db::db_get_anchored_items_for_file(project_id, &rel_path, target_line)?;
            let minute_entries: Vec<(i64, String)> = minutes
                .iter()
                .map(|m| (m.journal_id, m.title.clone()))
                .collect();
            let related_docs = crate::db::db_get_related_docs_for_file(project_id, &rel_path, &minute_entries, 10)?;

            Ok((
                indexed,
                active_symbol,
                active_start,
                active_end,
                minutes,
                debt_and_todos,
                related_docs,
            ))
        })
        .await
        .map_err(|e| SemanticError::Io(std::io::Error::other(format!("Line insights worker failed: {}", e))))??;

    let doc_ids: Vec<String> = related_docs.iter().map(|d| d.id.clone()).collect();
    let previews = store::fetch_chunk_previews_by_ids(project_id, &doc_ids).await?;
    for doc in &mut related_docs {
        if let Some((preview, lstart, lend)) = previews.get(&doc.id) {
            doc.content_preview = preview.clone();
            if doc.line_start.is_none() && *lstart > 0 {
                doc.line_start = Some(*lstart);
            }
            if *lend > 0 {
                doc.line_end = Some(*lend);
            }
        }
    }

    Ok(FileLineInsights {
        indexed,
        symbol_name,
        line_start,
        line_end,
        minutes,
        debt_and_todos,
        related_docs,
    })
}

/// Extracts structural outline symbols (Tree-sitter AST or Markdown headings) for a workspace file.
pub async fn get_file_symbols(
    args: GetFileSymbolsArgs,
) -> Result<Vec<FileSymbolItem>, SemanticError> {
    let rel_path = args.rel_path.trim().trim_start_matches("./").to_string();
    if rel_path.is_empty() {
        return Err(SemanticError::InvalidRequest("rel_path cannot be empty".to_string()));
    }
    let project_path = args.project_path;
    let inline_content = args.content;

    tokio::task::spawn_blocking(move || -> Result<Vec<FileSymbolItem>, SemanticError> {
        let source_content = match inline_content {
            Some(text) => text,
            None => {
                let doc = crate::kopeng::documents::read_project_document(project_path, rel_path.clone())
                    .map_err(|e| SemanticError::InvalidRequest(e.to_string()))?;
                doc.content
            }
        };

        if source_content.trim().is_empty() {
            return Ok(Vec::new());
        }

        let ext = Path::new(&rel_path)
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_ascii_lowercase());
        let is_markdown = matches!(ext.as_deref(), Some("md") | Some("markdown"));

        if is_markdown {
            let headings = chunker::extract_markdown_headings(&source_content);
            return Ok(headings
                .into_iter()
                .map(|(level, title, line_start, line_end)| FileSymbolItem {
                    symbol_name: title,
                    kind: "section".to_string(),
                    level,
                    line_start,
                    line_end,
                })
                .collect());
        }

        let chunks = chunker::chunk_code_file(0, &rel_path, &source_content);

        let mut symbols: Vec<FileSymbolItem> = Vec::new();
        let mut seen = HashSet::new();
        for c in chunks {
            if c.symbol_name.is_empty() || c.symbol_name.starts_with("chunk_") {
                continue;
            }
            let dedup_key = format!("{}::{}", c.symbol_name, c.line_start);
            if !seen.insert(dedup_key) {
                continue;
            }
            let level = (c.symbol_name.matches("::").count() as u8).saturating_add(1);
            symbols.push(FileSymbolItem {
                symbol_name: c.symbol_name,
                kind: "symbol".to_string(),
                level,
                line_start: c.line_start,
                line_end: c.line_end,
            });
        }

        symbols.sort_by(|a, b| {
            a.line_start
                .cmp(&b.line_start)
                .then_with(|| a.level.cmp(&b.level))
                .then_with(|| a.line_end.cmp(&b.line_end))
        });

        Ok(symbols)
    })
    .await
    .map_err(|e| SemanticError::Io(std::io::Error::other(format!("File symbols worker failed: {}", e))))?
}

/// Reads a bounded line slice from a sandboxed project document for inline insight inspection.
pub async fn get_file_line_range_content(
    args: GetFileLineRangeContentArgs,
) -> Result<FileLineRangeContent, SemanticError> {
    let rel_path = args.rel_path.trim().trim_start_matches("./").to_string();
    if rel_path.is_empty() {
        return Err(SemanticError::InvalidRequest("rel_path cannot be empty".to_string()));
    }
    let project_path = args.project_path;
    let requested_start = args.line_start.max(1);
    let requested_end = args.line_end;

    tokio::task::spawn_blocking(move || -> Result<FileLineRangeContent, SemanticError> {
        let doc = crate::kopeng::documents::read_project_document(project_path, rel_path)
            .map_err(|e| SemanticError::InvalidRequest(e.to_string()))?;
        let lines: Vec<&str> = doc.content.lines().collect();
        let total_lines = lines.len().max(1) as u32;

        let clamped_start = requested_start.min(total_lines);
        let default_end = clamped_start.saturating_add(79).min(total_lines);
        let raw_end = match requested_end {
            Some(end) if end >= clamped_start => end.min(total_lines),
            _ => default_end,
        };
        let clamped_end = raw_end.min(clamped_start.saturating_add(249)).min(total_lines);

        let start_idx = (clamped_start - 1) as usize;
        let end_idx = (clamped_end as usize).min(lines.len());
        let sliced = if start_idx < end_idx {
            lines[start_idx..end_idx].join("\n")
        } else {
            String::new()
        };

        Ok(FileLineRangeContent {
            rel_path: doc.rel_path,
            line_start: clamped_start,
            line_end: clamped_end,
            total_lines,
            lang: doc.doc_lang,
            doc_type: doc.doc_type,
            content: sliced,
        })
    })
    .await
    .map_err(|e| SemanticError::Io(std::io::Error::other(format!("Line range reader failed: {}", e))))?
}

/// Resolves candidate definition locations across deterministic AST symbols stored in SQLite (`project_code_symbols`).
pub async fn resolve_symbol_definition(
    args: ResolveSymbolDefinitionArgs,
) -> Result<Vec<SymbolDefinitionLocation>, SemanticError> {
    let query = args.symbol_name.trim().to_string();
    if query.is_empty() {
        return Err(SemanticError::InvalidRequest("symbol_name cannot be empty".to_string()));
    }

    let project_id = args.project_id;
    let current_rel = args
        .current_rel_path
        .map(|p| p.trim().trim_start_matches("./").to_string())
        .filter(|p| !p.is_empty());

    tokio::task::spawn_blocking(move || -> Result<Vec<SymbolDefinitionLocation>, SemanticError> {
        let indexed_code = crate::db::db_list_project_code_symbols(Some(project_id))?;
        let mut exact_matches = Vec::new();
        let mut partial_matches = Vec::new();
        let mut seen_keys = HashSet::new();
        let query_lower = query.to_ascii_lowercase();

        for sym in indexed_code {
            if sym.symbol_name.is_empty() || sym.symbol_name.starts_with("chunk_") {
                continue;
            }
            let short_name = &sym.short_name;

            let is_exact = sym.symbol_name == query || short_name == &query;
            let is_ci_exact = sym.symbol_name.eq_ignore_ascii_case(&query)
                || short_name.eq_ignore_ascii_case(&query);
            let is_partial = !is_exact
                && !is_ci_exact
                && (short_name.to_ascii_lowercase().contains(&query_lower)
                    || sym.symbol_name.to_ascii_lowercase().contains(&query_lower));

            if !is_exact && !is_ci_exact && !is_partial {
                continue;
            }

            let dedup_key = format!("{}::{}::{}", sym.file_path, sym.line_start, sym.symbol_name);
            if !seen_keys.insert(dedup_key) {
                continue;
            }

            let loc = SymbolDefinitionLocation {
                symbol_name: sym.symbol_name,
                file_path: sym.file_path,
                line_start: sym.line_start,
                line_end: sym.line_end,
                title: sym.title,
                exact_match: is_exact || is_ci_exact,
            };

            if loc.exact_match {
                exact_matches.push(loc);
            } else {
                partial_matches.push(loc);
            }
        }

        let sort_candidates = |items: &mut Vec<SymbolDefinitionLocation>| {
            items.sort_by(|a, b| {
                let a_local = current_rel.as_deref() == Some(a.file_path.as_str());
                let b_local = current_rel.as_deref() == Some(b.file_path.as_str());
                b_local
                    .cmp(&a_local)
                    .then_with(|| a.file_path.cmp(&b.file_path))
                    .then_with(|| a.line_start.cmp(&b.line_start))
            });
        };

        let mut results = if !exact_matches.is_empty() {
            sort_candidates(&mut exact_matches);
            exact_matches
        } else {
            sort_candidates(&mut partial_matches);
            partial_matches
        };

        results.truncate(20);
        Ok(results)
    })
    .await
    .map_err(|e| SemanticError::Io(std::io::Error::other(format!("Resolve symbol worker failed: {}", e))))?
}

/// Checks if a file's `file_sha256` matches all existing LanceDB records for `rel_path`.
fn try_skip_unchanged_file(
    rel_path: &str,
    file_sha256: &str,
    existing_by_file: &HashMap<String, Vec<&VectorEntityRecord>>,
    retained_ids: &mut HashSet<String>,
) -> Option<usize> {
    let records = existing_by_file.get(rel_path)?;
    if records.is_empty() || file_sha256.is_empty() {
        return None;
    }
    if rel_path.to_ascii_lowercase().ends_with(".astro")
        && records.iter().any(|r| r.symbol_name.starts_with("chunk_"))
    {
        return None;
    }
    if records.iter().all(|r| r.file_sha256 == file_sha256) {
        for r in records {
            retained_ids.insert(r.id.clone());
        }
        Some(records.len())
    } else {
        None
    }
}

/// Checks whether a relative path matches any ignored pattern from rula.toml.
pub(crate) fn is_path_ignored(rel_path: &str, ignored_patterns: &[String]) -> bool {
    let normalized = rel_path.replace('\\', "/");
    let segments: Vec<&str> = normalized.split('/').filter(|s| !s.is_empty()).collect();

    if segments.iter().any(|seg| seg.starts_with('.') && *seg != ".") {
        return true;
    }

    for pattern in ignored_patterns {
        let pat = pattern.trim().trim_matches('/');
        if pat.is_empty() {
            continue;
        }
        if pat.starts_with('*') {
            let suffix = &pat[1..];
            if normalized.ends_with(suffix) {
                return true;
            }
        } else if pat.ends_with('*') {
            let prefix = &pat[..pat.len() - 1];
            if segments.iter().any(|s| s.starts_with(prefix)) || normalized.starts_with(prefix) {
                return true;
            }
        } else if segments.contains(&pat) || normalized == pat || normalized.starts_with(&format!("{}/", pat)) {
            return true;
        }
    }
    false
}

fn retain_existing_records_for_file(
    rel: &str,
    existing_by_file: &HashMap<String, Vec<&VectorEntityRecord>>,
    retained_ids: &mut HashSet<String>,
) {
    if let Some(records) = existing_by_file.get(rel) {
        for rec in records {
            retained_ids.insert(rec.id.clone());
        }
    }
}

/// Recursively scans documentation directories (`.md`, `.txt`, `.pdf`) with `visited_files` deduplication and Level 1 `file_sha256` fast-skip.
#[allow(clippy::too_many_arguments)]
fn exceeds_max_file_size(path: &Path, max_file_size_bytes: u64) -> bool {
    if max_file_size_bytes == 0 {
        return false;
    }
    match std::fs::metadata(path) {
        Ok(m) => m.len() > max_file_size_bytes,
        Err(e) => {
            crate::logger::log(
                "WARN",
                "semantic",
                &format!("Cannot stat '{}' for size check: {e}", path.display()),
            );
            false
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn scan_docs_directory(
    project_id: i64,
    root: &Path,
    dir: &Path,
    ignored_patterns: &[String],
    max_file_size_bytes: u64,
    existing_by_file: &HashMap<String, Vec<&VectorEntityRecord>>,
    visited_files: &mut HashSet<String>,
    retained_ids: &mut HashSet<String>,
    out_chunks: &mut Vec<chunker::RawChunk>,
    out_count: &mut usize,
    skipped_files_count: &mut usize,
    depth_count: usize,
    max_depth_count: usize,
    visited_nodes_count: &mut usize,
    max_nodes_count: usize,
) {
    if depth_count > max_depth_count || *visited_nodes_count >= max_nodes_count || !dir.exists() || !dir.is_dir() {
        return;
    }
    match std::fs::read_dir(dir) {
        Ok(entries) => {
            for entry in entries.flatten() {
                if *visited_nodes_count >= max_nodes_count {
                    break;
                }
                *visited_nodes_count += 1;
                let path = entry.path();
                let rel = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().to_string();
                if is_path_ignored(&rel, ignored_patterns) {
                    continue;
                }
                if path.is_dir() {
                    scan_docs_directory(
                        project_id,
                        root,
                        &path,
                        ignored_patterns,
                        max_file_size_bytes,
                        existing_by_file,
                        visited_files,
                        retained_ids,
                        out_chunks,
                        out_count,
                        skipped_files_count,
                        depth_count + 1,
                        max_depth_count,
                        visited_nodes_count,
                        max_nodes_count,
                    );
                } else if path.is_file() {
                    let Some(ext) = path.extension().and_then(|s| s.to_str()).map(|s| s.to_ascii_lowercase()) else {
                        continue;
                    };
                    if !matches!(ext.as_str(), "md" | "txt" | "pdf") {
                        continue;
                    }
                    if !visited_files.insert(rel.clone()) {
                        continue;
                    }
                    if ext == "md" || ext == "txt" {
                        if exceeds_max_file_size(&path, max_file_size_bytes) {
                            *skipped_files_count += 1;
                            continue;
                        }
                        match std::fs::read_to_string(&path) {
                            Ok(content) => {
                                let file_sha256 = chunker::hash_text(&content);
                                if let Some(existing_count) = try_skip_unchanged_file(&rel, &file_sha256, existing_by_file, retained_ids) {
                                    *out_count += existing_count;
                                    *skipped_files_count += 1;
                                    continue;
                                }
                                let spec_chunks = chunker::chunk_markdown_spec_with_hash(project_id, &rel, &content, &file_sha256);
                                *out_count += spec_chunks.len();
                                out_chunks.extend(spec_chunks);
                            }
                            Err(e) => {
                                retain_existing_records_for_file(&rel, existing_by_file, retained_ids);
                                crate::logger::log("WARN", "semantic", &format!("Failed to read doc '{}': {}", rel, e));
                            }
                        }
                    } else if ext == "pdf" {
                        if exceeds_max_file_size(&path, max_file_size_bytes) {
                            *skipped_files_count += 1;
                            continue;
                        }
                        match std::fs::read(&path) {
                            Ok(raw_bytes) => {
                                let mut hasher = Sha256::new();
                                hasher.update(&raw_bytes);
                                let file_sha256 = format!("{:x}", hasher.finalize());
                                if let Some(existing_count) = try_skip_unchanged_file(&rel, &file_sha256, existing_by_file, retained_ids) {
                                    *out_count += existing_count;
                                    *skipped_files_count += 1;
                                    continue;
                                }
                                match chunker::chunk_pdf_bytes(project_id, &rel, &raw_bytes, &file_sha256) {
                                    Ok(pdf_chunks) => {
                                        *out_count += pdf_chunks.len();
                                        out_chunks.extend(pdf_chunks);
                                    }
                                    Err(e) => {
                                        retain_existing_records_for_file(&rel, existing_by_file, retained_ids);
                                        crate::logger::log("WARN", "semantic", &format!("Failed to extract PDF '{}': {}", rel, e));
                                    }
                                }
                            }
                            Err(e) => {
                                retain_existing_records_for_file(&rel, existing_by_file, retained_ids);
                                crate::logger::log("WARN", "semantic", &format!("Failed to read PDF '{}': {}", rel, e));
                            }
                        }
                    }
                }
            }
        }
        Err(e) => crate::logger::log("WARN", "semantic", &format!("Failed to read docs dir '{}': {}", dir.display(), e)),
    }
}

/// Recursively scans workspace source files strictly for deterministic Tree-sitter AST symbols
/// stored in SQLite (`project_code_symbols`, zero BGE-M3 inference) while collecting `known_project_files`.
#[allow(clippy::too_many_arguments)]
fn scan_code_symbols_directory(
    project_id: i64,
    root: &Path,
    dir: &Path,
    scanned_doc_dirs: &[PathBuf],
    ignored_patterns: &[String],
    included_extensions: &[String],
    max_file_size_bytes: u64,
    existing_symbol_hashes: &HashMap<String, String>,
    visited_code_files: &mut HashSet<String>,
    known_project_files: &mut HashSet<String>,
    changed_files: &mut Vec<String>,
    out_symbols: &mut Vec<crate::db::CodeSymbolRecord>,
    depth_count: usize,
    max_depth_count: usize,
    visited_nodes_count: &mut usize,
    max_nodes_count: usize,
) {
    if depth_count > max_depth_count || *visited_nodes_count >= max_nodes_count || !dir.exists() || !dir.is_dir() {
        return;
    }

    match std::fs::read_dir(dir) {
        Ok(entries) => {
            for entry in entries.flatten() {
                if *visited_nodes_count >= max_nodes_count {
                    break;
                }
                *visited_nodes_count += 1;
                let path = entry.path();
                let rel = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().to_string();
                if is_path_ignored(&rel, ignored_patterns) {
                    continue;
                }
                if path.is_dir() {
                    if scanned_doc_dirs.iter().any(|d| d == &path) {
                        continue;
                    }
                    scan_code_symbols_directory(
                        project_id,
                        root,
                        &path,
                        scanned_doc_dirs,
                        ignored_patterns,
                        included_extensions,
                        max_file_size_bytes,
                        existing_symbol_hashes,
                        visited_code_files,
                        known_project_files,
                        changed_files,
                        out_symbols,
                        depth_count + 1,
                        max_depth_count,
                        visited_nodes_count,
                        max_nodes_count,
                    );
                } else if path.is_file() {
                    known_project_files.insert(rel.clone());
                    let Some(ext) = path.extension().and_then(|s| s.to_str()) else {
                        continue;
                    };
                    let ext_lower = ext.to_ascii_lowercase();
                    if matches!(ext_lower.as_str(), "md" | "txt" | "pdf" | "html" | "htm" | "toml" | "yaml" | "yml" | "json" | "lock") {
                        continue;
                    }
                    if !included_extensions.iter().any(|allowed| allowed.eq_ignore_ascii_case(&ext_lower)) {
                        continue;
                    }
                    if !visited_code_files.insert(rel.clone()) {
                        continue;
                    }
                    if exceeds_max_file_size(&path, max_file_size_bytes) {
                        continue;
                    }
                    match std::fs::read_to_string(&path) {
                        Ok(content) => {
                            let file_sha256 = chunker::hash_text(&content);
                            if existing_symbol_hashes.get(&rel).is_some_and(|h| h == &file_sha256) {
                                continue;
                            }
                            changed_files.push(rel.clone());
                            let chunks = chunker::chunk_code_file_with_hash(project_id, &rel, &content, &file_sha256);
                            for c in chunks {
                                if c.symbol_name.is_empty() || c.symbol_name.starts_with("chunk_") {
                                    continue;
                                }
                                let short_name = c
                                    .symbol_name
                                    .rsplit("::")
                                    .next()
                                    .unwrap_or(c.symbol_name.as_str())
                                    .to_string();
                                out_symbols.push(crate::db::CodeSymbolRecord {
                                    project_id,
                                    file_path: rel.clone(),
                                    file_sha256: file_sha256.clone(),
                                    symbol_name: c.symbol_name,
                                    short_name,
                                    title: c.title,
                                    line_start: c.line_start,
                                    line_end: c.line_end,
                                });
                            }
                        }
                        Err(e) => {
                            crate::logger::log("WARN", "semantic", &format!("Failed to read code file '{}': {}", rel, e));
                        }
                    }
                }
            }
        }
        Err(e) => crate::logger::log("WARN", "semantic", &format!("Failed to read code dir '{}': {}", dir.display(), e)),
    }
}
