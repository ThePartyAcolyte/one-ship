//! Unit tests for the transversal semantic search pipeline.

use super::*;
use crate::semantic::chunker::{chunk_code_file, chunk_markdown_spec, hash_text, truncate_utf8};
use crate::semantic::types::KnowledgeScope;

#[test]
fn test_hash_text_deterministic() {
    let text = "fn calculate_orbit() -> f64 { 42.0 }";
    let hash1 = hash_text(text);
    let hash2 = hash_text(text);
    assert_eq!(hash1, hash2, "Hashing same text must produce identical SHA-256 digests");
    assert_eq!(hash1.len(), 64, "SHA-256 hex string must be 64 characters long");

    let modified = "fn calculate_orbit() -> f64 { 43.0 }";
    let hash3 = hash_text(modified);
    assert_ne!(hash1, hash3, "Different text must yield different hashes");
}

#[test]
fn test_truncate_utf8_respects_multibyte_boundaries() {
    let text = "Spec §5.2 — Búsqueda semántica con árbol sintáctico";
    for max_bytes in 0..=text.len() + 5 {
        let slice = truncate_utf8(text, max_bytes);
        assert!(slice.len() <= max_bytes);
    }
}

#[test]
fn test_chunk_markdown_spec_sections() {
    let markdown = r#"# Architecture Overview

This is the introductory overview of the system architecture.

## Vector Search Pipeline

LanceDB provides fast approximate nearest neighbor search over local embeddings.
BGE-M3 outputs 1024-dimension vectors.

## Host Resource Throttling

Sysinfo ensures that RAM and CPU consumption remain bounded during background indexing.
"#;

    let chunks = chunk_markdown_spec(1, "docs/specs/22_session_lifecycle.md", markdown);
    assert_eq!(chunks.len(), 3, "Expected 3 chunks: root intro + 2 sub-sections");

    // Intro chunk
    assert!(chunks[0].title.contains("Architecture Overview"));
    assert_eq!(chunks[0].line_start, 1);
    assert_eq!(chunks[0].entity_type, "specs");
    assert!(!chunks[0].file_sha256.is_empty());
    assert!(!chunks[0].content_sha256.is_empty());

    // Section 2
    assert!(chunks[1].title.contains("Vector Search Pipeline"));
    assert!(chunks[1].content.contains("LanceDB"));
    assert!(chunks[1].id.contains("#vector-search-pipeline"));

    // Section 3
    assert!(chunks[2].title.contains("Host Resource Throttling"));
    assert!(chunks[2].content.contains("Sysinfo"));
    assert!(chunks[2].id.contains("#host-resource-throttling"));
}

#[test]
fn test_chunk_markdown_dynamic_hierarchy_and_code_fence_safety() {
    let mut markdown = String::from(
        "# System Spec\n\nIntro paragraph.\n\n## Small Section\n\n```python\n# This is a Python comment, NOT a Markdown H1!\ndef run():\n    pass\n```\n\n#### Compact Subheading\n\nStays merged with Small Section.\n\n## Large Section\n\n",
    );
    markdown.push_str(&"Detailed architectural analysis paragraph. ".repeat(110));
    markdown.push_str("\n\n#### Deep Sub-Section Alpha\n\n");
    markdown.push_str(&"Alpha implementation details and reconciliation rules. ".repeat(35));
    markdown.push_str("\n\n#### Deep Sub-Section Beta\n\n");
    markdown.push_str(&"Beta implementation details and validation checks. ".repeat(35));

    let chunks = chunk_markdown_spec(1, "docs/specs/hierarchy.md", &markdown);

    // 1. Code fence `# This is a Python comment` must NOT split Small Section, and Compact Subheading
    // must coalesce into Small Section because combined size <= 4000 bytes.
    let small_chunk = chunks
        .iter()
        .find(|c| c.title.contains("Small Section"))
        .expect("Expected Small Section chunk");
    assert!(
        small_chunk.content.contains("# This is a Python comment"),
        "Code fence comment must stay inside Small Section"
    );
    assert!(
        small_chunk.content.contains("Compact Subheading"),
        "Small H4 must coalesce into compact parent H2"
    );

    // 2. Large Section (> 4000 bytes) must dynamically subdivide its H4 children with full breadcrumbs.
    let alpha_chunk = chunks
        .iter()
        .find(|c| c.title.contains("Deep Sub-Section Alpha"))
        .expect("Expected dynamically split H4 chunk when parent exceeds 4000 bytes");
    assert!(
        alpha_chunk.title.contains("Large Section > Deep Sub-Section Alpha"),
        "Subdivided deep heading must preserve ancestor breadcrumb in title"
    );
}

#[test]
fn test_chunk_code_file_tree_sitter_rust_and_line_shift_invariance() {
    let rust_code_v1 = r#"use std::sync::Arc;

/// Initializes the orbital kernel subsystem.
pub fn initialize_kernel() -> Result<(), String> {
    println!("Kernel ready");
    Ok(())
}

pub struct OrbitController {
    altitude_km: f64,
}

impl OrbitController {
    pub fn adjust_altitude(&mut self, delta_km: f64) {
        self.altitude_km += delta_km;
    }
}
"#;

    let chunks_v1 = chunk_code_file(1, "src/kernel.rs", rust_code_v1);
    assert!(!chunks_v1.is_empty(), "Tree-sitter Rust chunker must extract symbols");

    let init_chunk_v1 = chunks_v1
        .iter()
        .find(|c| c.id.ends_with("::initialize_kernel"))
        .expect("Expected initialize_kernel chunk");
    assert!(
        init_chunk_v1.content.contains("Initializes the orbital kernel subsystem"),
        "Leading doc comment should be attached to symbol chunk"
    );

    assert!(
        chunks_v1.iter().any(|c| c.id.contains("OrbitController::adjust_altitude")),
        "Expected scoped method OrbitController::adjust_altitude"
    );

    // Prepend 4 lines above initialize_kernel and verify ID + content_sha256 remain identical
    let rust_code_v2 = format!("// Header line 1\n// Header line 2\n\n{}", rust_code_v1);
    let chunks_v2 = chunk_code_file(1, "src/kernel.rs", &rust_code_v2);
    let init_chunk_v2 = chunks_v2
        .iter()
        .find(|c| c.id == init_chunk_v1.id)
        .expect("Symbol ID must remain stable when lines shift");

    assert_eq!(
        init_chunk_v1.content_sha256, init_chunk_v2.content_sha256,
        "Unchanged symbol shifted by lines must keep identical content_sha256"
    );
    assert!(init_chunk_v2.line_start > init_chunk_v1.line_start);
}

#[test]
fn test_chunk_code_file_multi_language_and_vue_sfc() {
    let py_code = r#"class TelemetryCollector:
    @staticmethod
    def sample_cpu(self) -> float:
        return 12.5
"#;
    let py_chunks = chunk_code_file(2, "agent/telemetry.py", py_code);
    assert!(
        py_chunks.iter().any(|c| c.symbol_name == "TelemetryCollector::sample_cpu"),
        "Python AST chunker must qualify class/method as Class::method without duplicating Class::Class"
    );

    let go_code = r#"package main

func DispatchCommand(cmd string) error {
    return nil
}
"#;
    let go_chunks = chunk_code_file(2, "cmd/main.go", go_code);
    assert!(
        go_chunks.iter().any(|c| c.title.contains("DispatchCommand")),
        "Go AST chunker must extract function declarations"
    );

    let vue_code = r#"<template>
  <div class="panel">Hello</div>
</template>

<script setup lang="ts">
export function computeScore(dist: number): number {
  return Math.max(0, 1 - dist / 2);
}
</script>
"#;
    let vue_chunks = chunk_code_file(2, "src/View.vue", vue_code);
    assert!(
        vue_chunks.iter().any(|c| c.title.contains("computeScore")),
        "Vue SFC chunker must extract TypeScript symbols inside <script setup>"
    );
}

#[test]
fn test_causal_knowledge_graph_edges_hunks_anchors_and_prose_only() {
    let spec_chunks = chunk_markdown_spec(
        1,
        "docs/specs/22_spec.md",
        "## Pipeline Rules\n\nGoverns `src/pipeline.rs` and references `docs/specs/01_core.md`.",
    );

    let mut nodes: Vec<GraphNodeMeta> = spec_chunks
        .iter()
        .map(|c| GraphNodeMeta {
            id: c.id.clone(),
            entity_type: c.entity_type.clone(),
            title: c.title.clone(),
            file_path: c.file_path.clone(),
            line_start: c.line_start,
            refs: c.referenced_identifiers.clone(),
            vector: vec![0.8f32, 0.2f32, 0.0f32, 0.0f32],
        })
        .collect();

    nodes.push(GraphNodeMeta {
        id: "journal::1::42".to_string(),
        entity_type: "journal".to_string(),
        title: "Journal: Hardened pipeline".to_string(),
        file_path: "journal://42".to_string(),
        line_start: 1,
        refs: vec!["src/pipeline.rs".to_string()],
        vector: vec![0.8f32, 0.2f32, 0.0f32, 0.0f32],
    });

    let hunks = vec![crate::db::JournalFileHunkRecord {
        project_id: 1,
        journal_id: 42,
        file_path: "src/pipeline.rs".to_string(),
        line_start: 12,
        line_end: 30,
        change_kind: "modified".to_string(),
        source_kind: "git_commit".to_string(),
    }];

    let anchors = vec![EntityAnchorMeta {
        node_id: "tech_debt::1::7".to_string(),
        entity_type: "tech_debt".to_string(),
        title: "Tech Debt: Split scanner".to_string(),
        file_path: Some("src/pipeline.rs".to_string()),
        line_start: Some(15),
        journal_id: Some(42),
    }];

    let known_files: HashSet<String> = [
        "src/pipeline.rs".to_string(),
        "docs/specs/22_spec.md".to_string(),
    ]
    .into_iter()
    .collect();

    let edges = build_project_knowledge_edges(1, &nodes, &hunks, &anchors, &known_files, 70);

    assert!(
        edges.iter().any(|e| e.edge_type == "touched"
            && e.origin == "git_hunk"
            && e.source_id == "journal::1::42"
            && e.target_id == "file::1::src/pipeline.rs"
            && e.target_line_start == Some(12)),
        "Expected causal 'touched' edge with hunk line_start=12"
    );
    assert!(
        edges.iter().any(|e| e.edge_type == "affects"
            && e.origin == "anchor"
            && e.source_id == "tech_debt::1::7"
            && e.target_id == "file::1::src/pipeline.rs"),
        "Expected anchored tech_debt 'affects' edge to file"
    );
    assert!(
        edges.iter().any(|e| e.edge_type == "documents"
            && e.origin == "semantic"
            && e.source_id == "journal::1::42"
            && e.target_entity_type == "specs"),
        "Expected prose-only semantic 'documents' edge from journal to spec"
    );
    assert!(
        edges.iter().any(|e| e.edge_type == "governs"
            && e.origin == "explicit_ref"
            && e.source_entity_type == "specs"
            && e.target_id == "file::1::src/pipeline.rs"),
        "Expected exact-path 'governs' edge from spec to file"
    );
    assert!(
        !edges.iter().any(|e| e.origin == "ast" || e.edge_type == "calls"),
        "Knowledge graph must not emit noisy AST 'calls' edges"
    );
}

#[test]
fn test_knowledge_scope_filter_strings() {
    assert_eq!(KnowledgeScope::All.as_filter_str(), None);
    assert_eq!(KnowledgeScope::Code.as_filter_str(), Some("code"));
    assert_eq!(KnowledgeScope::Specs.as_filter_str(), Some("specs"));
    assert_eq!(KnowledgeScope::Tasks.as_filter_str(), Some("tasks"));
    assert_eq!(KnowledgeScope::TechDebt.as_filter_str(), Some("tech_debt"));
    assert_eq!(KnowledgeScope::Journal.as_filter_str(), Some("journal"));
}

#[test]
fn test_throttler_evaluation_does_not_panic() {
    let bg_allowed = throttler::can_run_background_indexing();
    let manual_allowed = throttler::can_run_manual_indexing();
    let _ = bg_allowed;
    assert!(manual_allowed, "Host should have sufficient RAM (> 512 MB) during tests");
}

#[test]
fn test_cosine_distance_to_score_mapping() {
    let dist_zero = 0.0f32;
    let score_one = (1.0f32 - (dist_zero / 2.0f32)).clamp(0.0f32, 1.0f32);
    assert!((score_one - 1.0).abs() < f32::EPSILON);

    let dist_one = 1.0f32;
    let score_half = (1.0f32 - (dist_one / 2.0f32)).clamp(0.0f32, 1.0f32);
    assert!((score_half - 0.5).abs() < f32::EPSILON);

    let dist_two = 2.0f32;
    let score_zero = (1.0f32 - (dist_two / 2.0f32)).clamp(0.0f32, 1.0f32);
    assert!((score_zero - 0.0).abs() < f32::EPSILON);
}
