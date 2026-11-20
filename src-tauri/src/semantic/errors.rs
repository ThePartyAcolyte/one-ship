//! Strongly-typed domain error hierarchy for transversal semantic search.

use axum::http::StatusCode;
use thiserror::Error;

/// Errors emitted by the semantic search engine, vector store, and chunking pipeline.
#[derive(Debug, Error)]
pub enum SemanticError {
    #[error("Semantic search engine initialization failed: {0}")]
    EngineInit(String),

    #[error("Failed to generate embedding for input: {0}")]
    EmbeddingFailed(String),

    #[error("Vector database error (LanceDB): {0}")]
    Store(String),

    #[error("Background semantic indexing throttled: {0}")]
    Throttled(String),

    #[error("Invalid semantic search request: {0}")]
    InvalidRequest(String),

    #[error("Database error during semantic entity retrieval: {0}")]
    Database(#[from] crate::db::DatabaseError),

    #[error("Filesystem I/O error during chunking: {0}")]
    Io(#[from] std::io::Error),
}

impl crate::api::ApiError for SemanticError {
    fn status(&self) -> StatusCode {
        match self {
            Self::InvalidRequest(_) => StatusCode::BAD_REQUEST,
            Self::Throttled(_) => StatusCode::TOO_MANY_REQUESTS,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::EngineInit(_) => "SemanticError::EngineInit",
            Self::EmbeddingFailed(_) => "SemanticError::EmbeddingFailed",
            Self::Store(_) => "SemanticError::Store",
            Self::Throttled(_) => "SemanticError::Throttled",
            Self::InvalidRequest(_) => "SemanticError::InvalidRequest",
            Self::Database(_) => "SemanticError::Database",
            Self::Io(_) => "SemanticError::Io",
        }
    }
}
