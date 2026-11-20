//! Aggregated summary and customizable widget layout for the Project Overview dashboard (Spec 37).

use std::collections::HashMap;
use std::path::Path;

use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::api::ApiError;
use crate::db::DatabaseError;
use crate::registry::RegistryError;

const OVERVIEW_GRID_COLS_COUNT: u32 = 4;
const OVERVIEW_GRID_MAX_H_ROWS: u32 = 3;

/// Errors returned by Overview dashboard commands.
#[derive(Debug, thiserror::Error)]
pub enum OverviewError {
    #[error(transparent)]
    Registry(#[from] RegistryError),
    #[error(transparent)]
    Database(#[from] DatabaseError),
    #[error("Invalid overview widget layout: {0}")]
    InvalidLayout(String),
    #[error("Overview background task failed: {0}")]
    Task(String),
}

impl ApiError for OverviewError {
    fn status(&self) -> StatusCode {
        match self {
            Self::Registry(e) => e.status(),
            Self::InvalidLayout(_) => StatusCode::BAD_REQUEST,
            Self::Database(_) | Self::Task(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn code(&self) -> &'static str {
        match self {
            Self::Registry(e) => e.code(),
            Self::Database(_) => "OverviewError::Database",
            Self::InvalidLayout(_) => "OverviewError::InvalidLayout",
            Self::Task(_) => "OverviewError::Task",
        }
    }
}

/// Persisted grid geometry and visibility for a single Overview widget (4-column grid).
#[derive(Serialize, Deserialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OverviewWidgetLayoutItem {
    pub id: String,
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
    pub visible: bool,
}

/// Arguments for `save_project_overview_layout`.
#[derive(Deserialize, Debug, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[ts(export)]
pub struct SaveProjectOverviewLayoutArgs {
    #[ts(type = "number")]
    pub project_id: usize,
    pub layout: Vec<OverviewWidgetLayoutItem>,
}

/// Per-stage task count breakdown for the Project Management widget.
#[derive(Serialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OverviewStageCount {
    pub slug: String,
    pub label: String,
    #[ts(type = "number")]
    pub count: usize,
    pub is_terminal: bool,
}

/// Summary metrics for the Project Management widget.
#[derive(Serialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OverviewPmSummary {
    #[ts(type = "number")]
    pub active_tasks_count: usize,
    #[ts(type = "number")]
    pub overdue_tasks_count: usize,
    pub stages: Vec<OverviewStageCount>,
    pub tracking_mode: String,
    #[ts(type = "number")]
    pub milestones_count: usize,
    pub overall_progress_percent: f64,
    #[ts(type = "number")]
    pub drift_warnings_count: usize,
}

/// Summary metrics for the Forge (GitHub / GitLab) widget.
#[derive(Serialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OverviewForgeSummary {
    #[ts(type = "number")]
    pub unread_count: usize,
    #[ts(type = "number")]
    pub flagged_count: usize,
    #[ts(type = "number")]
    pub tracked_items_count: usize,
    pub provider: Option<String>,
}

/// Summary metrics for the Semantic Knowledge widget (Technical Debt #40).
#[derive(Serialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OverviewKnowledgeSummary {
    #[ts(type = "number")]
    pub indexed_chunks_count: usize,
    #[ts(type = "number")]
    pub code_chunks_count: usize,
    #[ts(type = "number")]
    pub spec_chunks_count: usize,
    #[ts(type = "number")]
    pub entity_chunks_count: usize,
    #[ts(type = "number")]
    pub graph_edges_count: usize,
    pub auto_index_enabled: bool,
    #[ts(type = "number")]
    pub min_score_percent: usize,
    pub is_indexing: bool,
    #[ts(type = "number | null")]
    pub last_duration_ms: Option<u64>,
}

/// Summary metrics for the Devlog / Session Journal widget.
#[derive(Serialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OverviewJournalSummary {
    #[ts(type = "number")]
    pub total_entries_count: usize,
    pub latest_title: Option<String>,
    pub latest_agent_id: Option<String>,
    #[ts(type = "number | null")]
    pub latest_created_at_seconds: Option<i64>,
}

/// Summary metrics for the Profiler widget.
#[derive(Serialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OverviewProfilerSummary {
    #[ts(type = "number")]
    pub runs_count: usize,
    pub latest_target_id: Option<String>,
    pub latest_status: Option<String>,
    #[ts(type = "number | null")]
    pub latest_duration_ms: Option<i64>,
}

/// Summary metrics for domain asset workspaces (`desigma`, `solidcraft`, `pcbcraft`).
#[derive(Serialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct OverviewArtifactDomainSummary {
    #[ts(type = "number")]
    pub files_count: usize,
    pub is_editor_running: bool,
}

/// Aggregated overview payload returned by `get_project_overview_summary`.
#[derive(Serialize, Clone, Debug, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ProjectOverviewSummary {
    #[ts(type = "number")]
    pub project_id: usize,
    pub layout: Vec<OverviewWidgetLayoutItem>,
    pub project_mgmt: Option<OverviewPmSummary>,
    pub forge: Option<OverviewForgeSummary>,
    pub knowledge: OverviewKnowledgeSummary,
    pub journal: OverviewJournalSummary,
    pub profiler: Option<OverviewProfilerSummary>,
    pub desigma: Option<OverviewArtifactDomainSummary>,
    pub solidcraft: Option<OverviewArtifactDomainSummary>,
    pub pcbcraft: Option<OverviewArtifactDomainSummary>,
}

fn gather_blocking_overview(
    project_id: usize,
    project_path: String,
    pm_enabled: bool,
    profiler_enabled: bool,
    has_git: bool,
) -> Result<
    (
        Vec<OverviewWidgetLayoutItem>,
        Option<OverviewPmSummary>,
        Option<OverviewForgeSummary>,
        OverviewJournalSummary,
        Option<OverviewProfilerSummary>,
    ),
    OverviewError,
> {
    let pid = project_id as i64;
    let layout = match crate::db::db_get_project_overview_layout(pid)? {
        Some(raw) if !raw.trim().is_empty() => serde_json::from_str(&raw).unwrap_or_default(),
        _ => Vec::new(),
    };

    let now_secs = crate::db::now_epoch_secs();
    let project_mgmt = if pm_enabled {
        let stages_db = crate::db::db_list_project_stages(pid)?;
        let tasks_db = crate::db::db_list_project_tasks(pid, None)?;
        let mut counts_by_slug: HashMap<String, usize> = HashMap::new();
        let mut terminal_slugs: HashMap<String, bool> = HashMap::new();
        for st in &stages_db {
            terminal_slugs.insert(st.slug.clone(), st.is_terminal);
        }
        let mut active_tasks_count = 0usize;
        let mut overdue_tasks_count = 0usize;
        for bundle in &tasks_db {
            *counts_by_slug.entry(bundle.task.status.clone()).or_insert(0) += 1;
            let is_term = terminal_slugs.get(&bundle.task.status).copied().unwrap_or(false);
            if !is_term {
                active_tasks_count += 1;
                if bundle.task.due_epoch_seconds.is_some_and(|due| due < now_secs) {
                    overdue_tasks_count += 1;
                }
            }
        }
        let stages = stages_db
            .into_iter()
            .map(|st| OverviewStageCount {
                count: counts_by_slug.get(&st.slug).copied().unwrap_or(0),
                slug: st.slug,
                label: st.label,
                is_terminal: st.is_terminal,
            })
            .collect();
        let tracking_mode = crate::kopeng::project_mgmt::resolve_tracking_mode(Path::new(&project_path))
            .map_err(|e| OverviewError::Task(e.to_string()))?;
        let milestones_db = crate::db::db_list_project_milestones(pid)?;
        let (overall_progress_percent, _, milestone_summaries) =
            crate::kopeng::project_mgmt::compute_milestone_summaries(pid, &tracking_mode, milestones_db)
                .map_err(|e| OverviewError::Task(e.to_string()))?;
        let milestones_count = milestone_summaries.len();
        let drift_warnings_count = milestone_summaries
            .iter()
            .filter(|s| {
                s.schedule_drift_days.is_some_and(|d| d > 0)
                    || s.uphill_risk_flag
                    || s.critical_tech_debt_count > 0
            })
            .count();
        Some(OverviewPmSummary {
            active_tasks_count,
            overdue_tasks_count,
            stages,
            tracking_mode,
            milestones_count,
            overall_progress_percent,
            drift_warnings_count,
        })
    } else {
        None
    };

    let forge = if has_git {
        let rows = crate::db::db_list_forge_inbox_items(pid, None)?;
        let tracked_items_count = rows.len();
        let unread_count = rows.iter().filter(|r| !r.is_read).count();
        let flagged_count = rows
            .iter()
            .filter(|r| r.triage_flag.as_deref() == Some("flagged"))
            .count();
        let provider = rows.first().map(|r| r.provider.clone());
        Some(OverviewForgeSummary {
            unread_count,
            flagged_count,
            tracked_items_count,
            provider,
        })
    } else {
        None
    };

    let journals = crate::db::db_get_project_session_journals(pid, None, 200)?;
    let latest_journal = journals.first();
    let journal = OverviewJournalSummary {
        total_entries_count: journals.len(),
        latest_title: latest_journal.map(|j| j.title.clone()),
        latest_agent_id: latest_journal.map(|j| j.agent_id.clone()),
        latest_created_at_seconds: latest_journal.map(|j| j.created_at),
    };

    let profiler = if profiler_enabled {
        let runs = crate::db::db_get_target_runs(pid, None, 200)?;
        let latest = runs.first();
        let latest_duration_ms = latest.and_then(|r| {
            r.terminated_at_ms
                .map(|end| end.saturating_sub(r.spawned_at_ms))
        });
        Some(OverviewProfilerSummary {
            runs_count: runs.len(),
            latest_target_id: latest.map(|r| r.target_id.clone()),
            latest_status: latest.map(|r| r.status.clone()),
            latest_duration_ms,
        })
    } else {
        None
    };

    let _ = project_path;
    Ok((layout, project_mgmt, forge, journal, profiler))
}

/// Gathers aggregated subsystem metrics and the persisted widget layout for a project.
pub async fn get_project_overview_summary(
    project_id: usize,
) -> Result<ProjectOverviewSummary, OverviewError> {
    let project = tokio::task::spawn_blocking(move || crate::registry::find_project_by_id(project_id))
        .await
        .map_err(|e| OverviewError::Task(e.to_string()))??;

    let p_path = Path::new(&project.path);
    let pm_enabled =
        crate::config::resolve_effective_bool(Some(&project.path), None, "project_management", "enabled");
    let profiler_enabled =
        crate::config::resolve_effective_bool(Some(&project.path), None, "profiler", "enabled");
    let desigma_enabled =
        crate::config::resolve_effective_bool(Some(&project.path), None, "desigma", "enabled");
    let solidcraft_enabled =
        crate::config::resolve_effective_bool(Some(&project.path), None, "solidcraft", "enabled");
    let pcbcraft_enabled =
        crate::config::resolve_effective_bool(Some(&project.path), None, "pcbcraft", "enabled");
    let has_git = p_path.join(".git").exists();

    let proj_path_clone = project.path.clone();
    let (layout, project_mgmt, forge, journal, profiler) = tokio::task::spawn_blocking(move || {
        gather_blocking_overview(project_id, proj_path_clone, pm_enabled, profiler_enabled, has_git)
    })
    .await
    .map_err(|e| OverviewError::Task(e.to_string()))??;

    let knowledge_status = crate::semantic::get_indexing_status(project_id as i64).await;
    let knowledge = match knowledge_status {
        Ok(st) => OverviewKnowledgeSummary {
            indexed_chunks_count: st.total_records_count,
            code_chunks_count: st.code_chunks_count,
            spec_chunks_count: st.spec_chunks_count,
            entity_chunks_count: st.entity_chunks_count,
            graph_edges_count: st.graph_edges_count,
            auto_index_enabled: st.auto_index_enabled,
            min_score_percent: st.min_score_percent,
            is_indexing: st.is_indexing,
            last_duration_ms: st.last_duration_ms,
        },
        Err(_) => OverviewKnowledgeSummary {
            indexed_chunks_count: 0,
            code_chunks_count: 0,
            spec_chunks_count: 0,
            entity_chunks_count: 0,
            graph_edges_count: 0,
            auto_index_enabled: true,
            min_score_percent: 70,
            is_indexing: false,
            last_duration_ms: None,
        },
    };

    let desigma = if desigma_enabled {
        crate::kopeng::desigma::get_project_desigma_status(project.path.clone())
            .await
            .ok()
            .map(|st| OverviewArtifactDomainSummary {
                files_count: st.design_files_count,
                is_editor_running: st.is_editor_running,
            })
    } else {
        None
    };

    let solidcraft = if solidcraft_enabled {
        crate::kopeng::solidcraft::get_project_solidcraft_status(project.path.clone())
            .await
            .ok()
            .map(|st| OverviewArtifactDomainSummary {
                files_count: st.cad_files_count,
                is_editor_running: st.is_editor_running,
            })
    } else {
        None
    };

    let pcbcraft = if pcbcraft_enabled {
        crate::kopeng::pcbcraft::get_project_pcbcraft_status(project.path.clone())
            .await
            .ok()
            .map(|st| OverviewArtifactDomainSummary {
                files_count: st.pcb_targets_count,
                is_editor_running: st.is_editor_running,
            })
    } else {
        None
    };

    Ok(ProjectOverviewSummary {
        project_id,
        layout,
        project_mgmt,
        forge,
        knowledge,
        journal,
        profiler,
        desigma,
        solidcraft,
        pcbcraft,
    })
}

/// Validates and persists the 4-column Overview Dashboard widget layout for a project.
pub fn save_project_overview_layout(
    args: SaveProjectOverviewLayoutArgs,
) -> Result<(), OverviewError> {
    let project = crate::registry::find_project_by_id(args.project_id)?;
    for item in &args.layout {
        if item.id.trim().is_empty() {
            return Err(OverviewError::InvalidLayout("widget id must not be empty".into()));
        }
        if item.w == 0 || item.w > OVERVIEW_GRID_COLS_COUNT {
            return Err(OverviewError::InvalidLayout(format!(
                "widget '{}' width {} must be between 1 and {}",
                item.id, item.w, OVERVIEW_GRID_COLS_COUNT
            )));
        }
        if item.h == 0 || item.h > OVERVIEW_GRID_MAX_H_ROWS {
            return Err(OverviewError::InvalidLayout(format!(
                "widget '{}' height {} must be between 1 and {}",
                item.id, item.h, OVERVIEW_GRID_MAX_H_ROWS
            )));
        }
        if item.x >= OVERVIEW_GRID_COLS_COUNT {
            return Err(OverviewError::InvalidLayout(format!(
                "widget '{}' x={} exceeds {}-column grid",
                item.id, item.x, OVERVIEW_GRID_COLS_COUNT
            )));
        }
    }
    let json = serde_json::to_string(&args.layout)
        .map_err(|e| OverviewError::InvalidLayout(e.to_string()))?;
    crate::db::db_save_project_overview_layout(project.id as i64, &json)?;
    Ok(())
}
