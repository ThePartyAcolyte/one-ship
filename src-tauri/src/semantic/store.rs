//! Embedded LanceDB vector store managing vector indexation, Apache Arrow schema, and ANN search.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use futures_util::TryStreamExt;
use lancedb::arrow::arrow_array::types::Float32Type;
use lancedb::arrow::arrow_array::{
    Array, FixedSizeListArray, Float32Array, Int64Array, RecordBatch, StringArray, UInt32Array,
};
use lancedb::arrow::arrow_schema::{DataType, Field, Schema};
use lancedb::query::{ExecutableQuery, QueryBase};
use lancedb::{connect, DistanceType, Table as LanceDbTable};

use crate::semantic::errors::SemanticError;
use crate::semantic::types::{KnowledgeSearchResult, KnowledgeScope, VectorEntityRecord};

/// Embedding vector dimension produced by BGE-M3.
pub const EMBEDDING_DIM: usize = 1024;

/// Target table name within the LanceDB database (Causal Knowledge v2: prose-only embeddings).
const KNOWLEDGE_TABLE_NAME: &str = "knowledge_v2";
/// Legacy v1 table name purged automatically on first boot after Causal Knowledge v2 upgrade.
const LEGACY_KNOWLEDGE_TABLE_NAME: &str = "knowledge";

/// Maximum number of record IDs combined in a single `id IN (...)` delete predicate.
const DELETE_BATCH_SIZE_COUNT: usize = 100;

/// Returns the filesystem directory path for LanceDB storage (`~/.config/one-ship/vectors.lance/`).
fn get_storage_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("one-ship")
        .join("vectors.lance")
}

/// Constructs the canonical Apache Arrow schema for knowledge records (Phase B schema with `symbol_name` + two-tier SHA-256).
fn build_schema() -> Arc<Schema> {
    Arc::new(Schema::new(vec![
        Field::new("id", DataType::Utf8, false),
        Field::new("project_id", DataType::Int64, false),
        Field::new("entity_type", DataType::Utf8, false),
        Field::new("entity_id", DataType::Utf8, false),
        Field::new("symbol_name", DataType::Utf8, false),
        Field::new("title", DataType::Utf8, false),
        Field::new("content_preview", DataType::Utf8, false),
        Field::new("file_path", DataType::Utf8, false),
        Field::new("file_sha256", DataType::Utf8, false),
        Field::new("content_sha256", DataType::Utf8, false),
        Field::new("line_start", DataType::UInt32, false),
        Field::new("line_end", DataType::UInt32, false),
        Field::new(
            "vector",
            DataType::FixedSizeList(
                Arc::new(Field::new("item", DataType::Float32, true)),
                EMBEDDING_DIM as i32,
            ),
            true,
        ),
    ]))
}

/// Converts a batch of domain vector records into an Apache Arrow `RecordBatch`.
fn records_to_batch(records: &[VectorEntityRecord]) -> Result<RecordBatch, SemanticError> {
    let schema = build_schema();

    let ids = Arc::new(StringArray::from_iter_values(records.iter().map(|r| r.id.as_str())));
    let project_ids = Arc::new(Int64Array::from_iter_values(records.iter().map(|r| r.project_id)));
    let entity_types = Arc::new(StringArray::from_iter_values(records.iter().map(|r| r.entity_type.as_str())));
    let entity_ids = Arc::new(StringArray::from_iter_values(records.iter().map(|r| r.entity_id.as_str())));
    let symbol_names = Arc::new(StringArray::from_iter_values(records.iter().map(|r| r.symbol_name.as_str())));
    let titles = Arc::new(StringArray::from_iter_values(records.iter().map(|r| r.title.as_str())));
    let previews = Arc::new(StringArray::from_iter_values(records.iter().map(|r| r.content_preview.as_str())));
    let file_paths = Arc::new(StringArray::from_iter_values(records.iter().map(|r| r.file_path.as_str())));
    let file_hashes = Arc::new(StringArray::from_iter_values(records.iter().map(|r| r.file_sha256.as_str())));
    let content_hashes = Arc::new(StringArray::from_iter_values(records.iter().map(|r| r.content_sha256.as_str())));
    let line_starts = Arc::new(UInt32Array::from_iter_values(records.iter().map(|r| r.line_start)));
    let line_ends = Arc::new(UInt32Array::from_iter_values(records.iter().map(|r| r.line_end)));

    let vector_array = Arc::new(
        FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(
            records.iter().map(|r| {
                if r.vector.len() == EMBEDDING_DIM {
                    Some(r.vector.iter().copied().map(Some))
                } else {
                    None
                }
            }),
            EMBEDDING_DIM as i32,
        ),
    );

    RecordBatch::try_new(
        schema,
        vec![
            ids,
            project_ids,
            entity_types,
            entity_ids,
            symbol_names,
            titles,
            previews,
            file_paths,
            file_hashes,
            content_hashes,
            line_starts,
            line_ends,
            vector_array,
        ],
    )
    .map_err(|e| SemanticError::Store(format!("Failed to build RecordBatch: {}", e)))
}

/// Opens or lazily creates the `knowledge` LanceDB table, recreating it cleanly if its schema predates Phase B (`symbol_name` + SHA-256).
async fn get_or_create_table() -> Result<LanceDbTable, SemanticError> {
    let storage_path = get_storage_path();
    let uri = storage_path.to_string_lossy().to_string();

    let db = connect(&uri)
        .execute()
        .await
        .map_err(|e| SemanticError::Store(format!("Failed to connect to LanceDB at '{}': {}", uri, e)))?;

    let table_names = db
        .table_names()
        .execute()
        .await
        .map_err(|e| SemanticError::Store(format!("Failed to list LanceDB tables: {}", e)))?;

    if table_names.iter().any(|t| t == LEGACY_KNOWLEDGE_TABLE_NAME) {
        crate::logger::log(
            "INFO",
            "semantic",
            "Dropping legacy LanceDB 'knowledge' table for clean Causal Knowledge v2 reset",
        );
        db.drop_table(LEGACY_KNOWLEDGE_TABLE_NAME, &[])
            .await
            .map_err(|e| SemanticError::Store(format!("Failed to drop legacy table '{}': {}", LEGACY_KNOWLEDGE_TABLE_NAME, e)))?;
    }

    if table_names.iter().any(|t| t == KNOWLEDGE_TABLE_NAME) {
        let table = db
            .open_table(KNOWLEDGE_TABLE_NAME)
            .execute()
            .await
            .map_err(|e| SemanticError::Store(format!("Failed to open table '{}': {}", KNOWLEDGE_TABLE_NAME, e)))?;

        if let Ok(existing_schema) = table.schema().await {
            let has_file_sha = existing_schema.field_with_name("file_sha256").is_ok();
            let has_content_sha = existing_schema.field_with_name("content_sha256").is_ok();
            let has_symbol_name = existing_schema.field_with_name("symbol_name").is_ok();
            if has_file_sha && has_content_sha && has_symbol_name {
                return Ok(table);
            }
        }

        crate::logger::log(
            "INFO",
            "semantic",
            "Resetting LanceDB 'knowledge' table to clean Phase B schema (symbol_name + two-tier SHA-256)",
        );
        db.drop_table(KNOWLEDGE_TABLE_NAME, &[])
            .await
            .map_err(|e| SemanticError::Store(format!("Failed to drop legacy table '{}': {}", KNOWLEDGE_TABLE_NAME, e)))?;
    }

    let schema = build_schema();
    db.create_empty_table(KNOWLEDGE_TABLE_NAME, schema)
        .execute()
        .await
        .map_err(|e| SemanticError::Store(format!("Failed to create empty table '{}': {}", KNOWLEDGE_TABLE_NAME, e)))
}

/// Fetches all existing vector records for a project from LanceDB to support two-tier SHA-256 incremental diffing.
pub async fn fetch_project_records(
    project_id: i64,
) -> Result<HashMap<String, VectorEntityRecord>, SemanticError> {
    let table = get_or_create_table().await?;
    let stream = table
        .query()
        .only_if(format!("project_id = {}", project_id))
        .execute()
        .await
        .map_err(|e| SemanticError::Store(format!("Failed to query existing project records: {}", e)))?;

    let batches: Vec<RecordBatch> = stream
        .try_collect()
        .await
        .map_err(|e| SemanticError::Store(format!("Failed to collect existing project records: {}", e)))?;

    let mut records_by_id = HashMap::new();

    for batch in batches {
        let count = batch.num_rows();
        let ids = batch.column_by_name("id").and_then(|c| c.as_any().downcast_ref::<StringArray>());
        let project_ids = batch.column_by_name("project_id").and_then(|c| c.as_any().downcast_ref::<Int64Array>());
        let entity_types = batch.column_by_name("entity_type").and_then(|c| c.as_any().downcast_ref::<StringArray>());
        let entity_ids = batch.column_by_name("entity_id").and_then(|c| c.as_any().downcast_ref::<StringArray>());
        let symbol_names = batch.column_by_name("symbol_name").and_then(|c| c.as_any().downcast_ref::<StringArray>());
        let titles = batch.column_by_name("title").and_then(|c| c.as_any().downcast_ref::<StringArray>());
        let previews = batch.column_by_name("content_preview").and_then(|c| c.as_any().downcast_ref::<StringArray>());
        let file_paths = batch.column_by_name("file_path").and_then(|c| c.as_any().downcast_ref::<StringArray>());
        let file_hashes = batch.column_by_name("file_sha256").and_then(|c| c.as_any().downcast_ref::<StringArray>());
        let content_hashes = batch.column_by_name("content_sha256").and_then(|c| c.as_any().downcast_ref::<StringArray>());
        let line_starts = batch.column_by_name("line_start").and_then(|c| c.as_any().downcast_ref::<UInt32Array>());
        let line_ends = batch.column_by_name("line_end").and_then(|c| c.as_any().downcast_ref::<UInt32Array>());
        let vectors = batch.column_by_name("vector").and_then(|c| c.as_any().downcast_ref::<FixedSizeListArray>());

        for i in 0..count {
            let id = ids.map(|a| a.value(i).to_string()).unwrap_or_default();
            if id.is_empty() {
                continue;
            }
            let pid = project_ids.map(|a| a.value(i)).unwrap_or(project_id);
            let etype = entity_types.map(|a| a.value(i).to_string()).unwrap_or_default();
            let eid = entity_ids.map(|a| a.value(i).to_string()).unwrap_or_default();
            let sname = symbol_names.map(|a| a.value(i).to_string()).unwrap_or_default();
            let title = titles.map(|a| a.value(i).to_string()).unwrap_or_default();
            let preview = previews.map(|a| a.value(i).to_string()).unwrap_or_default();
            let fpath = file_paths.map(|a| a.value(i).to_string()).unwrap_or_default();
            let fhash = file_hashes.map(|a| a.value(i).to_string()).unwrap_or_default();
            let chash = content_hashes.map(|a| a.value(i).to_string()).unwrap_or_default();
            let lstart = line_starts.map(|a| a.value(i)).unwrap_or(1);
            let lend = line_ends.map(|a| a.value(i)).unwrap_or(1);

            let vec_values: Vec<f32> = vectors
                .and_then(|arr| {
                    let sub = arr.value(i);
                    sub.as_any()
                        .downcast_ref::<Float32Array>()
                        .map(|farr| farr.values().to_vec())
                })
                .unwrap_or_default();

            if vec_values.len() == EMBEDDING_DIM {
                records_by_id.insert(
                    id.clone(),
                    VectorEntityRecord {
                        id,
                        project_id: pid,
                        entity_type: etype,
                        entity_id: eid,
                        symbol_name: sname,
                        title,
                        content_preview: preview,
                        file_path: fpath,
                        file_sha256: fhash,
                        content_sha256: chash,
                        line_start: lstart,
                        line_end: lend,
                        vector: vec_values,
                    },
                );
            }
        }
    }

    Ok(records_by_id)
}

/// Deletes a list of record IDs from LanceDB in bounded batches (`id IN (...)`).
pub async fn delete_records_by_ids(project_id: i64, ids: &[String]) -> Result<(), SemanticError> {
    if ids.is_empty() {
        return Ok(());
    }

    let table = get_or_create_table().await?;
    for chunk in ids.chunks(DELETE_BATCH_SIZE_COUNT) {
        let quoted: Vec<String> = chunk
            .iter()
            .map(|id| format!("'{}'", id.replace('\'', "''")))
            .collect();
        let predicate = format!("project_id = {} AND id IN ({})", project_id, quoted.join(", "));
        table
            .delete(&predicate)
            .await
            .map_err(|e| SemanticError::Store(format!("Failed batch delete in LanceDB: {}", e)))?;
    }

    Ok(())
}

/// Inserts or updates a batch of vector records in LanceDB, deduplicating by `id` and using batched `id IN (...)` deletion.
pub async fn upsert_records(records: Vec<VectorEntityRecord>) -> Result<(), SemanticError> {
    if records.is_empty() {
        return Ok(());
    }

    // Deduplicate within the incoming batch so duplicate IDs are never written together
    let mut seen_ids = HashSet::with_capacity(records.len());
    let deduped: Vec<VectorEntityRecord> = records
        .into_iter()
        .filter(|r| seen_ids.insert(r.id.clone()))
        .collect();

    let table = get_or_create_table().await?;

    for chunk in deduped.chunks(DELETE_BATCH_SIZE_COUNT) {
        let quoted: Vec<String> = chunk
            .iter()
            .map(|r| format!("'{}'", r.id.replace('\'', "''")))
            .collect();
        let predicate = format!("id IN ({})", quoted.join(", "));
        let _ = table.delete(&predicate).await;
    }

    let batch = records_to_batch(&deduped)?;
    table
        .add(batch)
        .execute()
        .await
        .map_err(|e| SemanticError::Store(format!("Failed to insert records into LanceDB: {}", e)))?;

    Ok(())
}

/// Counts the total number of indexed vector records for a specific project.
pub async fn count_project_records(project_id: i64) -> Result<usize, SemanticError> {
    let table = get_or_create_table().await?;
    let count = table
        .count_rows(Some(format!("project_id = {}", project_id)))
        .await
        .map_err(|e| SemanticError::Store(format!("Failed to count records: {}", e)))?;
    Ok(count)
}

/// Counts indexed vector records broken down into `(total, code_count, specs_count, entities_count)`.
pub async fn count_project_records_breakdown(
    project_id: i64,
) -> Result<(usize, usize, usize, usize), SemanticError> {
    let table = get_or_create_table().await?;
    let code_count = table
        .count_rows(Some(format!("project_id = {} AND entity_type = 'code'", project_id)))
        .await
        .map_err(|e| SemanticError::Store(format!("Failed to count code records: {}", e)))?;
    let specs_count = table
        .count_rows(Some(format!("project_id = {} AND entity_type = 'specs'", project_id)))
        .await
        .map_err(|e| SemanticError::Store(format!("Failed to count specs records: {}", e)))?;
    let total_count = table
        .count_rows(Some(format!("project_id = {}", project_id)))
        .await
        .map_err(|e| SemanticError::Store(format!("Failed to count total records: {}", e)))?;
    let entities_count = total_count.saturating_sub(code_count + specs_count);
    Ok((total_count, code_count, specs_count, entities_count))
}

/// Returns the set of distinct `file_path` values currently stored in LanceDB for `project_id`.
pub async fn list_indexed_file_paths(project_id: i64) -> Result<HashSet<String>, SemanticError> {
    let table = get_or_create_table().await?;
    let stream = table
        .query()
        .only_if(format!("project_id = {}", project_id))
        .select(lancedb::query::Select::columns(&["file_path"]))
        .execute()
        .await
        .map_err(|e| SemanticError::Store(format!("Failed to query indexed file paths: {}", e)))?;

    let batches: Vec<RecordBatch> = stream
        .try_collect()
        .await
        .map_err(|e| SemanticError::Store(format!("Failed to collect indexed file paths: {}", e)))?;

    let mut paths = HashSet::new();
    for batch in &batches {
        if let Some(col) = batch.column_by_name("file_path").and_then(|c| c.as_any().downcast_ref::<StringArray>()) {
            for i in 0..col.len() {
                if !col.is_null(i) {
                    let val = col.value(i).trim();
                    if !val.is_empty() {
                        paths.insert(val.to_string());
                    }
                }
            }
        }
    }
    Ok(paths)
}

/// Performs an Approximate Nearest Neighbor (ANN) search over LanceDB with predicate pushdown.
pub async fn search_knowledge(
    query_vector: &[f32],
    scope: Option<KnowledgeScope>,
    project_id: Option<i64>,
    limit: usize,
) -> Result<Vec<KnowledgeSearchResult>, SemanticError> {
    let table = get_or_create_table().await?;

    // Request slightly more candidates before deduplication so `limit` unique results are returned
    let fetch_limit = (limit * 2).max(10);
    let mut query = table
        .query()
        .nearest_to(query_vector)
        .map_err(|e| SemanticError::Store(format!("Failed to configure vector query: {}", e)))?
        .limit(fetch_limit)
        .distance_type(DistanceType::Cosine);

    // Build scalar filter predicate
    let mut predicates = Vec::new();
    if let Some(s) = scope {
        if let Some(type_str) = s.as_filter_str() {
            predicates.push(format!("entity_type = '{}'", type_str));
        }
    }
    if let Some(pid) = project_id {
        predicates.push(format!("project_id = {}", pid));
    }

    if !predicates.is_empty() {
        query = query.only_if(predicates.join(" AND "));
    }

    let stream = query
        .execute()
        .await
        .map_err(|e| SemanticError::Store(format!("Failed to execute vector search: {}", e)))?;

    let batches: Vec<RecordBatch> = stream
        .try_collect()
        .await
        .map_err(|e| SemanticError::Store(format!("Failed to collect search results: {}", e)))?;

    let mut results = Vec::new();

    for batch in batches {
        let count = batch.num_rows();
        let ids = batch.column_by_name("id").and_then(|c| c.as_any().downcast_ref::<StringArray>());
        let project_ids = batch.column_by_name("project_id").and_then(|c| c.as_any().downcast_ref::<Int64Array>());
        let entity_types = batch.column_by_name("entity_type").and_then(|c| c.as_any().downcast_ref::<StringArray>());
        let entity_ids = batch.column_by_name("entity_id").and_then(|c| c.as_any().downcast_ref::<StringArray>());
        let titles = batch.column_by_name("title").and_then(|c| c.as_any().downcast_ref::<StringArray>());
        let previews = batch.column_by_name("content_preview").and_then(|c| c.as_any().downcast_ref::<StringArray>());
        let file_paths = batch.column_by_name("file_path").and_then(|c| c.as_any().downcast_ref::<StringArray>());
        let line_starts = batch.column_by_name("line_start").and_then(|c| c.as_any().downcast_ref::<UInt32Array>());
        let line_ends = batch.column_by_name("line_end").and_then(|c| c.as_any().downcast_ref::<UInt32Array>());
        let distances = batch.column_by_name("_distance").and_then(|c| c.as_any().downcast_ref::<Float32Array>());

        for i in 0..count {
            let id = ids.map(|a| a.value(i).to_string()).unwrap_or_default();
            let pid = project_ids.map(|a| a.value(i)).unwrap_or(0);
            let etype = entity_types.map(|a| a.value(i).to_string()).unwrap_or_default();
            let eid = entity_ids.map(|a| a.value(i).to_string()).unwrap_or_default();
            let title = titles.map(|a| a.value(i).to_string()).unwrap_or_default();
            let preview = previews.map(|a| a.value(i).to_string()).unwrap_or_default();
            let fpath = file_paths.map(|a| a.value(i)).filter(|s| !s.is_empty()).map(|s| s.to_string());
            let lstart = line_starts.map(|a| a.value(i)).filter(|&n| n > 0);
            let lend = line_ends.map(|a| a.value(i)).filter(|&n| n > 0);

            // Cosine distance is in [0, 2]; convert to similarity score in [0.0, 1.0]
            let dist = distances.map(|a| a.value(i)).unwrap_or(1.0);
            let score = (1.0f32 - (dist / 2.0f32)).clamp(0.0f32, 1.0f32);

            results.push(KnowledgeSearchResult {
                id,
                project_id: pid,
                entity_type: etype,
                entity_id: eid,
                title,
                content_preview: preview,
                file_path: fpath,
                line_start: lstart,
                line_end: lend,
                score,
                relations: Vec::new(),
            });
        }
    }

    // Sort descending by similarity score first, then deduplicate by `id`
    results.sort_by(|a, b| b.score.total_cmp(&a.score));
    let mut seen_ids = HashSet::with_capacity(results.len());
    results.retain(|item| seen_ids.insert(item.id.clone()));
    results.truncate(limit);
    Ok(results)
}

/// Lightweight indexed chunk metadata fetched from LanceDB without deserializing embedding vectors.
#[derive(Debug, Clone)]
pub struct StoredChunkMeta {
    pub id: String,
    pub entity_type: String,
    pub symbol_name: String,
    pub title: String,
    pub content_preview: String,
    pub file_path: String,
    pub line_start: u32,
    pub line_end: u32,
}

fn collect_chunk_metadata(batches: Vec<RecordBatch>) -> Result<Vec<StoredChunkMeta>, SemanticError> {
    let mut items = Vec::new();
    let mut seen = HashSet::new();

    for batch in batches {
        let count = batch.num_rows();
        let ids = batch
            .column_by_name("id")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>())
            .ok_or_else(|| SemanticError::Store("Missing 'id' StringArray column in LanceDB batch".to_string()))?;
        let entity_types = batch
            .column_by_name("entity_type")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>())
            .ok_or_else(|| SemanticError::Store("Missing 'entity_type' StringArray column in LanceDB batch".to_string()))?;
        let symbol_names = batch
            .column_by_name("symbol_name")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>())
            .ok_or_else(|| SemanticError::Store("Missing 'symbol_name' StringArray column in LanceDB batch".to_string()))?;
        let titles = batch
            .column_by_name("title")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>())
            .ok_or_else(|| SemanticError::Store("Missing 'title' StringArray column in LanceDB batch".to_string()))?;
        let previews = batch
            .column_by_name("content_preview")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>())
            .ok_or_else(|| SemanticError::Store("Missing 'content_preview' StringArray column in LanceDB batch".to_string()))?;
        let file_paths = batch
            .column_by_name("file_path")
            .and_then(|c| c.as_any().downcast_ref::<StringArray>())
            .ok_or_else(|| SemanticError::Store("Missing 'file_path' StringArray column in LanceDB batch".to_string()))?;
        let line_starts = batch
            .column_by_name("line_start")
            .and_then(|c| c.as_any().downcast_ref::<UInt32Array>())
            .ok_or_else(|| SemanticError::Store("Missing 'line_start' UInt32Array column in LanceDB batch".to_string()))?;
        let line_ends = batch
            .column_by_name("line_end")
            .and_then(|c| c.as_any().downcast_ref::<UInt32Array>())
            .ok_or_else(|| SemanticError::Store("Missing 'line_end' UInt32Array column in LanceDB batch".to_string()))?;

        for i in 0..count {
            let id = ids.value(i).to_string();
            if id.is_empty() || !seen.insert(id.clone()) {
                continue;
            }
            items.push(StoredChunkMeta {
                id,
                entity_type: entity_types.value(i).to_string(),
                symbol_name: symbol_names.value(i).to_string(),
                title: titles.value(i).to_string(),
                content_preview: previews.value(i).to_string(),
                file_path: file_paths.value(i).to_string(),
                line_start: line_starts.value(i),
                line_end: line_ends.value(i),
            });
        }
    }

    Ok(items)
}

/// Fetches `content_preview`, `line_start`, and `line_end` for a list of chunk IDs without deserializing vectors.
pub async fn fetch_chunk_previews_by_ids(
    project_id: i64,
    chunk_ids: &[String],
) -> Result<HashMap<String, (String, u32, u32)>, SemanticError> {
    let mut map = HashMap::new();
    if chunk_ids.is_empty() {
        return Ok(map);
    }

    let table = get_or_create_table().await?;
    for batch_ids in chunk_ids.chunks(DELETE_BATCH_SIZE_COUNT) {
        let quoted: Vec<String> = batch_ids
            .iter()
            .map(|id| format!("'{}'", id.replace('\'', "''")))
            .collect();
        let predicate = format!("project_id = {} AND id IN ({})", project_id, quoted.join(", "));
        let stream = table
            .query()
            .only_if(predicate)
            .select(lancedb::query::Select::columns(&[
                "id",
                "entity_type",
                "symbol_name",
                "title",
                "content_preview",
                "file_path",
                "line_start",
                "line_end",
            ]))
            .execute()
            .await
            .map_err(|e| SemanticError::Store(format!("Failed to query chunk previews by IDs: {}", e)))?;

        let batches: Vec<RecordBatch> = stream
            .try_collect()
            .await
            .map_err(|e| SemanticError::Store(format!("Failed to collect chunk previews by IDs: {}", e)))?;

        for item in collect_chunk_metadata(batches)? {
            map.insert(item.id, (item.content_preview, item.line_start, item.line_end));
        }
    }

    Ok(map)
}
