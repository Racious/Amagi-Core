use tauri::State;
use crate::{AppError, AppState};
use crate::models::workflow::{ProjectWorkflows, WorkflowRun};
use crate::core::{project_manager, workflow_manager};

#[tauri::command]
pub async fn scan_project_workflows(
    project_id: String,
    state: State<'_, AppState>,
) -> Result<ProjectWorkflows, AppError> {
    let data_dir = state.data_dir.clone();
    let project = project_manager::get_project(&project_id, &data_dir)
        .ok_or_else(|| AppError::ProjectNotFound(project_id.clone()))?;
    Ok(workflow_manager::scan_project_workflows(&project_id, &project.path))
}

#[tauri::command]
pub async fn list_all_workflows(
    state: State<'_, AppState>,
) -> Result<Vec<ProjectWorkflows>, AppError> {
    let data_dir = state.data_dir.clone();
    let projects = project_manager::list_projects(&data_dir);
    Ok(projects
        .iter()
        .map(|p| workflow_manager::scan_project_workflows(&p.id, &p.path))
        .filter(|pw| pw.has_workflow_dir || !pw.workflows.is_empty())
        .collect())
}

#[tauri::command]
pub async fn generate_workflow_command(
    runner_path: String,
    workflow_id: String,
    inputs: std::collections::HashMap<String, String>,
    mode: String,
) -> Result<String, AppError> {
    Ok(workflow_manager::generate_run_command(
        &runner_path,
        &workflow_id,
        &inputs,
        &mode,
    ))
}

#[tauri::command]
pub async fn plan_workflow(
    project_id: String,
    runner_path: String,
    workflow_id: String,
    inputs: std::collections::HashMap<String, String>,
    state: State<'_, AppState>,
) -> Result<WorkflowRun, AppError> {
    let data_dir = state.data_dir.clone();
    let project = project_manager::get_project(&project_id, &data_dir)
        .ok_or_else(|| AppError::ProjectNotFound(project_id.clone()))?;
    let mut run = workflow_manager::plan_workflow(
        &runner_path,
        &workflow_id,
        &project.path,
        &inputs,
    )?;
    run.project_id = project_id;
    Ok(run)
}
