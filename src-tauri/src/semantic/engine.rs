//! Local embedding engine executing BGE-M3 (1024 dimensions) via ONNX Runtime.

use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock};
use fastembed::{Bgem3Embedding, Bgem3InitOptions, Bgem3Model};

use crate::semantic::errors::SemanticError;

/// Global thread-safe handle holding the loaded BGE-M3 embedding model instance.
static EMBEDDER_INSTANCE: OnceLock<Arc<Mutex<Option<Bgem3Embedding>>>> = OnceLock::new();

/// Returns the platform model cache directory (`~/.cache/one-ship/models/`).
fn get_model_cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("one-ship")
        .join("models")
}

/// Retrieves or initializes the shared BGE-M3 embedding instance.
fn get_embedder_holder() -> &'static Arc<Mutex<Option<Bgem3Embedding>>> {
    EMBEDDER_INSTANCE.get_or_init(|| Arc::new(Mutex::new(None)))
}

/// Acquires the embedder mutex, recovering from poison instead of permanently failing (AGENTS.md §9.1).
fn lock_embedder(context: &str) -> std::sync::MutexGuard<'static, Option<Bgem3Embedding>> {
    get_embedder_holder().lock().unwrap_or_else(|poisoned| {
        crate::logger::log("WARN", "semantic", &format!("Embedder mutex poisoned ({}), recovering inner guard", context));
        poisoned.into_inner()
    })
}

/// Lazily loads the BGE-M3 ONNX model into memory if not already initialized.
pub fn ensure_model_loaded() -> Result<(), SemanticError> {
    let mut guard = lock_embedder("ensure_model_loaded");

    if guard.is_some() {
        return Ok(());
    }

    let cache_dir = get_model_cache_dir();
    if let Err(e) = std::fs::create_dir_all(&cache_dir) {
        crate::logger::log(
            "WARN",
            "semantic",
            &format!("Failed to create model cache directory {:?}: {}", cache_dir, e),
        );
    }

    crate::logger::log(
        "INFO",
        "semantic",
        &format!("Initializing BGE-M3 local embedding model in {:?}", cache_dir),
    );

    let intra_threads = crate::config::resolve_effective_usize(
        None,
        None,
        "semantic",
        "onnx_intra_threads_count",
    )
    .clamp(1, 32);

    let options = Bgem3InitOptions::new(Bgem3Model::BGEM3Q)
        .with_cache_dir(cache_dir)
        .with_intra_threads(intra_threads);

    let embedder = Bgem3Embedding::try_new(options)
        .map_err(|e| SemanticError::EngineInit(format!("Failed to load BGE-M3 ONNX model: {}", e)))?;

    *guard = Some(embedder);
    crate::logger::log("INFO", "semantic", "BGE-M3 local embedding model initialized successfully");
    Ok(())
}

/// Computes a 1024-dimension dense vector for a single text string.
pub fn embed_single(text: &str) -> Result<Vec<f32>, SemanticError> {
    ensure_model_loaded()?;
    let mut guard = lock_embedder("embed_single");

    let embedder = guard.as_mut().ok_or_else(|| {
        SemanticError::EngineInit("Embedder instance missing after ensure_model_loaded".to_string())
    })?;

    let output = embedder
        .embed(vec![text], Some(1))
        .map_err(|e| SemanticError::EmbeddingFailed(format!("Inference failure: {}", e)))?;

    output.dense.into_iter().next().ok_or_else(|| {
        SemanticError::EmbeddingFailed("No dense embedding returned for input text".to_string())
    })
}

/// Computes 1024-dimension dense vectors for a batch of text chunks.
pub fn embed_batch(texts: &[String]) -> Result<Vec<Vec<f32>>, SemanticError> {
    if texts.is_empty() {
        return Ok(Vec::new());
    }

    ensure_model_loaded()?;
    let mut guard = lock_embedder("embed_batch");

    let embedder = guard.as_mut().ok_or_else(|| {
        SemanticError::EngineInit("Embedder instance missing after ensure_model_loaded".to_string())
    })?;

    let output = embedder
        .embed(texts, Some(8))
        .map_err(|e| SemanticError::EmbeddingFailed(format!("Batch inference failure: {}", e)))?;

    Ok(output.dense)
}
