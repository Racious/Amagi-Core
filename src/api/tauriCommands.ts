import { invoke } from '@tauri-apps/api/core'

export interface ProjectInfo {
  id: string
  name: string
  path: string
  isGitRepo: boolean
  currentBranch: string | null
  initialized: boolean
  pendingReviewCount: number
  vaultFolder: string | null
  /** 專案目錄是否仍可作為分發目標（後端 is_dir）。false → 不存在或非目錄（幽靈專案）。 */
  pathExists: boolean
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

export type ReviewItemType = 'memory' | 'skill' | 'blocked' | 'wiki'
export type RiskLevel = 'low' | 'medium' | 'high'
export type ReviewStatus = 'pending' | 'accepted' | 'ignored' | 'synced'

export type SyncScope = 'project' | 'shared' | 'global'

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

// ── Vault 知識庫 相關型別 ──────────────────────────
export interface VaultConfig {
  vaultPath: string | null
  pointerWritten: boolean
}

export interface VaultStatus {
  /** 是否已設定 vault 路徑（未設 → 首次啟動引導）。 */
  configured: boolean
  vaultPath: string | null
  /** vault 是否已是 git repo（未掛 → 強烈建議掛 git 保命）。 */
  isGitRepo: boolean
}

export interface VaultSetResult {
  vaultPath: string
  looksLikeVault: boolean
  claudeMdPath: string
  backupMade: boolean
  pointerAction: 'appended' | 'replaced'
}

export interface WikiIngestInput {
  projectId: string
  layer: string
  pageType: string
  title: string
  content: string
}

export interface WikiWriteResult {
  written: string[]
  skipped: string[]
}

export interface LibrarySkill {
  slug: string
  name: string
}

export interface DistributeResult {
  skillCount: number
  repoCount: number
  writtenCount: number
  /** 磁碟目錄已不存在、被略過分發的目標路徑（如幽靈專案）。 */
  invalidTargets: string[]
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
  /** Phase 3b-2：把一筆已同步的專案層記憶升級為跨專案共用（移到 vault shared/agent/memory）。 */
  promoteMemory: (itemId: string) => invoke<void>('promote_memory', { itemId }),

  // ── 差異匯出 ──────────────────────────────────────
  listChangedFiles: (projectId: string) => invoke<ChangedFile[]>('list_changed_files', { projectId }),
  generateDiffText: (projectId: string, paths: string[]) =>
    invoke<DiffBundle>('generate_diff_text', { projectId, paths }),
  revealInExplorer: (projectId: string, relPath?: string) =>
    invoke<void>('reveal_in_explorer', { projectId, relPath: relPath ?? null }),

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

  // ── Vault 知識庫 ──────────────────────────────────
  getVaultConfig: () => invoke<VaultConfig>('get_vault_config'),
  getVaultStatus: () => invoke<VaultStatus>('get_vault_status'),
  setVaultPath: (path: string) => invoke<VaultSetResult>('set_vault_path', { path }),
  vaultGitStatus: () => invoke<string>('vault_git_status'),
  vaultGitPull: () => invoke<string>('vault_git_pull'),
  vaultGitSync: (message?: string) => invoke<string>('vault_git_sync', { message: message ?? null }),
  initProjectVault: (projectId: string) => invoke<InitResult>('init_project_vault', { projectId }),

  // ── 知識匯入（Wiki）──────────────────────────────
  ingestWikiPage: (input: WikiIngestInput) => invoke<ReviewItem>('ingest_wiki_page', { ...input }),
  ingestWikiFromFile: (input: { projectId: string; layer: string; pageType: string; filePath: string }) =>
    invoke<ReviewItem>('ingest_wiki_from_file', { ...input }),
  scanVaultClips: () => invoke<number>('scan_vault_clips'),
  writeWikiPages: (ids: string[]) => invoke<WikiWriteResult>('write_wiki_pages', { ids }),

  // ── 技能庫 ────────────────────────────────────────
  listLibrarySkills: () => invoke<LibrarySkill[]>('list_library_skills'),
  distributeSkillsSelective: (selections: { skillSlug: string; target: string }[]) =>
    invoke<DistributeResult>('distribute_skills_selective', { selections }),
}
