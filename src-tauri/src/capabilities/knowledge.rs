//! Hybrid Knowledge Search, Deterministic AST Symbols & Causal Graph Provider (`domain = "knowledge"`, Spec 22 / Spec 40).
//!
//! Exposes the refactored `semantic` subsystem cleanly for both the Global Voice Controller
//! and managed MCP coding agents:
//! - `search_knowledge`: BGE-M3 prose search (`specs`, `journal`, `tasks`, `tech_debt`) + deterministic
//!   Tree-sitter AST lookup (`code`) + 1-hop causal/transversal graph relations.
//! - `resolve_symbol_definition`: Deterministic SQLite AST symbol lookup (`project_code_symbols`, zero ONNX).
//! - `get_file_symbols`: Tree-sitter / Markdown structural outline for a project file.
//! - `get_file_line_insights`: Causal minutes (`journal_file_hunks` + `files_touched`), anchored debt/TODOs,
//!   and linked specs for `(rel_path, line)`.
//! - `get_indexing_status`, `index_project`, `rebuild_graph`: Project index inspection and refresh.

use std::borrow::Cow;
use std::path::Path;
use serde_json::{json, Value};

use crate::semantic::{
    get_file_line_insights, get_file_symbols, get_indexing_status, index_project_knowledge,
    rebuild_project_knowledge_graph, resolve_symbol_definition, search_knowledge,
    GetFileLineInsightsArgs, GetFileSymbolsArgs, KnowledgeScope, ResolveSymbolDefinitionArgs,
    SearchKnowledgeArgs,
};

use super::{
    paginate_items, parse_pagination, require_non_empty_str, BoxFuture, CapabilityContext,
    CapabilityDef, CapabilityError, CapabilityProvider, CapabilityRisk, CapabilityTarget,
    CapabilityVisibility,
};

pub static PROVIDER: CapabilityProvider = CapabilityProvider {
    domain_id: "knowledge",
    label: "Knowledge Graph & Semantic Search",
    required_capability: None,
    linked_surface_path: Some("knowledge"),
    static_actions,
    dynamic_actions: |_| Vec::new(),
    execute,
};

fn cap(
    id: &'static str,
    mcp_name: Option<&'static str>,
    description: &'static str,
    parameters_schema: Value,
    risk: CapabilityRisk,
) -> CapabilityDef {
    CapabilityDef {
        id: Cow::Borrowed(id),
        mcp_name: mcp_name.map(Cow::Borrowed),
        description: Cow::Borrowed(description),
        parameters_schema,
        visibility: CapabilityVisibility::Shared,
        risk,
        target: CapabilityTarget::Backend,
        voice_max_items: None,
    }
}

fn static_actions() -> Vec<CapabilityDef> {
    vec![
        cap(
            "search_knowledge",
            Some("search_knowledge"),
            "Runs hybrid knowledge search: BGE-M3 semantic search across prose (specs, journal, tasks, tech_debt) combined with deterministic Tree-sitter AST lookup for code and 1-hop causal graph relations.",
            json!({
                "type": "OBJECT",
                "properties": {
                    "query": { "type": "STRING", "description": "Natural-language question, concept, or code symbol name" },
                    "scope": {
                        "type": "STRING",
                        "enum": ["all", "code", "specs", "journal", "tasks", "tech_debt"],
                        "description": "Subset filter (default: all; 'code' queries deterministic AST symbols)"
                    },
                    "limit": { "type": "INTEGER", "description": "Max hits to return (default: 5, semantic backend caps at 50)" },
                    "include_relations": { "type": "BOOLEAN", "description": "Expand 1-hop causal/transversal relations per hit (default: true)" },
                    "max_relations_per_result": { "type": "INTEGER", "description": "Max 1-hop relations returned per hit (1-50)" },
                    "min_relation_score_percent": { "type": "INTEGER", "description": "Minimum relation weight percentage filter (1-100)" }
                },
                "required": ["query"]
            }),
            CapabilityRisk::ReadOnly,
        ),
        cap(
            "resolve_symbol_definition",
            None,
            "Looks up deterministic Tree-sitter AST symbol definitions in SQLite (project_code_symbols, zero ONNX) with verified file paths and line ranges (supports offset/limit pagination).",
            json!({
                "type": "OBJECT",
                "properties": {
                    "symbol_name": { "type": "STRING", "description": "Function, struct, class, or method symbol name to locate" },
                    "current_rel_path": { "type": "STRING", "description": "Optional current relative file path to prioritize same-file definitions" },
                    "offset": { "type": "INTEGER", "description": "Pagination start index (default 0)" },
                    "limit": { "type": "INTEGER", "description": "Optional page size (omit for all)" }
                },
                "required": ["symbol_name"]
            }),
            CapabilityRisk::ReadOnly,
        ),
        cap(
            "get_file_symbols",
            None,
            "Extracts structural Tree-sitter AST symbols or Markdown headings with nesting levels and line ranges for a project file (supports offset/limit pagination).",
            json!({
                "type": "OBJECT",
                "properties": {
                    "rel_path": { "type": "STRING", "description": "Project-relative file path to inspect" },
                    "offset": { "type": "INTEGER", "description": "Pagination start index (default 0)" },
                    "limit": { "type": "INTEGER", "description": "Optional page size (omit for all)" }
                },
                "required": ["rel_path"]
            }),
            CapabilityRisk::ReadOnly,
        ),
        cap(
            "get_file_line_insights",
            Some("get_file_line_insights"),
            "Resolves deterministic causal context for a file line: enclosing symbol, session minutes from Git hunks/files_touched, anchored debt/TODOs, and linked specifications.",
            json!({
                "type": "OBJECT",
                "properties": {
                    "rel_path": { "type": "STRING", "description": "Project-relative file path to inspect" },
                    "line": { "type": "INTEGER", "description": "1-based cursor/target line number (default: 1)" }
                },
                "required": ["rel_path"]
            }),
            CapabilityRisk::ReadOnly,
        ),
        cap(
            "get_indexing_status",
            None,
            "Returns project knowledge index metrics (AST symbol count, prose spec/entity chunks, causal graph edges count, active indexing flag, auto-index state).",
            json!({
                "type": "OBJECT",
                "properties": {}
            }),
            CapabilityRisk::ReadOnly,
        ),
        cap(
            "index_project",
            None,
            "Triggers incremental prose vectorization, Tree-sitter AST symbol sync, Git hunk backfill, and causal graph edge rebuild for the project.",
            json!({
                "type": "OBJECT",
                "properties": {
                    "force": { "type": "BOOLEAN", "description": "Bypass background resource throttling for manual indexing (default: true)" }
                }
            }),
            CapabilityRisk::Mutating,
        ),
        cap(
            "rebuild_graph",
            None,
            "Recomputes causal and prose semantic graph edges (<300 ms) from stored vectors, journal hunks, and SQLite anchors without re-running BGE-M3 inference.",
            json!({
                "type": "OBJECT",
                "properties": {}
            }),
            CapabilityRisk::Mutating,
        ),
    ]
}

fn parse_scope(params: &Value) -> Result<KnowledgeScope, CapabilityError> {
    match params.get("scope") {
        None | Some(Value::Null) => Ok(KnowledgeScope::All),
        Some(v) => match v.as_str() {
            Some("all") => Ok(KnowledgeScope::All),
            Some("code") => Ok(KnowledgeScope::Code),
            Some("specs") => Ok(KnowledgeScope::Specs),
            Some("journal") => Ok(KnowledgeScope::Journal),
            Some("tasks") => Ok(KnowledgeScope::Tasks),
            Some("tech_debt") => Ok(KnowledgeScope::TechDebt),
            Some(other) => Err(CapabilityError::InvalidParams(format!(
                "Invalid knowledge scope '{other}'; expected all | code | specs | journal | tasks | tech_debt"
            ))),
            None => Err(CapabilityError::InvalidParams(
                "'scope' must be a string".to_string(),
            )),
        },
    }
}

fn parse_optional_usize_min(
    params: &Value,
    key: &str,
    min_value: u64,
    max_value: Option<u64>,
) -> Result<Option<usize>, CapabilityError> {
    match params.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => match v.as_u64() {
            Some(n) if n >= min_value && max_value.is_none_or(|max| n <= max) => {
                usize::try_from(n).map(Some).map_err(|_| {
                    CapabilityError::InvalidParams(format!("'{key}' is out of usize range"))
                })
            }
            _ => match max_value {
                Some(max) => Err(CapabilityError::InvalidParams(format!(
                    "'{key}' must be an integer between {min_value} and {max}"
                ))),
                None => Err(CapabilityError::InvalidParams(format!(
                    "'{key}' must be an integer >= {min_value}"
                ))),
            },
        },
    }
}

fn parse_optional_bool(params: &Value, key: &str) -> Result<Option<bool>, CapabilityError> {
    match params.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => v
            .as_bool()
            .map(Some)
            .ok_or_else(|| CapabilityError::InvalidParams(format!("'{key}' must be a boolean"))),
    }
}

fn parse_optional_u32_positive(
    params: &Value,
    key: &str,
    default: u32,
) -> Result<u32, CapabilityError> {
    match params.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(v) => match v.as_u64() {
            Some(n) if n >= 1 => u32::try_from(n).map_err(|_| {
                CapabilityError::InvalidParams(format!("'{key}' exceeds maximum 32-bit line index"))
            }),
            _ => Err(CapabilityError::InvalidParams(format!(
                "'{key}' must be a positive integer"
            ))),
        },
    }
}

fn validate_relative_path(params: &Value, key: &str) -> Result<String, CapabilityError> {
    let raw = require_non_empty_str(params, key)?;
    let trimmed = raw.trim().trim_start_matches("./").to_string();
    if trimmed.is_empty() {
        return Err(CapabilityError::InvalidParams(format!(
            "'{key}' cannot be empty"
        )));
    }
    if Path::new(&trimmed).is_absolute()
        || trimmed.split(['/', '\\']).any(|seg| seg == "..")
    {
        return Err(CapabilityError::InvalidParams(format!(
            "'{key}' must be a relative path inside the project root"
        )));
    }
    Ok(trimmed)
}

fn parse_optional_string(params: &Value, key: &str) -> Result<Option<String>, CapabilityError> {
    match params.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => {
            let s = v.as_str().ok_or_else(|| {
                CapabilityError::InvalidParams(format!("'{key}' must be a string"))
            })?;
            let trimmed = s.trim();
            if trimmed.is_empty() {
                Ok(None)
            } else {
                Ok(Some(trimmed.to_string()))
            }
        }
    }
}

fn execute<'a>(
    ctx: CapabilityContext,
    action_id: &'a str,
    params: Value,
) -> BoxFuture<'a, Result<Value, CapabilityError>> {
    Box::pin(async move {
        let project_id = ctx.project_id();
        let project_path = ctx.project.path.clone();

        match action_id {
            "search_knowledge" => {
                let query = require_non_empty_str(&params, "query")?;
                let scope = parse_scope(&params)?;
                let limit = parse_optional_usize_min(&params, "limit", 1, None)?;
                let include_relations = match parse_optional_bool(&params, "include_relations")? {
                    Some(b) => Some(b),
                    None => Some(true),
                };
                let max_relations_per_result =
                    parse_optional_usize_min(&params, "max_relations_per_result", 1, Some(50))?;
                let min_relation_score_percent =
                    parse_optional_usize_min(&params, "min_relation_score_percent", 1, Some(100))?;

                let hits = search_knowledge(SearchKnowledgeArgs {
                    query,
                    scope: Some(scope),
                    limit,
                    project_id: Some(project_id),
                    include_relations,
                    max_relations_per_result,
                    min_relation_score_percent,
                })
                .await?;

                Ok(json!({
                    "results": hits,
                    "count": hits.len(),
                }))
            }
            "resolve_symbol_definition" => {
                let symbol_name = require_non_empty_str(&params, "symbol_name")?;
                let current_rel_path = parse_optional_string(&params, "current_rel_path")?;
                let (_, _) = parse_pagination(&params)?;
                let definitions = resolve_symbol_definition(ResolveSymbolDefinitionArgs {
                    project_id,
                    symbol_name,
                    current_rel_path,
                })
                .await?;
                paginate_items(definitions, "definitions", &params)
            }
            "get_file_symbols" => {
                let rel_path = validate_relative_path(&params, "rel_path")?;
                let (_, _) = parse_pagination(&params)?;
                let symbols = get_file_symbols(GetFileSymbolsArgs {
                    project_path,
                    rel_path,
                    content: None,
                })
                .await?;
                paginate_items(symbols, "symbols", &params)
            }
            "get_file_line_insights" => {
                let rel_path = validate_relative_path(&params, "rel_path")?;
                let line = parse_optional_u32_positive(&params, "line", 1)?;
                let insights = get_file_line_insights(GetFileLineInsightsArgs {
                    project_id,
                    rel_path,
                    line,
                })
                .await?;
                serde_json::to_value(insights).map_err(|e| CapabilityError::Task(e.to_string()))
            }
            "get_indexing_status" => {
                let status = get_indexing_status(project_id).await?;
                serde_json::to_value(status).map_err(|e| CapabilityError::Task(e.to_string()))
            }
            "index_project" => {
                let force = parse_optional_bool(&params, "force")?;
                let force_flag = match force {
                    Some(v) => v,
                    None => true,
                };
                let stats = index_project_knowledge(project_id, force_flag).await?;
                serde_json::to_value(stats).map_err(|e| CapabilityError::Task(e.to_string()))
            }
            "rebuild_graph" => {
                let graph_edges_count = rebuild_project_knowledge_graph(project_id).await?;
                Ok(json!({
                    "ok": true,
                    "project_id": project_id,
                    "graph_edges_count": graph_edges_count,
                }))
            }
            other => Err(CapabilityError::UnknownAction {
                domain: "knowledge".to_string(),
                action_id: other.to_string(),
            }),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_and_numeric_parsers_reject_invalid_inputs() {
        assert_eq!(parse_scope(&json!({})).unwrap(), KnowledgeScope::All);
        assert_eq!(
            parse_scope(&json!({ "scope": "code" })).unwrap(),
            KnowledgeScope::Code
        );
        assert_eq!(
            parse_scope(&json!({ "scope": "tech_debt" })).unwrap(),
            KnowledgeScope::TechDebt
        );
        assert!(parse_scope(&json!({ "scope": "invalid_scope" })).is_err());
        assert!(parse_scope(&json!({ "scope": 123 })).is_err());

        assert_eq!(
            parse_optional_usize_min(&json!({ "min_relation_score_percent": 75 }), "min_relation_score_percent", 1, Some(100)).unwrap(),
            Some(75)
        );
        assert!(parse_optional_usize_min(&json!({ "min_relation_score_percent": 0 }), "min_relation_score_percent", 1, Some(100)).is_err());
        assert!(parse_optional_usize_min(&json!({ "min_relation_score_percent": 101 }), "min_relation_score_percent", 1, Some(100)).is_err());
        assert!(parse_optional_usize_min(&json!({ "limit": 0 }), "limit", 1, None).is_err());
        assert!(parse_optional_bool(&json!({ "include_relations": "yes" }), "include_relations").is_err());
        assert!(parse_optional_u32_positive(&json!({ "line": 0 }), "line", 1).is_err());
    }

    #[test]
    fn validate_relative_path_rejects_traversal_and_absolute_paths() {
        assert_eq!(
            validate_relative_path(&json!({ "rel_path": "./src/main.rs" }), "rel_path").unwrap(),
            "src/main.rs"
        );
        assert!(validate_relative_path(&json!({ "rel_path": "" }), "rel_path").is_err());
        assert!(validate_relative_path(&json!({ "rel_path": "/etc/passwd" }), "rel_path").is_err());
        assert!(validate_relative_path(&json!({ "rel_path": "../secret.txt" }), "rel_path").is_err());
    }

    #[test]
    fn every_knowledge_parameter_declares_a_description() {
        for def in static_actions() {
            let props = def
                .parameters_schema
                .get("properties")
                .and_then(|v| v.as_object())
                .expect("properties object");
            for (param, schema) in props {
                let desc = schema
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                assert!(
                    !desc.trim().is_empty(),
                    "knowledge action '{}' parameter '{}' must declare a non-empty description",
                    def.id,
                    param
                );
            }
        }
    }
}
