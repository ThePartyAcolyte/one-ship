//! Data types and transfer objects for semantic search and LanceDB vector records.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Scope filter for transversal semantic knowledge discovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum KnowledgeScope {
    All,
    Code,
    Specs,
    Journal,
    Tasks,
    TechDebt,
}

impl KnowledgeScope {
    /// Returns the string representation used in LanceDB scalar metadata filters.
    pub fn as_filter_str(&self) -> Option<&'static str> {
        match self {
            Self::All => None,
            Self::Code => Some("code"),
            Self::Specs => Some("specs"),
            Self::Journal => Some("journal"),
            Self::Tasks => Some("tasks"),
            Self::TechDebt => Some("tech_debt"),
        }
    }
}

/// Arguments for the `search_knowledge` daemon command and MCP tool.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SearchKnowledgeArgs {
    /// Natural language or technical query describing the target knowledge.
    pub query: String,
    /// Optional domain scope filter to narrow down the search space.
    #[serde(default)]
    pub scope: Option<KnowledgeScope>,
    /// Optional limit on the number of nearest neighbors returned (default: 5).
    #[serde(default)]
    pub limit: Option<usize>,
    /// Optional project ID scoping. If omitted, searches across all indexed projects.
    #[serde(default)]
    #[ts(type = "number | null")]
    pub project_id: Option<i64>,
    /// Optional flag to expand 1-hop knowledge graph relations (`calls`, `imports`, `governs`, `affects`).
    #[serde(default)]
    pub include_relations: Option<bool>,
    /// Optional cap on 1-hop relations returned per search hit (default: 6).
    #[serde(default)]
    pub max_relations_per_result: Option<usize>,
    /// Optional minimum relation weight percentage (1-100) filter applied at query time.
    #[serde(default)]
    pub min_relation_score_percent: Option<usize>,
}

/// A 1-hop structural or transversal relationship attached to a semantic search result.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct KnowledgeRelation {
    /// Relation direction relative to the seed entity (`"outgoing"` or `"incoming"`).
    pub direction: String,
    /// Relationship kind (`"calls"`, `"imports"`, `"contains"`, `"governs"`, `"affects"`).
    pub edge_type: String,
    /// Unique compound identifier of the related entity (`<type>::<project_id>::<entity_id>`).
    pub peer_id: String,
    /// Entity category of the related node (`"code"`, `"specs"`, `"journal"`, `"tasks"`, `"tech_debt"`).
    pub peer_entity_type: String,
    /// Human-readable title or symbol name of the related node.
    pub peer_title: String,
    /// Relative file path of the related node, if applicable.
    pub peer_file_path: Option<String>,
    /// Starting 1-based line offset of the related node, if applicable.
    pub peer_line_start: Option<u32>,
    /// Edge weight / similarity score in range [0.0, 1.0].
    pub weight: f32,
}

/// A matched semantic entity returned by LanceDB vector search.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct KnowledgeSearchResult {
    /// Unique compound identifier in LanceDB (`<type>::<project_id>::<entity_id>`).
    pub id: String,
    /// Associated project ID.
    #[ts(type = "number")]
    pub project_id: i64,
    /// Entity category (`code`, `specs`, `journal`, `tasks`, `tech_debt`).
    pub entity_type: String,
    /// Specific entity key or symbol identifier.
    pub entity_id: String,
    /// Human-readable title or symbol name.
    pub title: String,
    /// Preview text snippet of the matched content.
    pub content_preview: String,
    /// Relative or absolute file path if the match corresponds to a filesystem document.
    pub file_path: Option<String>,
    /// Verified starting line offset in the file (1-based, 0 if not applicable).
    pub line_start: Option<u32>,
    /// Verified ending line offset in the file (1-based, 0 if not applicable).
    pub line_end: Option<u32>,
    /// Relevance score in range [0.0, 1.0], derived from vector cosine similarity.
    pub score: f32,
    /// 1-hop structural and transversal relations from the project knowledge graph.
    #[serde(default)]
    pub relations: Vec<KnowledgeRelation>,
}

/// Arguments for triggering manual project re-indexing.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct IndexProjectArgs {
    #[ts(type = "number")]
    pub project_id: i64,
    /// Explicit override for manual user requests to bypass idle background throttling.
    #[serde(default)]
    pub force: Option<bool>,
}

/// Outcome metrics emitted after project vector indexing completes.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct IndexingStats {
    #[ts(type = "number")]
    pub project_id: i64,
    pub indexed_symbols_count: usize,
    pub indexed_specs_count: usize,
    pub indexed_entities_count: usize,
    #[ts(type = "number")]
    pub duration_ms: u64,
}

/// Current vector index status and metrics for a project.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProjectIndexingStatus {
    #[ts(type = "number")]
    pub project_id: i64,
    pub is_indexing: bool,
    pub total_records_count: usize,
    pub code_chunks_count: usize,
    pub spec_chunks_count: usize,
    pub entity_chunks_count: usize,
    pub graph_edges_count: usize,
    pub auto_index_enabled: bool,
    pub min_score_percent: usize,
    #[ts(type = "number | null")]
    pub last_duration_ms: Option<u64>,
}

/// Request payload for the local `/api/v1/audit/semantic` evaluation endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticAuditArgs {
    pub project_id: i64,
    #[serde(default)]
    pub reindex: Option<bool>,
    #[serde(default)]
    pub queries: Option<Vec<String>>,
    #[serde(default)]
    pub limit: Option<usize>,
    #[serde(default)]
    pub candidate_min_score_percent: Option<usize>,
}

/// A node in the interactive project knowledge graph visualizer (`ProjectKnowledgeGraphTab.vue`).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct KnowledgeGraphNode {
    pub id: String,
    #[ts(type = "number")]
    pub project_id: i64,
    pub entity_type: String,
    pub entity_id: String,
    pub symbol_name: String,
    pub title: String,
    pub content_preview: String,
    pub file_path: String,
    pub line_start: u32,
    pub line_end: u32,
}

/// A weighted edge in the interactive project knowledge graph visualizer (`ProjectKnowledgeGraphTab.vue`).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct KnowledgeGraphEdge {
    pub source_id: String,
    pub source_entity_type: String,
    pub source_title: String,
    pub source_file_path: Option<String>,
    pub source_line_start: Option<u32>,
    pub target_id: String,
    pub target_entity_type: String,
    pub target_title: String,
    pub target_file_path: Option<String>,
    pub target_line_start: Option<u32>,
    pub edge_type: String,
    pub origin: String,
    pub weight: f32,
}

/// Complete project knowledge graph payload returned by `get_project_knowledge_graph`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProjectKnowledgeGraph {
    #[ts(type = "number")]
    pub project_id: i64,
    pub min_score_percent: usize,
    pub nodes: Vec<KnowledgeGraphNode>,
    pub edges: Vec<KnowledgeGraphEdge>,
}

/// Atomic vector record ready for batch insertion into LanceDB.
#[derive(Debug, Clone)]
pub struct VectorEntityRecord {
    pub id: String,
    pub project_id: i64,
    pub entity_type: String,
    pub entity_id: String,
    pub symbol_name: String,
    pub title: String,
    pub content_preview: String,
    pub file_path: String,
    pub file_sha256: String,
    pub content_sha256: String,
    pub line_start: u32,
    pub line_end: u32,
    pub vector: Vec<f32>,
}

/// Relational knowledge graph edge persisted in SQLite (`knowledge_edges`).
#[derive(Debug, Clone)]
pub struct KnowledgeEdgeRecord {
    pub project_id: i64,
    pub source_id: String,
    pub source_entity_type: String,
    pub source_title: String,
    pub source_file_path: Option<String>,
    pub source_line_start: Option<u32>,
    pub target_id: String,
    pub target_entity_type: String,
    pub target_title: String,
    pub target_file_path: Option<String>,
    pub target_line_start: Option<u32>,
    pub edge_type: String,
    pub origin: String,
    pub weight: f64,
}

/// Request payload for deterministic line-level causal knowledge insights in the explorer editor.
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GetFileLineInsightsArgs {
    #[ts(type = "number")]
    pub project_id: i64,
    pub rel_path: String,
    pub line: u32,
}

/// A session minute causally linked to a file or overlapping line range via Git hunks or `files_touched`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FileCausalMinute {
    #[ts(type = "number")]
    pub journal_id: i64,
    pub agent_id: String,
    pub title: String,
    pub summary: String,
    pub decisions: Option<String>,
    pub commit_hash: Option<String>,
    pub snapshot_ref: Option<String>,
    pub line_start: u32,
    pub line_end: u32,
    pub change_kind: String,
    pub source_kind: String,
    pub exact_line_match: bool,
    #[ts(type = "number")]
    pub created_at: i64,
}

/// A technical debt or TODO item anchored or explicitly referencing a file/line.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FileAnchoredItem {
    pub entity_type: String,
    #[ts(type = "number")]
    pub entity_id: i64,
    pub title: String,
    pub details: String,
    pub status: String,
    pub severity_or_priority: String,
    pub file_path: Option<String>,
    pub line_start: Option<u32>,
    pub line_end: Option<u32>,
    pub exact_line_match: bool,
    pub link_reason: String,
}

/// A specification or documentation section connected via causal minutes or explicit file/symbol references.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FileRelatedDoc {
    pub id: String,
    pub title: String,
    pub content_preview: String,
    pub file_path: String,
    pub line_start: Option<u32>,
    pub line_end: Option<u32>,
    pub weight: f32,
    pub link_reason: String,
    pub via_minute_title: Option<String>,
}

/// Deterministic line-level causal context (minutes, anchored debt/todos, and related specs) for the active cursor.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FileLineInsights {
    pub indexed: bool,
    pub symbol_name: Option<String>,
    pub line_start: Option<u32>,
    pub line_end: Option<u32>,
    pub minutes: Vec<FileCausalMinute>,
    pub debt_and_todos: Vec<FileAnchoredItem>,
    pub related_docs: Vec<FileRelatedDoc>,
}

/// Request payload for extracting Tree-sitter / Markdown outline symbols for a file.
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GetFileSymbolsArgs {
    pub project_path: String,
    pub rel_path: String,
    pub content: Option<String>,
}

/// A structural symbol or heading entry for the editor outline navigator.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FileSymbolItem {
    pub symbol_name: String,
    pub kind: String,
    pub level: u8,
    pub line_start: u32,
    pub line_end: u32,
}

/// Request payload for reading a bounded line range of a project document for inline insight inspection.
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GetFileLineRangeContentArgs {
    pub project_path: String,
    pub rel_path: String,
    pub line_start: u32,
    pub line_end: Option<u32>,
}

/// Bounded file slice returned for inline preview inside the Knowledge Graph Insights drawer.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FileLineRangeContent {
    pub rel_path: String,
    pub line_start: u32,
    pub line_end: u32,
    pub total_lines: u32,
    pub lang: Option<String>,
    pub doc_type: String,
    pub content: String,
}

/// Request payload for deterministic Tree-sitter symbol definition lookup across a project.
#[derive(Debug, Clone, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ResolveSymbolDefinitionArgs {
    #[ts(type = "number")]
    pub project_id: i64,
    pub symbol_name: String,
    pub current_rel_path: Option<String>,
}

/// A candidate definition location resolved from indexed AST symbols.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SymbolDefinitionLocation {
    pub symbol_name: String,
    pub file_path: String,
    pub line_start: u32,
    pub line_end: u32,
    pub title: String,
    pub exact_match: bool,
}
