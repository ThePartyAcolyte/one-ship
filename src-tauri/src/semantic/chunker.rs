//! Multi-language structural & AST chunking pipeline with two-tier SHA-256 change detection
//! and deterministic reference extraction for the Hybrid Knowledge Graph (Spec 22 §5.2, §5.7).

use std::collections::{HashMap, HashSet};
use std::path::Path;
use sha2::{Digest, Sha256};
use tree_sitter::{Language, Node, Parser};

use crate::semantic::errors::SemanticError;
use crate::semantic::types::VectorEntityRecord;

/// Maximum recursion depth when walking a Tree-sitter AST to prevent stack overflow on pathological inputs.
const MAX_AST_DEPTH_COUNT: usize = 16;

/// Maximum number of distinct symbol/file references retained per chunk for graph linking.
const MAX_REFS_PER_CHUNK_COUNT: usize = 32;

/// Maximum byte length of `content_preview` stored in LanceDB records.
const MAX_PREVIEW_BYTES: usize = 600;

/// Target maximum lines per structural fallback block before splitting.
const STRUCTURAL_MAX_LINES_COUNT: usize = 55;

/// Minimum lines accumulated before a top-level blank line triggers a structural block split.
const STRUCTURAL_MIN_LINES_COUNT: usize = 12;

/// Sliding window size in lines when an individual block exceeds `STRUCTURAL_MAX_LINES_COUNT`.
const FALLBACK_WINDOW_LINES_COUNT: usize = 40;

/// Sliding window stride in lines (40 lines window - 10 lines overlap = 30 stride).
const FALLBACK_STRIDE_LINES_COUNT: usize = 30;

/// Computes a hex-encoded SHA-256 hash of a string.
pub fn hash_text(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Safely truncates a UTF-8 string to at most `max_bytes` on a valid character boundary.
pub fn truncate_utf8(text: &str, max_bytes: usize) -> &str {
    if text.len() <= max_bytes {
        return text;
    }
    let mut end = max_bytes;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// A parsed content chunk prior to vector embedding calculation.
#[derive(Debug, Clone)]
pub struct RawChunk {
    pub id: String,
    pub entity_type: String,
    pub entity_id: String,
    pub symbol_name: String,
    pub title: String,
    pub content: String,
    pub file_path: String,
    pub file_sha256: String,
    pub content_sha256: String,
    pub line_start: u32,
    pub line_end: u32,
    pub referenced_identifiers: Vec<String>,
}

impl RawChunk {
    /// Creates a `RawChunk` and deterministically computes its `content_sha256` from the embedding payload.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: String,
        entity_type: String,
        entity_id: String,
        symbol_name: String,
        title: String,
        content: String,
        file_path: String,
        file_sha256: String,
        line_start: u32,
        line_end: u32,
        referenced_identifiers: Vec<String>,
    ) -> Self {
        let embed_payload = format!("{}: {}\n{}", entity_type, title, content);
        let content_sha256 = hash_text(&embed_payload);
        Self {
            id,
            entity_type,
            entity_id,
            symbol_name,
            title,
            content,
            file_path,
            file_sha256,
            content_sha256,
            line_start,
            line_end,
            referenced_identifiers,
        }
    }

    /// Formats the header-boosted text sent to the embedding model.
    pub fn embedding_text(&self) -> String {
        format!("{}: {}\n{}", self.entity_type, self.title, self.content)
    }
}

/// Extracts candidate symbol and file-path references from prose or structural code text.
pub fn extract_text_references(text: &str) -> Vec<String> {
    let mut refs = Vec::new();
    let mut seen = HashSet::new();

    // 1. Extract backtick-delimited identifiers or file paths (`symbol` or `path/to/file.rs`)
    let mut rest = text;
    while let Some(start) = rest.find('`') {
        let after_start = &rest[start + 1..];
        let Some(end) = after_start.find('`') else {
            break;
        };
        let token = after_start[..end].trim().trim_matches(&['(', ')', '"', '\''][..]);
        if token.len() >= 3 && token.len() <= 120 && !token.contains('\n') {
            if seen.insert(token.to_string()) {
                refs.push(token.to_string());
                if refs.len() >= MAX_REFS_PER_CHUNK_COUNT {
                    return refs;
                }
            }
            // Also index trailing segment after `::` or `/` if qualified
            if let Some(short) = token.rsplit(&[':', '/'][..]).next() {
                let short_clean = short.trim_end_matches("()");
                if short_clean.len() >= 3 && seen.insert(short_clean.to_string()) {
                    refs.push(short_clean.to_string());
                    if refs.len() >= MAX_REFS_PER_CHUNK_COUNT {
                        return refs;
                    }
                }
            }
        }
        rest = &after_start[end + 1..];
    }

    // 2. Extract explicit file paths from prose (requiring '/' or a known file extension)
    for raw_word in text.split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-' && c != '.' && c != '/') {
        let token = raw_word.trim_matches(&['.', '/'][..]);
        if token.len() < 4 || token.len() > 120 {
            continue;
        }
        let looks_like_file_path = token.contains('/')
            || token.ends_with(".rs")
            || token.ends_with(".ts")
            || token.ends_with(".tsx")
            || token.ends_with(".js")
            || token.ends_with(".jsx")
            || token.ends_with(".vue")
            || token.ends_with(".svelte")
            || token.ends_with(".astro")
            || token.ends_with(".py")
            || token.ends_with(".go")
            || token.ends_with(".md");

        if looks_like_file_path && !is_common_keyword(token) && seen.insert(token.to_string()) {
            refs.push(token.to_string());
            if refs.len() >= MAX_REFS_PER_CHUNK_COUNT {
                break;
            }
        }
    }

    refs
}

/// Collects called/imported identifiers inside a Tree-sitter AST subtree for deterministic `calls`/`imports` edges.
fn collect_ast_references(node: Node<'_>, source_bytes: &[u8]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    walk_ast_references(node, source_bytes, 0, &mut seen, &mut out);
    out
}

fn walk_ast_references(
    node: Node<'_>,
    source_bytes: &[u8],
    depth_count: usize,
    seen: &mut HashSet<String>,
    out: &mut Vec<String>,
) {
    if depth_count > MAX_AST_DEPTH_COUNT || out.len() >= MAX_REFS_PER_CHUNK_COUNT {
        return;
    }

    let kind = node.kind();
    if matches!(
        kind,
        "call_expression"
            | "call"
            | "method_invocation"
            | "invocation_expression"
            | "scoped_identifier"
            | "field_expression"
            | "member_expression"
            | "use_declaration"
            | "import_statement"
            | "import_from_statement"
    ) {
        if let Some(ident) = extract_callee_or_import_token(node, source_bytes) {
            if ident.len() >= 3 && !is_common_keyword(&ident) && seen.insert(ident.clone()) {
                out.push(ident.clone());
                if let Some(short) = ident.rsplit("::").next() {
                    if short.len() >= 3 && !is_common_keyword(short) && seen.insert(short.to_string()) {
                        out.push(short.to_string());
                    }
                }
            }
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_ast_references(child, source_bytes, depth_count + 1, seen, out);
        if out.len() >= MAX_REFS_PER_CHUNK_COUNT {
            break;
        }
    }
}

fn extract_callee_or_import_token(node: Node<'_>, source_bytes: &[u8]) -> Option<String> {
    for field in ["function", "method", "name", "field", "property", "argument"] {
        if let Some(child) = node.child_by_field_name(field) {
            if let Ok(text) = child.utf8_text(source_bytes) {
                let cleaned = text.trim().trim_matches(&['"', '\'', ';'][..]);
                if !cleaned.is_empty() && cleaned.len() <= 96 && !cleaned.contains('\n') {
                    // For `obj.method` or `crate::mod::func`, take the last segment or scoped pair
                    if let Some(last_dot) = cleaned.rsplit('.').next() {
                        return Some(last_dot.trim().to_string());
                    }
                    return Some(cleaned.to_string());
                }
            }
        }
    }
    find_first_identifier(node, source_bytes)
}

fn is_common_keyword(token: &str) -> bool {
    matches!(
        token,
        "Self"
            | "self"
            | "super"
            | "crate"
            | "Some"
            | "None"
            | "Ok"
            | "Err"
            | "Result"
            | "Option"
            | "String"
            | "Vec"
            | "HashMap"
            | "HashSet"
            | "Arc"
            | "Mutex"
            | "RwLock"
            | "clone"
            | "into"
            | "from"
            | "iter"
            | "collect"
            | "unwrap"
            | "expect"
            | "format"
            | "println"
            | "to_string"
            | "as_str"
            | "default"
            | "true"
            | "false"
            | "null"
            | "undefined"
            | "Promise"
            | "Object"
            | "Array"
            | "Number"
            | "Boolean"
    )
}

/// Resolves a Tree-sitter `Language` grammar from a lowercase file extension.
fn resolve_tree_sitter_language(ext: &str) -> Option<Language> {
    match ext {
        "rs" => Some(tree_sitter_rust::LANGUAGE.into()),
        "ts" => Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
        "tsx" => Some(tree_sitter_typescript::LANGUAGE_TSX.into()),
        "js" | "jsx" | "mjs" | "cjs" => Some(tree_sitter_javascript::LANGUAGE.into()),
        "py" => Some(tree_sitter_python::LANGUAGE.into()),
        "go" => Some(tree_sitter_go::LANGUAGE.into()),
        "java" => Some(tree_sitter_java::LANGUAGE.into()),
        "cs" => Some(tree_sitter_c_sharp::LANGUAGE.into()),
        "c" | "h" => Some(tree_sitter_c::LANGUAGE.into()),
        "cpp" | "cc" | "cxx" | "hpp" | "hxx" => Some(tree_sitter_cpp::LANGUAGE.into()),
        "rb" => Some(tree_sitter_ruby::LANGUAGE.into()),
        "php" => Some(tree_sitter_php::LANGUAGE_PHP.into()),
        "sh" | "bash" | "zsh" => Some(tree_sitter_bash::LANGUAGE.into()),
        "html" | "htm" => Some(tree_sitter_html::LANGUAGE.into()),
        "css" | "scss" => Some(tree_sitter_css::LANGUAGE.into()),
        "json" => Some(tree_sitter_json::LANGUAGE.into()),
        _ => None,
    }
}

/// Parses a source code file into semantic symbol chunks using Tier 1 Tree-sitter AST parsing
/// (with SFC support for `.vue`, `.svelte`, and `.astro`) and Tier 2 structural fallback.
pub fn chunk_code_file(project_id: i64, relative_path: &str, content: &str) -> Vec<RawChunk> {
    let file_sha256 = hash_text(content);
    chunk_code_file_with_hash(project_id, relative_path, content, &file_sha256)
}

/// Parses a code file into structural chunks using a precomputed `file_sha256`.
pub fn chunk_code_file_with_hash(
    project_id: i64,
    relative_path: &str,
    content: &str,
    file_sha256: &str,
) -> Vec<RawChunk> {
    if content.trim().is_empty() {
        return Vec::new();
    }

    let ext = Path::new(relative_path)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    let mut name_occurrences: HashMap<String, usize> = HashMap::new();
    let mut chunks = Vec::new();

    // Handle Single-File Components (.vue, .svelte, .astro) by extracting <script>, <template>, <style>
    if matches!(ext.as_str(), "vue" | "svelte" | "astro") {
        extract_sfc_chunks(
            project_id,
            relative_path,
            content,
            file_sha256,
            &mut name_occurrences,
            &mut chunks,
        );
        if !chunks.is_empty() {
            return chunks;
        }
    } else if let Some(lang) = resolve_tree_sitter_language(&ext) {
        extract_ast_chunks_with_language(
            project_id,
            relative_path,
            content,
            file_sha256,
            lang,
            0,
            &mut name_occurrences,
            &mut chunks,
        );
        if !chunks.is_empty() {
            return chunks;
        }
    }

    // Tier 2: Language-agnostic brace & indentation structural segmenter
    chunk_structural_fallback(project_id, relative_path, content, file_sha256)
}

/// Extracts `<script>`, `<template>`, `<style>`, and Astro `---` frontmatter regions from `.vue`, `.svelte`, and `.astro` files.
fn extract_sfc_chunks(
    project_id: i64,
    relative_path: &str,
    content: &str,
    file_sha256: &str,
    name_occurrences: &mut HashMap<String, usize>,
    out_chunks: &mut Vec<RawChunk>,
) {
    let lines: Vec<&str> = content.lines().collect();
    let mut idx = 0usize;
    let is_astro = relative_path.to_ascii_lowercase().ends_with(".astro");
    let component_stem = Path::new(relative_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Component")
        .to_string();

    // Handle Astro `--- ... ---` frontmatter block at the top of `.astro` files
    if is_astro {
        while idx < lines.len() && lines[idx].trim().is_empty() {
            idx += 1;
        }
        if idx < lines.len() && lines[idx].trim() == "---" {
            let fm_start_idx = idx + 1;
            let mut fm_end_idx = fm_start_idx;
            while fm_end_idx < lines.len() && lines[fm_end_idx].trim() != "---" {
                fm_end_idx += 1;
            }
            if fm_end_idx > fm_start_idx && fm_end_idx < lines.len() {
                let fm_text = lines[fm_start_idx..fm_end_idx].join("\n");
                let line_offset = fm_start_idx as u32;
                let before_count = out_chunks.len();

                extract_ast_chunks_with_language(
                    project_id,
                    relative_path,
                    &fm_text,
                    file_sha256,
                    tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
                    line_offset,
                    name_occurrences,
                    out_chunks,
                );

                if out_chunks.len() == before_count && !fm_text.trim().is_empty() {
                    let symbol_key = next_qualified_key(&format!("{}::frontmatter", component_stem), name_occurrences);
                    let refs = extract_text_references(&fm_text);
                    out_chunks.push(RawChunk::new(
                        format!("code::{}::{}::{}", project_id, relative_path, symbol_key),
                        "code".to_string(),
                        format!("{}::{}", relative_path, symbol_key),
                        component_stem.clone(),
                        format!("{} (frontmatter) in {}", component_stem, relative_path),
                        fm_text,
                        relative_path.to_string(),
                        file_sha256.to_string(),
                        line_offset + 1,
                        fm_end_idx as u32,
                        refs,
                    ));
                }
                idx = fm_end_idx + 1;
            }
        }
    }

    let mut astro_markup_lines: Vec<(usize, &str)> = Vec::new();

    while idx < lines.len() {
        let trimmed = lines[idx].trim();
        let block_kind = if trimmed.starts_with("<script") {
            Some(("script", trimmed.contains("lang=\"ts\"") || trimmed.contains("lang='ts'")))
        } else if trimmed.starts_with("<style") {
            Some(("style", false))
        } else if trimmed.starts_with("<template") {
            Some(("template", false))
        } else {
            None
        };

        if let Some((tag, is_ts)) = block_kind {
            let close_tag = format!("</{}", tag);
            let body_start_idx = idx + 1;
            let mut body_end_idx = body_start_idx;
            while body_end_idx < lines.len() && !lines[body_end_idx].trim().starts_with(&close_tag) {
                body_end_idx += 1;
            }

            if body_end_idx > body_start_idx && body_end_idx <= lines.len() {
                let inner_text = lines[body_start_idx..body_end_idx].join("\n");
                let line_offset = body_start_idx as u32;
                let lang = match tag {
                    "script" if is_ts => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
                    "script" => tree_sitter_javascript::LANGUAGE.into(),
                    "style" => tree_sitter_css::LANGUAGE.into(),
                    _ => tree_sitter_html::LANGUAGE.into(),
                };

                let before_count = out_chunks.len();
                extract_ast_chunks_with_language(
                    project_id,
                    relative_path,
                    &inner_text,
                    file_sha256,
                    lang,
                    line_offset,
                    name_occurrences,
                    out_chunks,
                );

                // If a non-empty <template> or <script> had no inner AST symbols, keep it as a block chunk
                if out_chunks.len() == before_count && !inner_text.trim().is_empty() {
                    let base_sym = if tag == "template" {
                        component_stem.clone()
                    } else {
                        format!("{}::<{}>", component_stem, tag)
                    };
                    let symbol_key = next_qualified_key(&base_sym, name_occurrences);
                    let refs = extract_text_references(&inner_text);
                    out_chunks.push(RawChunk::new(
                        format!("code::{}::{}::{}", project_id, relative_path, symbol_key),
                        "code".to_string(),
                        format!("{}::{}", relative_path, symbol_key),
                        component_stem.clone(),
                        format!("{} in {}", symbol_key, relative_path),
                        inner_text,
                        relative_path.to_string(),
                        file_sha256.to_string(),
                        line_offset + 1,
                        body_end_idx as u32,
                        refs,
                    ));
                }
            }
            idx = (body_end_idx + 1).min(lines.len());
        } else {
            if is_astro && !trimmed.is_empty() {
                astro_markup_lines.push((idx, lines[idx]));
            }
            idx += 1;
        }
    }

    // Emit Astro template markup body outside <script>/<style> as the component template chunk
    if is_astro && !astro_markup_lines.is_empty() {
        let start_line = (astro_markup_lines.first().map(|(i, _)| *i).unwrap_or(0) + 1) as u32;
        let end_line = (astro_markup_lines.last().map(|(i, _)| *i).unwrap_or(0) + 1) as u32;
        let markup_text: String = astro_markup_lines
            .into_iter()
            .map(|(_, l)| l)
            .collect::<Vec<_>>()
            .join("\n");
        let symbol_key = next_qualified_key(&component_stem, name_occurrences);
        let refs = extract_text_references(&markup_text);
        out_chunks.push(RawChunk::new(
            format!("code::{}::{}::{}", project_id, relative_path, symbol_key),
            "code".to_string(),
            format!("{}::{}", relative_path, symbol_key),
            component_stem.clone(),
            format!("{} (template) in {}", component_stem, relative_path),
            markup_text,
            relative_path.to_string(),
            file_sha256.to_string(),
            start_line,
            end_line,
            refs,
        ));
    }
}

/// Parses `content` with the given Tree-sitter `Language` and extracts structural declarations.
#[allow(clippy::too_many_arguments)]
fn extract_ast_chunks_with_language(
    project_id: i64,
    relative_path: &str,
    content: &str,
    file_sha256: &str,
    language: Language,
    line_offset: u32,
    name_occurrences: &mut HashMap<String, usize>,
    out_chunks: &mut Vec<RawChunk>,
) {
    let mut parser = Parser::new();
    if parser.set_language(&language).is_err() {
        return;
    }

    let Some(tree) = parser.parse(content, None) else {
        return;
    };

    let source_bytes = content.as_bytes();
    walk_ast_node(
        tree.root_node(),
        source_bytes,
        content,
        project_id,
        relative_path,
        file_sha256,
        line_offset,
        None,
        0,
        name_occurrences,
        out_chunks,
    );
}

/// Recursively walks a Tree-sitter AST node to extract top-level and member declarations.
#[allow(clippy::too_many_arguments)]
fn walk_ast_node(
    node: Node<'_>,
    source_bytes: &[u8],
    full_text: &str,
    project_id: i64,
    relative_path: &str,
    file_sha256: &str,
    line_offset: u32,
    parent_scope: Option<&str>,
    depth_count: usize,
    name_occurrences: &mut HashMap<String, usize>,
    out_chunks: &mut Vec<RawChunk>,
) {
    if depth_count > MAX_AST_DEPTH_COUNT {
        return;
    }

    let kind = node.kind();

    // Check if this node is a container (impl, class, module, interface, namespace)
    if is_container_node_kind(kind) && node.named_child_count() > 0 {
        let container_name = extract_node_symbol_name(node, source_bytes)
            .unwrap_or_else(|| kind.to_string());
        let scoped_name = match parent_scope {
            Some(parent)
                if parent == container_name
                    || parent.ends_with(&format!("::{}", container_name)) =>
            {
                parent.to_string()
            }
            Some(parent) => format!("{}::{}", parent, container_name),
            None => container_name,
        };

        let (start_byte, start_line_zero) = include_leading_comments(node, source_bytes);
        let end_byte = node.end_byte().min(full_text.len());
        let end_line_zero = node.end_position().row as u32;
        let line_span_count = end_line_zero.saturating_sub(start_line_zero) + 1;

        // Emit a chunk for the container itself when it is compact or represents a type definition
        if line_span_count <= 80 || !has_nested_declaration_children(node) {
            if let Some(slice) = full_text.get(start_byte..end_byte) {
                if !slice.trim().is_empty() {
                    let unique_symbol = next_qualified_key(&scoped_name, name_occurrences);
                    let refs = collect_ast_references(node, source_bytes);
                    out_chunks.push(RawChunk::new(
                        format!("code::{}::{}::{}", project_id, relative_path, unique_symbol),
                        "code".to_string(),
                        format!("{}::{}", relative_path, unique_symbol),
                        scoped_name.clone(),
                        format!("{} in {}", unique_symbol, relative_path),
                        slice.to_string(),
                        relative_path.to_string(),
                        file_sha256.to_string(),
                        line_offset + start_line_zero + 1,
                        line_offset + end_line_zero + 1,
                        refs,
                    ));
                }
            }
        }

        // Recurse into children with `scoped_name` so methods are indexed as `Container::method`
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            walk_ast_node(
                child,
                source_bytes,
                full_text,
                project_id,
                relative_path,
                file_sha256,
                line_offset,
                Some(&scoped_name),
                depth_count + 1,
                name_occurrences,
                out_chunks,
            );
        }
        return;
    }

    // Check if this node is a concrete symbol declaration (function, method, struct, enum, etc.)
    if is_symbol_declaration_kind(kind) && node.named_child_count() > 0 {
        if let Some(raw_name) = extract_node_symbol_name(node, source_bytes) {
            let qualified = match parent_scope {
                Some(parent)
                    if parent == raw_name || parent.ends_with(&format!("::{}", raw_name)) =>
                {
                    parent.to_string()
                }
                Some(parent) => format!("{}::{}", parent, raw_name),
                None => raw_name,
            };
            let (start_byte, start_line_zero) = include_leading_comments(node, source_bytes);
            let end_byte = node.end_byte().min(full_text.len());
            let end_line_zero = node.end_position().row as u32;

            if let Some(slice) = full_text.get(start_byte..end_byte) {
                if !slice.trim().is_empty() {
                    let unique_symbol = next_qualified_key(&qualified, name_occurrences);
                    let refs = collect_ast_references(node, source_bytes);
                    out_chunks.push(RawChunk::new(
                        format!("code::{}::{}::{}", project_id, relative_path, unique_symbol),
                        "code".to_string(),
                        format!("{}::{}", relative_path, unique_symbol),
                        qualified,
                        format!("{} in {}", unique_symbol, relative_path),
                        slice.to_string(),
                        relative_path.to_string(),
                        file_sha256.to_string(),
                        line_offset + start_line_zero + 1,
                        line_offset + end_line_zero + 1,
                        refs,
                    ));
                    return;
                }
            }
        }
    }

    // Otherwise traverse child nodes (e.g. program, source_file, export_statement, declaration_list)
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        walk_ast_node(
            child,
            source_bytes,
            full_text,
            project_id,
            relative_path,
            file_sha256,
            line_offset,
            parent_scope,
            depth_count + 1,
            name_occurrences,
            out_chunks,
        );
    }
}

/// Returns true if the Tree-sitter node kind represents a type/module container with member declarations.
fn is_container_node_kind(kind: &str) -> bool {
    matches!(
        kind,
        "impl_item"
            | "trait_item"
            | "mod_item"
            | "class_declaration"
            | "class_definition"
            | "class_specifier"
            | "interface_declaration"
            | "namespace_declaration"
            | "module"
            | "class"
    )
}

/// Returns true if the Tree-sitter node kind represents an atomic symbol declaration.
fn is_symbol_declaration_kind(kind: &str) -> bool {
    matches!(
        kind,
        // Rust
        "function_item"
            | "struct_item"
            | "enum_item"
            | "type_item"
            | "const_item"
            | "static_item"
            | "macro_definition"
            // TypeScript / JavaScript
            | "function_declaration"
            | "generator_function_declaration"
            | "method_definition"
            | "type_alias_declaration"
            | "enum_declaration"
            | "lexical_declaration"
            // Python
            | "function_definition"
            // Go
            | "method_declaration"
            | "type_declaration"
            // Java / C# / PHP / Ruby
            | "constructor_declaration"
            | "property_declaration"
            | "record_declaration"
            | "method"
            | "singleton_method"
            // C / C++
            | "struct_specifier"
            | "enum_specifier"
            // CSS / HTML
            | "rule_set"
    )
}

/// Checks if a container node contains nested method/function declarations.
fn has_nested_declaration_children(node: Node<'_>) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if is_symbol_declaration_kind(child.kind()) {
            return true;
        }
        let mut inner_cursor = child.walk();
        for grandchild in child.children(&mut inner_cursor) {
            if is_symbol_declaration_kind(grandchild.kind()) {
                return true;
            }
        }
    }
    false
}

/// Extracts a human-readable identifier name from a declaration or container AST node.
fn extract_node_symbol_name(node: Node<'_>, source_bytes: &[u8]) -> Option<String> {
    for field in ["name", "type", "declarator"] {
        if let Some(child) = node.child_by_field_name(field) {
            if let Some(ident) = find_first_identifier(child, source_bytes) {
                return Some(ident);
            }
        }
    }
    find_first_identifier(node, source_bytes)
}

/// Searches a node and its shallow descendants for the first clean identifier text.
fn find_first_identifier(node: Node<'_>, source_bytes: &[u8]) -> Option<String> {
    let kind = node.kind();
    if is_identifier_kind(kind) {
        if let Ok(text) = node.utf8_text(source_bytes) {
            let trimmed = text.trim();
            if !trimmed.is_empty() && trimmed.len() <= 96 {
                return Some(trimmed.to_string());
            }
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        let ckind = child.kind();
        if is_identifier_kind(ckind) {
            if let Ok(text) = child.utf8_text(source_bytes) {
                let trimmed = text.trim();
                if !trimmed.is_empty() && trimmed.len() <= 96 {
                    return Some(trimmed.to_string());
                }
            }
        }
        if matches!(
            ckind,
            "variable_declarator"
                | "function_declarator"
                | "pointer_declarator"
                | "type_spec"
                | "function_definition"
                | "class_definition"
                | "selectors"
        ) {
            if let Some(nested) = find_first_identifier(child, source_bytes) {
                return Some(nested);
            }
        }
    }
    None
}

/// Returns true if a node kind represents an identifier token across supported grammars.
fn is_identifier_kind(kind: &str) -> bool {
    matches!(
        kind,
        "identifier"
            | "type_identifier"
            | "field_identifier"
            | "property_identifier"
            | "constant"
            | "name"
            | "word"
            | "class_selector"
            | "id_selector"
            | "tag_name"
    )
}

/// Walks backwards over contiguous preceding comment/decorator siblings so docstrings and decorators are attached to the chunk.
fn include_leading_comments(node: Node<'_>, _source_bytes: &[u8]) -> (usize, u32) {
    let base_node = match node.parent() {
        Some(parent) if parent.kind() == "decorated_definition" => parent,
        _ => node,
    };
    let mut start_byte = base_node.start_byte();
    let mut start_row = base_node.start_position().row as u32;
    let mut current = base_node;

    while let Some(prev) = current.prev_sibling() {
        let pkind = prev.kind();
        let prev_end_row = prev.end_position().row as u32;
        if (pkind.contains("comment") || pkind == "attribute_item" || pkind == "decorator")
            && start_row.saturating_sub(prev_end_row) <= 1
        {
            start_byte = prev.start_byte();
            start_row = prev.start_position().row as u32;
            current = prev;
        } else {
            break;
        }
    }

    (start_byte, start_row)
}

/// Produces a deterministic symbol key within a file, appending `#2`, `#3` only on duplicate symbol names.
fn next_qualified_key(base_name: &str, occurrences: &mut HashMap<String, usize>) -> String {
    let count = occurrences.entry(base_name.to_string()).or_insert(0);
    *count += 1;
    if *count == 1 {
        base_name.to_string()
    } else {
        format!("{}#{}", base_name, *count)
    }
}

/// Tier 2 language-agnostic structural segmenter respecting brace depth `{}` and blank-line boundaries.
fn chunk_structural_fallback(
    project_id: i64,
    relative_path: &str,
    content: &str,
    file_sha256: &str,
) -> Vec<RawChunk> {
    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() {
        return Vec::new();
    }

    let mut chunks = Vec::new();
    let mut block_start_idx = 0usize;
    let mut brace_depth: i32 = 0;
    let mut chunk_idx = 0usize;

    for (idx, line) in lines.iter().enumerate() {
        for ch in line.chars() {
            match ch {
                '{' | '[' => brace_depth += 1,
                '}' | ']' => brace_depth = (brace_depth - 1).max(0),
                _ => {}
            }
        }

        let current_len = idx - block_start_idx + 1;
        let is_blank = line.trim().is_empty();
        let is_last = idx + 1 == lines.len();

        let should_flush = is_last
            || (brace_depth == 0 && is_blank && current_len >= STRUCTURAL_MIN_LINES_COUNT)
            || (brace_depth == 0 && current_len >= STRUCTURAL_MAX_LINES_COUNT);

        if should_flush {
            emit_structural_window(
                project_id,
                relative_path,
                file_sha256,
                &lines[block_start_idx..=idx],
                block_start_idx,
                &mut chunk_idx,
                &mut chunks,
            );
            block_start_idx = idx + 1;
        }
    }

    chunks
}

/// Emits one or more overlapping chunks for a structural block slice.
fn emit_structural_window(
    project_id: i64,
    relative_path: &str,
    file_sha256: &str,
    slice: &[&str],
    base_line_idx: usize,
    chunk_idx: &mut usize,
    out_chunks: &mut Vec<RawChunk>,
) {
    if slice.iter().all(|l| l.trim().is_empty()) {
        return;
    }

    if slice.len() <= STRUCTURAL_MAX_LINES_COUNT {
        let text = slice.join("\n");
        let start_line = (base_line_idx + 1) as u32;
        let end_line = (base_line_idx + slice.len()) as u32;
        let idx_val = *chunk_idx;
        *chunk_idx += 1;
        let sym = format!("chunk_{}", idx_val);
        let refs = extract_text_references(&text);

        out_chunks.push(RawChunk::new(
            format!("code::{}::{}#{}", project_id, relative_path, sym),
            "code".to_string(),
            format!("{}#{}", relative_path, sym),
            sym,
            format!("{} (lines {}-{})", relative_path, start_line, end_line),
            text,
            relative_path.to_string(),
            file_sha256.to_string(),
            start_line,
            end_line,
            refs,
        ));
        return;
    }

    let mut offset = 0usize;
    while offset < slice.len() {
        let end = (offset + FALLBACK_WINDOW_LINES_COUNT).min(slice.len());
        let window_text = slice[offset..end].join("\n");
        if !window_text.trim().is_empty() {
            let start_line = (base_line_idx + offset + 1) as u32;
            let end_line = (base_line_idx + end) as u32;
            let idx_val = *chunk_idx;
            *chunk_idx += 1;
            let sym = format!("chunk_{}", idx_val);
            let refs = extract_text_references(&window_text);

            out_chunks.push(RawChunk::new(
                format!("code::{}::{}#{}", project_id, relative_path, sym),
                "code".to_string(),
                format!("{}#{}", relative_path, sym),
                sym,
                format!("{} (lines {}-{})", relative_path, start_line, end_line),
                window_text,
                relative_path.to_string(),
                file_sha256.to_string(),
                start_line,
                end_line,
                refs,
            ));
        }
        if end == slice.len() {
            break;
        }
        offset += FALLBACK_STRIDE_LINES_COUNT;
    }
}

/// Maximum byte size where deep sub-headings (`H4..H6`) remain coalesced into their parent section
/// instead of splitting into separate chunks.
const MARKDOWN_COALESCE_MAX_BYTES: usize = 4000;

/// Minimum byte size of an accumulated deep sub-section (`H4..H6`) before another sibling deep heading
/// triggers a separate chunk split.
const MARKDOWN_DEEP_SUBSECTION_MIN_BYTES: usize = 1200;

/// Parses a Markdown heading line (`#` through `######`) outside fenced code blocks,
/// returning `(heading_level, heading_title)`.
fn parse_markdown_heading(trimmed: &str) -> Option<(u8, &str)> {
    let hashes_count = trimmed.bytes().take_while(|&b| b == b'#').count();
    if !(1..=6).contains(&hashes_count) {
        return None;
    }
    let rest = &trimmed[hashes_count..];
    if !rest.starts_with(' ') && !rest.starts_with('\t') {
        return None;
    }
    let title = rest.trim();
    if title.is_empty() {
        None
    } else {
        Some((hashes_count as u8, title))
    }
}

/// Extracts hierarchical Markdown headings `(level, title, line_start, line_end)` outside fenced code blocks.
pub fn extract_markdown_headings(content: &str) -> Vec<(u8, String, u32, u32)> {
    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() {
        return Vec::new();
    }

    let total_lines = lines.len() as u32;
    let mut headings: Vec<(u8, String, u32)> = Vec::new();
    let mut in_code_fence = false;

    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_code_fence = !in_code_fence;
            continue;
        }
        if in_code_fence {
            continue;
        }
        if let Some((level, title)) = parse_markdown_heading(trimmed) {
            headings.push((level, title.to_string(), (idx + 1) as u32));
        }
    }

    let mut result = Vec::with_capacity(headings.len());
    for (idx, (level, title, start_line)) in headings.iter().enumerate() {
        let mut end_line = total_lines;
        for (next_level, _, next_start) in headings.iter().skip(idx + 1) {
            if *next_level <= *level {
                end_line = next_start.saturating_sub(1).max(*start_line);
                break;
            }
        }
        result.push((*level, title.clone(), *start_line, end_line));
    }
    result
}

/// Atomic Markdown section delimited by any heading (`H1..H6`) prior to dynamic hierarchical coalescing.
struct MarkdownSectionNode {
    level: u8,
    header: String,
    breadcrumb: String,
    start_line: u32,
    end_line: u32,
    text: String,
}

/// Parses a Markdown document (e.g. `docs/specs/*.md`) into section-header chunks.
pub fn chunk_markdown_spec(project_id: i64, relative_path: &str, content: &str) -> Vec<RawChunk> {
    let file_sha256 = hash_text(content);
    chunk_markdown_spec_with_hash(project_id, relative_path, content, &file_sha256)
}

/// Parses a Markdown document into dynamically coalesced hierarchical section chunks (`H1..H6`),
/// ignoring `#` inside fenced code blocks and coalescing small `H4..H6` sub-sections into their parent/sibling.
pub fn chunk_markdown_spec_with_hash(
    project_id: i64,
    relative_path: &str,
    content: &str,
    file_sha256: &str,
) -> Vec<RawChunk> {
    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() {
        return Vec::new();
    }

    let mut raw_nodes: Vec<MarkdownSectionNode> = Vec::new();
    let mut ancestors: Vec<(u8, String)> = Vec::new();
    let mut current_level = 1u8;
    let mut current_header = "Introduction".to_string();
    let mut current_breadcrumb = "Introduction".to_string();
    let mut start_line = 1u32;
    let mut buffer: Vec<&str> = Vec::new();
    let mut in_code_fence = false;

    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        let line_num = (idx + 1) as u32;

        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_code_fence = !in_code_fence;
        }

        if !in_code_fence {
            if let Some((level, heading_title)) = parse_markdown_heading(trimmed) {
                if !buffer.is_empty() {
                    let section_text = buffer.join("\n");
                    if !section_text.trim().is_empty() {
                        let end_line = line_num.saturating_sub(1).max(start_line);
                        raw_nodes.push(MarkdownSectionNode {
                            level: current_level,
                            header: current_header.clone(),
                            breadcrumb: current_breadcrumb.clone(),
                            start_line,
                            end_line,
                            text: section_text,
                        });
                    }
                    buffer.clear();
                }

                while ancestors.last().is_some_and(|(lvl, _)| *lvl >= level) {
                    ancestors.pop();
                }
                ancestors.push((level, heading_title.to_string()));

                current_level = level;
                current_header = heading_title.to_string();
                current_breadcrumb = ancestors
                    .iter()
                    .map(|(_, h)| h.as_str())
                    .collect::<Vec<_>>()
                    .join(" > ");
                start_line = line_num;
            }
        }

        buffer.push(line);
    }

    if !buffer.is_empty() {
        let section_text = buffer.join("\n");
        if !section_text.trim().is_empty() {
            let end_line = (lines.len() as u32).max(start_line);
            raw_nodes.push(MarkdownSectionNode {
                level: current_level,
                header: current_header,
                breadcrumb: current_breadcrumb,
                start_line,
                end_line,
                text: section_text,
            });
        }
    }

    // Dynamic hierarchical coalescing:
    // - Primary headings (H1..H3) always open a new section unless the previous block was a tiny header-only stub.
    // - Deep headings (H4..H6) coalesce into the current section when the combined size fits within
    //   `MARKDOWN_COALESCE_MAX_BYTES`, or when the current deep sub-section is still below `MARKDOWN_DEEP_SUBSECTION_MIN_BYTES`.
    let mut coalesced: Vec<MarkdownSectionNode> = Vec::with_capacity(raw_nodes.len());
    for node in raw_nodes {
        let should_merge_with_prev = if let Some(prev) = coalesced.last() {
            if node.level >= 4 {
                let combined_bytes = prev.text.len() + 1 + node.text.len();
                combined_bytes <= MARKDOWN_COALESCE_MAX_BYTES
                    || (prev.level >= 4 && prev.text.len() < MARKDOWN_DEEP_SUBSECTION_MIN_BYTES)
            } else {
                false
            }
        } else {
            false
        };

        if should_merge_with_prev {
            if let Some(prev) = coalesced.last_mut() {
                prev.text.push('\n');
                prev.text.push_str(&node.text);
                prev.end_line = node.end_line;
            }
        } else {
            coalesced.push(node);
        }
    }

    let mut chunks = Vec::with_capacity(coalesced.len());
    let mut slug_occurrences: HashMap<String, usize> = HashMap::new();

    for node in coalesced {
        let slug = next_qualified_key(&slugify(&node.header), &mut slug_occurrences);
        let refs = extract_text_references(&node.text);
        let display_header = if node.level >= 4 {
            &node.breadcrumb
        } else {
            &node.header
        };

        chunks.push(RawChunk::new(
            format!("specs::{}::{}#{}", project_id, relative_path, slug),
            "specs".to_string(),
            format!("{}#{}", relative_path, slug),
            slug,
            format!("{} (§{})", relative_path, display_header),
            node.text,
            relative_path.to_string(),
            file_sha256.to_string(),
            node.start_line,
            node.end_line,
            refs,
        ));
    }

    chunks
}

/// Extracts text from already-loaded `.pdf` bytes by page and segments it into `specs` chunks.
pub fn chunk_pdf_bytes(
    project_id: i64,
    relative_path: &str,
    raw_bytes: &[u8],
    file_sha256: &str,
) -> Result<Vec<RawChunk>, SemanticError> {
    let pages = pdf_extract::extract_text_from_mem_by_pages(raw_bytes)
        .map_err(|e| SemanticError::InvalidRequest(format!("PDF extraction failed for '{}': {}", relative_path, e)))?;

    let mut chunks = Vec::new();
    for (idx, page_text) in pages.into_iter().enumerate() {
        let trimmed = page_text.trim();
        if trimmed.is_empty() {
            continue;
        }
        let page_num = (idx + 1) as u32;
        let sym = format!("page_{}", page_num);
        let refs = extract_text_references(trimmed);
        chunks.push(RawChunk::new(
            format!("specs::{}::{}#{}", project_id, relative_path, sym),
            "specs".to_string(),
            format!("{}#{}", relative_path, sym),
            sym,
            format!("{} (Page {})", relative_path, page_num),
            trimmed.to_string(),
            relative_path.to_string(),
            file_sha256.to_string(),
            page_num,
            page_num,
            refs,
        ));
    }

    Ok(chunks)
}

/// Simple slug generator for section anchors.
fn slugify(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// Converts a raw chunk and an already-known embedding vector into a `VectorEntityRecord`.
pub fn build_vector_record(project_id: i64, chunk: RawChunk, vector: Vec<f32>) -> VectorEntityRecord {
    let preview = if chunk.content.len() > MAX_PREVIEW_BYTES {
        format!("{}...", truncate_utf8(&chunk.content, MAX_PREVIEW_BYTES))
    } else {
        chunk.content.clone()
    };

    VectorEntityRecord {
        id: chunk.id,
        project_id,
        entity_type: chunk.entity_type,
        entity_id: chunk.entity_id,
        symbol_name: chunk.symbol_name,
        title: chunk.title,
        content_preview: preview,
        file_path: chunk.file_path,
        file_sha256: chunk.file_sha256,
        content_sha256: chunk.content_sha256,
        line_start: chunk.line_start,
        line_end: chunk.line_end,
        vector,
    }
}

/// Converts raw chunks into vectorized records using the local BGE-M3 embedding engine.
pub fn vectorize_chunks(
    project_id: i64,
    raw_chunks: Vec<RawChunk>,
) -> Result<Vec<VectorEntityRecord>, SemanticError> {
    if raw_chunks.is_empty() {
        return Ok(Vec::new());
    }

    let texts: Vec<String> = raw_chunks.iter().map(RawChunk::embedding_text).collect();
    let vectors = crate::semantic::engine::embed_batch(&texts)?;

    let mut records = Vec::with_capacity(raw_chunks.len());
    for (chunk, vector) in raw_chunks.into_iter().zip(vectors.into_iter()) {
        records.push(build_vector_record(project_id, chunk, vector));
    }

    Ok(records)
}
