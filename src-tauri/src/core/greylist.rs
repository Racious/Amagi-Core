//! 封鎖項丟棄灰名單（adr-007）：「誤判，不再提醒」的持久化壓制清單。
//!
//! - 權威源＝vault `projects/<folder>/agent/blocked-greylist.json`（vault-first：
//!   git 同步＝跨機同步、刪檔＝全部解除）。只存遮罩值與 digest，無明文機密。
//! - key＝`(file_path, rule_label, value_digest)`——與 D2 同粒度；digest 為
//!   正規化後完整命中字串的 SHA-256（`safety_filter::value_digest`）。
//! - fail 語意：**產卡端**讀失敗→空（寧吵不漏）；**寫端**讀失敗→Err（不得在
//!   損壞檔上覆寫）；**檢視端**讀失敗→Err（UI 顯示「靜音效果已暫停」警示）。
//! - 同機並發：寫入端以 process-wide `Mutex` 序列化（read-modify-write 防
//!   lost-update）；讀端無鎖——讀到舊快照＝多出一張卡，安全側可容忍。

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};

use crate::models::review::BlockedHit;
use crate::utils::json_store;
use crate::AppError;

pub const GREYLIST_VERSION: u32 = 1;
pub const GREYLIST_SCOPE_EXACT: &str = "exact";

/// 灰名單條目：壓制身分（key 三元組）＋顯示（masked）＋最低限度稽核欄。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GreylistEntry {
    pub file_path: Option<String>,
    pub rule_label: String,
    pub value_digest: String,
    pub masked: String,
    /// v1 固定 "exact"；欄位預留 file-rule 擴充（adr-007 D2）。
    pub scope: String,
    /// 稽核：丟棄當下的卡 id（卡已出列、不可解引用，僅供比對紀錄）與原卡產生時間。
    pub source_item_id: String,
    pub source_created_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GreylistData {
    pub version: u32,
    pub entries: Vec<GreylistEntry>,
}

impl Default for GreylistData {
    fn default() -> Self {
        Self { version: GREYLIST_VERSION, entries: Vec::new() }
    }
}

/// 壓制身分鍵：`(file_path, rule_label, value_digest)`。
pub type GreylistKey = (Option<String>, String, String);
pub type GreylistKeySet = HashSet<GreylistKey>;

pub fn entry_key(e: &GreylistEntry) -> GreylistKey {
    (e.file_path.clone(), e.rule_label.clone(), e.value_digest.clone())
}

pub fn hit_key(h: &BlockedHit) -> GreylistKey {
    (h.file_path.clone(), h.rule_label.clone(), h.value_digest.clone())
}

/// 解析灰名單落點（adr-007 D3）：`vault_root / <resolved_project_folder> / agent / blocked-greylist.json`。
/// `project_folder` 須為呼叫端以 `Project.vault_folder`（缺值 fallback
/// `agent_exporter::project_vault_folder`）解析出的相對邏輯路徑。安全閘兩關（impl-review 發現 1）：
/// ① 形狀：與記憶寫入閘同源（`is_safe_project_vault_folder`：相對、全 Normal、首段 `projects`），
///    另要求至少含 slug（拒絕裸 `projects`）——灰名單是權威源，不得寫入 shared/daily 等他 bucket；
/// ② canonical containment：對目標「最深既存祖先」canonicalize，須落在 canonical vault_root 之下
///   （擋 vault 內既存 symlink/junction 逃逸）；vault_root 尚不存在（首次建立）→ 形狀已驗、
///    下方皆為待建乾淨段，放行。其他 IO 錯誤一律 fail-closed。
pub fn resolve_greylist_path(vault_root: &str, project_folder: &str) -> Result<PathBuf, AppError> {
    let folder = project_folder.trim();
    let p = Path::new(folder);
    let shape_ok = crate::core::agent_exporter::is_safe_project_vault_folder(folder)
        && p.components().count() >= 2;
    if !shape_ok {
        return Err(AppError::InvalidPath(format!(
            "不安全的專案 vault 資料夾（須為 projects/<slug> 形）：{project_folder}"
        )));
    }
    let target = Path::new(vault_root).join(p).join("agent").join("blocked-greylist.json");
    let mut anc: &Path = target.as_path();
    let canon_ancestor = loop {
        match anc.canonicalize() {
            Ok(c) => break c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                anc = anc
                    .parent()
                    .ok_or_else(|| AppError::InvalidPath("灰名單路徑無有效既存祖先".into()))?;
            }
            Err(e) => return Err(AppError::Io(e.to_string())),
        }
    };
    match Path::new(vault_root).canonicalize() {
        Ok(croot) if canon_ancestor.starts_with(&croot) => Ok(target),
        Ok(_) => Err(AppError::InvalidPath(format!(
            "灰名單路徑逃逸出 vault（symlink/junction？）：{}",
            target.display()
        ))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(target),
        Err(e) => Err(AppError::Io(e.to_string())),
    }
}

/// 產卡端讀取（lenient）：檔不存在或解析失敗一律回空集合——寧吵不漏。
pub fn read_keys_lenient(path: &Path) -> GreylistKeySet {
    read_data(path)
        .map(|d| d.entries.iter().map(entry_key).collect())
        .unwrap_or_default()
}

/// 嚴格讀取：檔不存在＝空清單（合法初始態）；存在但讀取/解析失敗 → Err
/// （檢視端據此顯示「靜音效果已暫停」；寫端據此中止，不得在損壞檔上覆寫）。
pub fn read_data(path: &Path) -> Result<GreylistData, AppError> {
    if !path.exists() {
        return Ok(GreylistData::default());
    }
    let raw = std::fs::read_to_string(path).map_err(|e| AppError::Io(e.to_string()))?;
    serde_json::from_str(&raw)
        .map_err(|e| AppError::Io(format!("灰名單檔解析失敗（{}）：{}", path.display(), e)))
}

/// 寫入端 process-wide 鎖：三支寫入口（append/remove）共用，序列化 read-modify-write。
static WRITE_LOCK: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

fn write_sorted(path: &Path, mut data: GreylistData) -> Result<(), AppError> {
    // 按 key 排序寫入：輸出確定性，收斂跨機 git merge 衝突面（adr-007 §9 已決 2）
    data.entries.sort_by(|a, b| entry_key(a).cmp(&entry_key(b)));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| AppError::Io(e.to_string()))?;
    }
    json_store::write_json(path, &data)
}

/// 追加條目（同 key 去重）：回傳實際新增筆數。寫失敗 → Err（呼叫端不得出列卡片）。
pub fn append_entries(path: &Path, new_entries: Vec<GreylistEntry>) -> Result<usize, AppError> {
    let _guard = WRITE_LOCK.lock().map_err(|_| AppError::Io("灰名單寫入鎖失效".into()))?;
    let mut data = read_data(path)?;
    let mut existing: GreylistKeySet = data.entries.iter().map(entry_key).collect();
    let mut added = 0usize;
    for e in new_entries {
        if existing.insert(entry_key(&e)) {
            data.entries.push(e);
            added += 1;
        }
    }
    write_sorted(path, data)?;
    Ok(added)
}

/// 解除條目（按 key 移除）：回傳實際移除筆數；下次學習該值照常出卡。
pub fn remove_entries(path: &Path, keys: &[GreylistKey]) -> Result<usize, AppError> {
    let _guard = WRITE_LOCK.lock().map_err(|_| AppError::Io("灰名單寫入鎖失效".into()))?;
    let mut data = read_data(path)?;
    let target: GreylistKeySet = keys.iter().cloned().collect();
    let before = data.entries.len();
    data.entries.retain(|e| !target.contains(&entry_key(e)));
    let removed = before - data.entries.len();
    if removed > 0 {
        write_sorted(path, data)?;
    }
    Ok(removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> PathBuf {
        std::env::temp_dir()
            .join(format!("amagi-greylist-{}", uuid::Uuid::new_v4()))
            .join("agent")
            .join("blocked-greylist.json")
    }

    fn entry(path: Option<&str>, label: &str, digest: &str) -> GreylistEntry {
        GreylistEntry {
            file_path: path.map(|s| s.to_string()),
            rule_label: label.into(),
            value_digest: digest.into(),
            masked: "ab…yz（共 40 字）".into(),
            scope: GREYLIST_SCOPE_EXACT.into(),
            source_item_id: "card-1".into(),
            source_created_at: Utc::now(),
            created_at: Utc::now(),
        }
    }

    #[test]
    fn test_missing_file_is_empty_everywhere() {
        let p = tmp();
        assert!(read_keys_lenient(&p).is_empty());
        assert!(read_data(&p).unwrap().entries.is_empty(), "檔不存在＝合法空清單");
    }

    #[test]
    fn test_append_dedup_and_remove_roundtrip() {
        let p = tmp();
        let added = append_entries(&p, vec![
            entry(Some("a.md"), "規則A", "d1"),
            entry(Some("a.md"), "規則A", "d1"), // 同 key 去重
            entry(None, "規則B", "d2"),
        ]).unwrap();
        assert_eq!(added, 2);
        let keys = read_keys_lenient(&p);
        assert!(keys.contains(&(Some("a.md".into()), "規則A".into(), "d1".into())));
        assert!(keys.contains(&(None, "規則B".into(), "d2".into())), "file_path=None 照樣參比");

        let removed = remove_entries(&p, &[(None, "規則B".into(), "d2".into())]).unwrap();
        assert_eq!(removed, 1);
        assert_eq!(read_data(&p).unwrap().entries.len(), 1, "解除後條目移除，下次學習重現");
    }

    #[test]
    fn test_malformed_json_lenient_empty_strict_err() {
        let p = tmp();
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, "{not json").unwrap();
        assert!(read_keys_lenient(&p).is_empty(), "產卡端：損壞檔＝空（寧吵不漏）");
        assert!(read_data(&p).is_err(), "檢視/寫端：損壞檔須回明確 Err");
        assert!(append_entries(&p, vec![entry(None, "R", "d")]).is_err(), "寫端不得在損壞檔上覆寫");
    }

    #[test]
    fn test_resolve_path_shape_gate() {
        // vault_root 不存在 → 形狀驗過即放行（首次建立語意）
        assert!(resolve_greylist_path("C:/vault-not-exist", "projects/amagi-core").is_ok());
        assert!(resolve_greylist_path("C:/vault-not-exist", "../escape").is_err(), "拒絕 ..");
        assert!(resolve_greylist_path("C:/vault-not-exist", "").is_err(), "拒絕空值");
        assert!(resolve_greylist_path("C:/vault-not-exist", "C:/abs").is_err(), "拒絕絕對路徑");
        assert!(resolve_greylist_path("C:/vault-not-exist", "shared/foo").is_err(),
            "拒絕非 projects bucket（灰名單不得寫入 shared/daily 等權威層）");
        assert!(resolve_greylist_path("C:/vault-not-exist", "daily/foo").is_err(), "拒絕 daily bucket");
        assert!(resolve_greylist_path("C:/vault-not-exist", "projects").is_err(), "拒絕裸 projects（無 slug）");
    }

    #[test]
    fn test_resolve_path_canonical_containment_with_existing_root() {
        // 既存 vault root：最深既存祖先 canonical 後須落在 canonical root 下
        let root = std::env::temp_dir().join(format!("amagi-gl-root-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("projects").join("demo")).unwrap();
        let root_s = root.to_string_lossy().to_string();
        let ok = resolve_greylist_path(&root_s, "projects/demo").unwrap();
        assert!(ok.ends_with(Path::new("projects/demo/agent/blocked-greylist.json").as_os_str()));
        // 目標尚不存在（agent/ 未建）也應放行——上溯到既存祖先 projects/demo 驗 containment
        let ok2 = resolve_greylist_path(&root_s, "projects/newproj");
        assert!(ok2.is_ok(), "未建目錄以最深既存祖先驗，仍應放行");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_entries_sorted_on_write() {
        let p = tmp();
        append_entries(&p, vec![entry(Some("z.md"), "R", "d9"), entry(Some("a.md"), "R", "d1")]).unwrap();
        let d = read_data(&p).unwrap();
        let keys: Vec<_> = d.entries.iter().map(entry_key).collect();
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(keys, sorted, "寫出須按 key 排序（收斂 merge 衝突面）");
    }
}
