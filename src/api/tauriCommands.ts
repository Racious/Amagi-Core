import { invoke } from '@tauri-apps/api/core'

export interface ProjectInfo {
  id: string
  name: string
  path: string
  isGitRepo: boolean
  currentBranch: string | null
  initialized: boolean
  pendingReviewCount: number
}

export interface InitResult {
  projectId: string
  createdDirs: string[]
  createdFiles: string[]
}

export interface ScanResult {
  projectId: string
  branch: string
  statusShort: string
  diffStat: string
  diffText: string
  recentLog: string
  changedFiles: string[]
}

export interface LearnResult {
  projectId: string
  candidatesGenerated: number
  blockedCount: number
  candidateIds: string[]
}

export type ReviewItemType = 'memory' | 'skill' | 'blocked'
export type RiskLevel = 'low' | 'medium' | 'high'
export type ReviewStatus = 'pending' | 'accepted' | 'ignored' | 'synced'

export type SyncScope = 'project' | 'global'

export interface ReviewItem {
  id: string
  projectId: string
  itemType: ReviewItemType
  category: string
  title: string
  content: string
  risk: RiskLevel
  status: ReviewStatus
  syncTargets: string[]
  syncScope: SyncScope
  sourcePendingFile: string | null
  createdAt: string
  reviewedAt: string | null
}

export interface ReviewApplyResult {
  acceptedIds: string[]
  writtenFiles: string[]
}

export interface ItemConflict {
  itemId: string
  itemTitle: string
  reasons: string[]
}

export interface SyncResult {
  projectId: string
  writtenFiles: string[]
  skippedFiles: string[]
  blockedConflicts: ItemConflict[]
}

export interface FileDiffPreview {
  filePath: string
  currentContent: string | null
  newContent: string
  isNewFile: boolean
}

// ── 差異匯出 相關型別 ──────────────────────────────
export type ChangedStatus = 'modified' | 'added' | 'deleted' | 'renamed' | 'untracked'
export type DiffGroup = 'edited' | 'addedDeleted'

export interface ChangedFile {
  path: string
  status: ChangedStatus
  group: DiffGroup
  staged: boolean
}

export interface DiffBundle {
  editedPatch: string
  addedDeletedPatch: string
  skipped: string[]
  truncated: boolean
}

// ── Workflow 相關型別 ──────────────────────────────
export interface WorkflowInput {
  key: string
  label: string
  required: boolean
  defaultValue: string | null
}

export interface WorkflowStep {
  id: string
  name: string
  description: string
  badge: string | null
  requiresStop: boolean
}

export interface WorkflowDefinition {
  id: string
  name: string
  description: string
  steps: WorkflowStep[]
  inputs: WorkflowInput[]
}

export interface ProjectWorkflows {
  projectId: string
  projectName: string
  projectPath: string
  hasWorkflowDir: boolean
  runnerPath: string | null
  workflows: WorkflowDefinition[]
}

export interface WorkflowRun {
  id: string
  projectId: string
  workflowId: string
  workflowName: string
  inputs: Record<string, string>
  status: 'planning' | 'running' | 'stopped' | 'done' | 'failed'
  log: string[]
  startedAt: string
  finishedAt: string | null
}

// ── File Bridge 相關型別 ──────────────────────────
export type BridgeRunStatus = 'awaitingResult' | 'done' | 'cancelled'
export type BridgeStepStatus = 'pending' | 'active' | 'done'

export interface BridgeStep {
  id: string
  name: string
  instruction: string
  status: BridgeStepStatus
  result: string | null
}

export interface BridgeRun {
  id: string
  projectId: string
  projectPath: string
  workflowId: string
  workflowName: string
  task: string
  steps: BridgeStep[]
  currentStep: number
  status: BridgeRunStatus
  createdAt: string
  updatedAt: string
}

export const api = {
  addProject: (path: string) => invoke<ProjectInfo>('add_project', { path }),
  initProject: (projectId: string) => invoke<InitResult>('init_project', { projectId }),
  listProjects: () => invoke<ProjectInfo[]>('list_projects'),
  removeProject: (projectId: string) => invoke<void>('remove_project', { projectId }),

  scanProject: (projectId: string) => invoke<ScanResult>('scan_project', { projectId }),
  learnFromProject: (projectId: string) => invoke<LearnResult>('learn_from_project', { projectId }),

  listReviewItems: (projectId?: string) => invoke<ReviewItem[]>('list_review_items', { projectId: projectId ?? null }),
  acceptReviewItems: (ids: string[]) => invoke<ReviewApplyResult>('accept_review_items', { ids }),
  ignoreReviewItems: (ids: string[]) => invoke<void>('ignore_review_items', { ids }),
  updateReviewItem: (item: ReviewItem) => invoke<ReviewItem>('update_review_item', { item }),

  syncAgentFiles: (projectId: string, force = false) => invoke<SyncResult>('sync_agent_files', { projectId, force }),
  previewSyncDiff: (projectId: string) => invoke<FileDiffPreview[]>('preview_sync_diff', { projectId }),

  // ── 差異匯出 ──────────────────────────────────────
  listChangedFiles: (projectId: string) => invoke<ChangedFile[]>('list_changed_files', { projectId }),
  generateDiffText: (projectId: string, paths: string[]) =>
    invoke<DiffBundle>('generate_diff_text', { projectId, paths }),

  // ── Workflow 指令 ──────────────────────────────
  scanProjectWorkflows: (projectId: string) =>
    invoke<ProjectWorkflows>('scan_project_workflows', { projectId }),
  listAllWorkflows: () =>
    invoke<ProjectWorkflows[]>('list_all_workflows'),
  generateWorkflowCommand: (
    runnerPath: string,
    workflowId: string,
    inputs: Record<string, string>,
    mode: string,
  ) => invoke<string>('generate_workflow_command', { runnerPath, workflowId, inputs, mode }),
  planWorkflow: (
    projectId: string,
    runnerPath: string,
    workflowId: string,
    inputs: Record<string, string>,
  ) => invoke<WorkflowRun>('plan_workflow', { projectId, runnerPath, workflowId, inputs }),

  // ── File Bridge 指令 ──────────────────────────────
  startBridgeRun: (projectId: string, workflowId: string, task: string) =>
    invoke<BridgeRun>('start_bridge_run', { projectId, workflowId, task }),
  advanceBridgeRun: (projectId: string) =>
    invoke<BridgeRun>('advance_bridge_run', { projectId }),
  getBridgeRun: (projectId: string) =>
    invoke<BridgeRun | null>('get_bridge_run', { projectId }),
  cancelBridgeRun: (projectId: string) =>
    invoke<void>('cancel_bridge_run', { projectId }),
}
