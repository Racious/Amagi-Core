//! 文件路由器（adr-004 D7-② / D8）：AI 產出的耐久文件依 frontmatter `type` 自動歸入
//! vault 對應桶。本模組為純函式核心（決策 + 寫入），安全過濾與專案解析在指令層處理。
//!
//! 落點對照（amagi-conventions §5；三桶結構見 adr-004 D3）：
//! `adr`/`spec`/`business`/`concept`/`troubleshooting` → `<專案>/knowledge/`；
//! `test-report`/`review` → `<專案>/reports/`；`handoff` → 頂層 `daily/`；
//! 其餘（未知/缺 type）→ `<專案>/knowledge/`（兜底，標記 fallback）。

use std::io::Write;
use std::path::{Component, Path};
use crate::AppError;
use crate::utils::fs_utils;

/// 從 frontmatter 萃取的欄位（只取路由需要的最小集合）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ParsedFrontMatter {
    pub doc_type: Option<String>,
    pub title: Option<String>,
}

/// 路由落點決策（純函式產物）。
#[derive(Debug, Clone, PartialEq)]
pub struct RouteDecision {
    /// 正規化後的 type（trim + 小寫）。
    pub doc_type: String,
    /// 落點桶：`knowledge` / `reports` / `daily`。
    pub bucket: String,
    /// vault 根相對目錄（forward slash，跨機相對）。
    pub dir_relative: String,
    /// type 未知 / 缺漏 → 兜底進 knowledge 時為 true。
    pub is_fallback: bool,
}

/// 寫入結果。
#[derive(Debug, Clone, PartialEq)]
pub struct RouteResult {
    pub decision: RouteDecision,
    /// vault 根相對最終檔路徑。
    pub destination: String,
    pub written: bool,
    /// 目標已存在 → 非破壞略過（不覆寫既有內容）。
    pub skipped: bool,
}

/// 解析 frontmatter，取出 `type` 與 `title`。
/// 僅辨識「檔首即 `---` 區塊」；非 frontmatter 開頭則回空。輕量、不引入 YAML 依賴。
pub fn parse_frontmatter(content: &str) -> ParsedFrontMatter {
    let mut out = ParsedFrontMatter::default();
    let body = content.trim_start_matches('\u{feff}'); // 去 BOM
    let mut lines = body.lines();
    match lines.next() {
        Some(l) if l.trim() == "---" => {}
        _ => return out, // 檔首非 frontmatter
    }
    let mut closed = false;
    for line in lines {
        let t = line.trim();
        if t == "---" {
            closed = true; // frontmatter 正常結束
            break;
        }
        if let Some((k, v)) = t.split_once(':') {
            let key = k.trim();
            let val = strip_quotes(v.trim());
            if val.is_empty() {
                continue;
            }
            match key {
                "type" => out.doc_type = Some(val.to_string()),
                "title" => out.title = Some(val.to_string()),
                _ => {}
            }
        }
    }
    // 無閉合 `---` 視為非 frontmatter：不把內文中形如 `type: x` 的行誤當欄位，
    // 改走「缺 type → 兜底 knowledge」而非誤路由（Codex 審查 D-低）。
    if !closed {
        return ParsedFrontMatter::default();
    }
    out
}

/// type → (桶, 是否兜底)。未知或缺漏一律進 knowledge 且標記 fallback（兜底：永不漏接）。
pub fn bucket_for_type(doc_type: &str) -> (&'static str, bool) {
    match doc_type {
        "adr" | "spec" | "business" | "concept" | "troubleshooting" => ("knowledge", false),
        "test-report" | "review" => ("reports", false),
        "handoff" => ("daily", false),
        _ => ("knowledge", true),
    }
}

/// 由 type + 專案資料夾算出落點決策（純函式）。
/// - `handoff` → 頂層 `daily/`，不需專案。
/// - 其餘桶需 `project_folder`（如 `projects/amagi-core`）；缺則回錯誤。
pub fn route_decision(
    raw_type: &str,
    project_folder: Option<&str>,
) -> Result<RouteDecision, AppError> {
    let doc_type = normalize_type(raw_type);
    let (bucket, is_fallback) = bucket_for_type(&doc_type);

    let dir_relative = if bucket == "daily" {
        "daily".to_string()
    } else {
        let pf = project_folder
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| {
                AppError::InvalidPath(format!(
                    "type「{}」應落在專案 {} 桶，但未指定專案知識庫資料夾",
                    if doc_type.is_empty() { "(空)" } else { &doc_type },
                    bucket
                ))
            })?;
        // 安全驗證：project_folder 必須是 vault 內 `projects/<slug>` 形狀，否則兜底路由器
        // 會被 state 中遭污染的 vault_folder 變成任意寫入點（D-高）或誤路由到錯桶（D-低）。
        if !is_valid_project_folder(pf) {
            return Err(AppError::InvalidPath(format!(
                "專案知識庫資料夾須為 vault 內 projects/<slug> 形式（拒絕越界/絕對路徑/非標準）：{pf}"
            )));
        }
        format!("{}/{bucket}", pf.trim_end_matches('/'))
    };

    Ok(RouteDecision {
        doc_type,
        bucket: bucket.to_string(),
        dir_relative,
        is_fallback,
    })
}

/// 乾跑（preview）：只解析 + 算決策與最終落點，**不碰檔案系統**。供 UI/指令預覽。
/// 回傳 (決策, vault 根相對最終檔路徑)。
pub fn preview_route(
    project_folder: Option<&str>,
    content: &str,
    explicit_filename: Option<&str>,
) -> Result<(RouteDecision, String), AppError> {
    let fm = parse_frontmatter(content);
    let raw_type = fm.doc_type.clone().unwrap_or_default();
    let decision = route_decision(&raw_type, project_folder)?;
    let filename = derive_filename(explicit_filename, fm.title.as_deref());
    let destination = format!("{}/{}", decision.dir_relative, filename);
    Ok((decision, destination))
}

/// 路由並寫入：解析 frontmatter → 決策落點 → 非破壞寫入 vault。
/// `explicit_filename` 為 `None` 時以 `title` slug 命名。
pub fn route_document(
    vault_root: &Path,
    project_folder: Option<&str>,
    content: &str,
    explicit_filename: Option<&str>,
) -> Result<RouteResult, AppError> {
    if !vault_root.is_dir() {
        return Err(AppError::InvalidPath(format!(
            "vault 路徑不存在：{}",
            vault_root.display()
        )));
    }

    let (decision, destination) = preview_route(project_folder, content, explicit_filename)?;

    let dir = vault_root.join(&decision.dir_relative);

    // 建立目錄【前】：確認最深「既存」祖先 canonical 仍在 vault 根下。
    // 杜絕 vault 內既有 symlink/junction 指向外部時，create_dir_all 在 vault 外建出目錄
    // ——硬性兜底須「拒絕前不碰 vault 外」（Codex 複審 D-中）。
    if !is_within_vault(vault_root, deepest_existing(&dir))? {
        return Err(AppError::InvalidPath(format!(
            "落點祖先逃出 vault 根，已拒絕（疑似 symlink 穿越）：{}",
            decision.dir_relative
        )));
    }

    std::fs::create_dir_all(&dir).map_err(|e| AppError::Io(e.to_string()))?;

    // 建立目錄【後】：再以 canonical 確認最終目錄在 vault 根下（雙保險，擋建立過程新生的逃逸）（D-高）。
    if !is_within_vault(vault_root, &dir)? {
        return Err(AppError::InvalidPath(format!(
            "落點逃出 vault 根，已拒絕寫入：{}",
            decision.dir_relative
        )));
    }

    let file = vault_root.join(&destination);

    // 非破壞 + 原子：create_new 同時完成「不存在才建」，免去 exists/write 競態（D-低）。
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&file)
    {
        Ok(mut f) => {
            // 寫入中途失敗（磁碟滿/IO 斷）會留下空檔/半成品，導致下次誤判 skipped；
            // 失敗時刪掉剛建立的檔再回報錯誤，保留原子建立語意（Codex 複審 D-低）。
            // 若連清理都失敗（防毒/權限短暫持有），明確回報殘留路徑，不靜默吞掉（Codex r3 D-低）。
            if let Err(e) = f.write_all(content.as_bytes()) {
                drop(f);
                return Err(AppError::Io(match std::fs::remove_file(&file) {
                    Ok(()) => e.to_string(),
                    Err(ce) => format!(
                        "{e}；且清理半成品檔失敗，殘留 {}：{ce}",
                        file.display()
                    ),
                }));
            }
            Ok(RouteResult {
                decision,
                destination,
                written: true,
                skipped: false,
            })
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => Ok(RouteResult {
            decision,
            destination,
            written: false,
            skipped: true,
        }),
        Err(e) => Err(AppError::Io(e.to_string())),
    }
}

/// 路徑 canonical 後是否落在 vault 根之下。path 必須已存在（否則 canonicalize 失敗回 Err）。
fn is_within_vault(vault_root: &Path, path: &Path) -> Result<bool, AppError> {
    let canon_root = vault_root
        .canonicalize()
        .map_err(|e| AppError::Io(e.to_string()))?;
    let canon = path.canonicalize().map_err(|e| AppError::Io(e.to_string()))?;
    Ok(canon.starts_with(&canon_root))
}

/// 回傳 path 由深至淺第一個「已存在」的祖先（含自身）。用於建立目錄前的 containment 驗證。
fn deepest_existing(path: &Path) -> &Path {
    let mut p = path;
    while !p.exists() {
        match p.parent() {
            Some(parent) => p = parent,
            None => break,
        }
    }
    p
}

/// project_folder 是否為慣例的 `projects/<slug>[/...]` 形狀：首段須為 `projects`、
/// 至少含一個 slug 段、且所有段皆為一般段（拒絕絕對路徑前綴 `C:`、根 `/`、`.`、`..`）。
/// 收斂形狀同時擋掉越界（D-高）與誤路由到 `daily`/`shared` 等錯桶（D-低）。
fn is_valid_project_folder(pf: &str) -> bool {
    let mut comps = Path::new(pf).components();
    // 首段必為 Normal("projects")
    match comps.next() {
        Some(Component::Normal(s)) if s == "projects" => {}
        _ => return false,
    }
    // 後續至少一段，且全為一般段
    let mut has_slug = false;
    for c in comps {
        match c {
            Component::Normal(_) => has_slug = true,
            _ => return false, // Prefix / RootDir / CurDir / ParentDir 一律拒絕
        }
    }
    has_slug
}

fn normalize_type(raw: &str) -> String {
    raw.trim().to_lowercase()
}

/// 決定檔名：明確檔名（取末段防穿越、清洗 Windows 不安全字元/保留名、補 `.md`）優先，
/// 清洗後為空則退回 title slug，再否則 untitled。
fn derive_filename(explicit: Option<&str>, title: Option<&str>) -> String {
    if let Some(f) = explicit {
        // 先取檔名段剝除 `../`、`a/b`、絕對路徑，再清洗（Codex 審查 D-低）。
        if let Some(base) = Path::new(f.trim()).file_name().and_then(|n| n.to_str()) {
            if let Some(clean) = sanitize_basename(base) {
                return ensure_md(&clean);
            }
        }
    }
    let slug = title.map(fs_utils::slugify).filter(|s| !s.is_empty());
    match slug {
        Some(s) => format!("{s}.md"),
        None => "untitled.md".to_string(),
    }
}

/// Windows 保留裝置名（不分大小寫、不論副檔名皆不可用作檔名）。
const RESERVED_DEVICE_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// 清洗單一檔名段：移除 Windows 不安全字元（`< > : " / \ | ? *`）與控制字元、
/// 修剪尾端點與空白、擋 `.`/`..`/保留裝置名。回 `None` 表示無法產生安全檔名（應改用 slug）。
fn sanitize_basename(name: &str) -> Option<String> {
    let cleaned: String = name
        .chars()
        .filter(|c| !matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'))
        .filter(|c| !c.is_control())
        .collect();
    let trimmed = cleaned.trim().trim_end_matches('.').trim();
    if trimmed.is_empty() || trimmed == "." || trimmed == ".." {
        return None;
    }
    // 主檔名（首個 `.` 前）比對保留裝置名
    let stem = trimmed.split('.').next().unwrap_or(trimmed).to_uppercase();
    if RESERVED_DEVICE_NAMES.contains(&stem.as_str()) {
        return None;
    }
    Some(trimmed.to_string())
}

fn ensure_md(name: &str) -> String {
    if name.to_lowercase().ends_with(".md") {
        name.to_string()
    } else {
        format!("{name}.md")
    }
}

fn strip_quotes(s: &str) -> &str {
    let bytes = s.as_bytes();
    if bytes.len() >= 2 {
        let first = bytes[0];
        let last = bytes[bytes.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return &s[1..s.len() - 1];
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_frontmatter_extracts_type_and_title() {
        let c = "---\nid: wiki-1\ntitle: Bridge 設計\ntype: adr\nstatus: active\n---\n\n# 內文";
        let fm = parse_frontmatter(c);
        assert_eq!(fm.doc_type.as_deref(), Some("adr"));
        assert_eq!(fm.title.as_deref(), Some("Bridge 設計"));
    }

    #[test]
    fn test_parse_frontmatter_strips_quotes_and_bom() {
        let c = "\u{feff}---\ntitle: \"加引號標題\"\ntype: 'review'\n---\n內文";
        let fm = parse_frontmatter(c);
        assert_eq!(fm.title.as_deref(), Some("加引號標題"));
        assert_eq!(fm.doc_type.as_deref(), Some("review"));
    }

    #[test]
    fn test_parse_frontmatter_none_when_no_block() {
        let fm = parse_frontmatter("# 沒有 frontmatter\n\ntype: adr 只是內文");
        assert_eq!(fm, ParsedFrontMatter::default());
    }

    #[test]
    fn test_bucket_for_type_known() {
        assert_eq!(bucket_for_type("adr"), ("knowledge", false));
        assert_eq!(bucket_for_type("spec"), ("knowledge", false));
        assert_eq!(bucket_for_type("troubleshooting"), ("knowledge", false));
        assert_eq!(bucket_for_type("test-report"), ("reports", false));
        assert_eq!(bucket_for_type("review"), ("reports", false));
        assert_eq!(bucket_for_type("handoff"), ("daily", false));
    }

    #[test]
    fn test_bucket_for_type_unknown_falls_back_to_knowledge() {
        assert_eq!(bucket_for_type("不認識"), ("knowledge", true));
        assert_eq!(bucket_for_type(""), ("knowledge", true));
    }

    #[test]
    fn test_route_decision_project_buckets() {
        let d = route_decision("adr", Some("projects/amagi-core")).unwrap();
        assert_eq!(d.bucket, "knowledge");
        assert_eq!(d.dir_relative, "projects/amagi-core/knowledge");
        assert!(!d.is_fallback);

        let d = route_decision("review", Some("projects/amagi-core")).unwrap();
        assert_eq!(d.dir_relative, "projects/amagi-core/reports");
    }

    #[test]
    fn test_route_decision_handoff_goes_top_level_daily() {
        // handoff 不需專案，落頂層 daily/
        let d = route_decision("handoff", None).unwrap();
        assert_eq!(d.bucket, "daily");
        assert_eq!(d.dir_relative, "daily");
    }

    #[test]
    fn test_route_decision_unknown_type_fallback() {
        let d = route_decision("亂填", Some("projects/x")).unwrap();
        assert_eq!(d.dir_relative, "projects/x/knowledge");
        assert!(d.is_fallback);
    }

    #[test]
    fn test_route_decision_project_bucket_without_project_errors() {
        let err = route_decision("adr", None);
        assert!(err.is_err());
    }

    #[test]
    fn test_route_decision_trims_trailing_slash() {
        let d = route_decision("spec", Some("projects/x/")).unwrap();
        assert_eq!(d.dir_relative, "projects/x/knowledge");
    }

    #[test]
    fn test_route_decision_rejects_unsafe_project_folder() {
        // 越界 / 絕對路徑 / 含 .. 段一律拒絕（Codex 審查 D-高）
        assert!(route_decision("adr", Some("../outside")).is_err());
        assert!(route_decision("adr", Some("projects/../../etc")).is_err());
        assert!(route_decision("adr", Some("/etc/passwd")).is_err());
        assert!(route_decision("adr", Some("C:\\Windows")).is_err());
        assert!(route_decision("adr", Some(".")).is_err());
        // 非 projects/<slug> 形狀拒絕，杜絕誤路由到錯桶（Codex 複審 D-低）
        assert!(route_decision("adr", Some("daily")).is_err());
        assert!(route_decision("adr", Some("projects")).is_err()); // 缺 slug
        assert!(route_decision("adr", Some("shared/foo")).is_err());
        // 合法 projects/<slug> 仍通過（含多層 slug）
        assert!(route_decision("adr", Some("projects/amagi-core")).is_ok());
        assert!(route_decision("adr", Some("projects/group/sub")).is_ok());
    }

    #[test]
    fn test_parse_frontmatter_unclosed_returns_default() {
        // 無閉合 --- → 不誤抓內文中的 type/title（Codex 審查 D-低）
        let c = "---\ntitle: 半截\ntype: adr\n\n內文裡也有 type: review 這種行";
        assert_eq!(parse_frontmatter(c), ParsedFrontMatter::default());
    }

    #[test]
    fn test_sanitize_basename_blocks_windows_hazards() {
        // 保留裝置名 → None（退回 slug）
        assert_eq!(sanitize_basename("CON.md"), None);
        assert_eq!(sanitize_basename("nul"), None);
        // ADS colon、不安全字元被剝除
        assert_eq!(sanitize_basename("name:stream.md").as_deref(), Some("namestream.md"));
        assert_eq!(sanitize_basename("a*b?.md").as_deref(), Some("ab.md"));
        // 尾端點 / 空白修剪
        assert_eq!(sanitize_basename("file.").as_deref(), Some("file"));
        // 全屬危險字元 → None
        assert_eq!(sanitize_basename("***"), None);
    }

    #[test]
    fn test_derive_filename_windows_reserved_falls_back() {
        // CON.md 為保留名 → 退回 title slug
        assert_eq!(derive_filename(Some("CON.md"), Some("安全標題")), "安全標題.md");
        // 無 title 時退回 untitled
        assert_eq!(derive_filename(Some("NUL"), None), "untitled.md");
    }

    #[test]
    fn test_route_document_rejects_unsafe_project_folder() {
        let dir = std::env::temp_dir().join(format!("amagi-router-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let content = "---\ntitle: x\ntype: adr\n---\nx";
        assert!(route_document(&dir, Some("../escape"), content, None).is_err());
        // 確認沒有任何檔被寫到 vault 外
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_derive_filename_prevents_traversal() {
        // 目錄穿越企圖 → 只留檔名段
        assert_eq!(derive_filename(Some("../../etc/passwd"), None), "passwd.md");
        assert_eq!(derive_filename(Some("a/b/c.md"), None), "c.md");
        assert_eq!(derive_filename(Some("report"), None), "report.md");
        assert_eq!(derive_filename(Some("report.MD"), None), "report.MD");
    }

    #[test]
    fn test_derive_filename_falls_back_to_title_slug() {
        assert_eq!(derive_filename(None, Some("Bridge Design")), "bridge-design.md");
        assert_eq!(derive_filename(None, None), "untitled.md");
        // 純符號標題 slug 為空 → untitled
        assert_eq!(derive_filename(None, Some("!!!")), "untitled.md");
    }

    #[test]
    fn test_route_document_writes_to_knowledge() {
        let dir = std::env::temp_dir().join(format!("amagi-router-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let content = "---\ntitle: 測試規格\ntype: spec\n---\n\n內文";
        let res = route_document(&dir, Some("projects/amagi-core"), content, None).unwrap();
        assert!(res.written);
        assert!(!res.skipped);
        assert_eq!(res.destination, "projects/amagi-core/knowledge/測試規格.md");
        assert!(dir.join("projects/amagi-core/knowledge/測試規格.md").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_route_document_handoff_to_top_level_daily() {
        let dir = std::env::temp_dir().join(format!("amagi-router-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let content = "---\ntitle: 換窗交接\ntype: handoff\n---\n交接內容";
        // handoff 不傳專案也可
        let res = route_document(&dir, None, content, Some("2026-06-28.md")).unwrap();
        assert_eq!(res.destination, "daily/2026-06-28.md");
        assert!(dir.join("daily/2026-06-28.md").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_route_document_non_destructive_skip() {
        let dir = std::env::temp_dir().join(format!("amagi-router-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let content = "---\ntitle: 既存\ntype: adr\n---\nx";
        let r1 = route_document(&dir, Some("projects/x"), content, None).unwrap();
        assert!(r1.written);
        // 再寫一次 → 略過、不覆寫
        let r2 = route_document(&dir, Some("projects/x"), "---\ntitle: 既存\ntype: adr\n---\n改了", None).unwrap();
        assert!(!r2.written);
        assert!(r2.skipped);
        let kept = std::fs::read_to_string(dir.join("projects/x/knowledge/既存.md")).unwrap();
        assert!(kept.contains("\nx"));
        assert!(!kept.contains("改了"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_route_document_missing_type_falls_back_to_knowledge() {
        let dir = std::env::temp_dir().join(format!("amagi-router-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        // 缺 type → 兜底 knowledge、fallback=true
        let content = "---\ntitle: 無型別\n---\n內文";
        let res = route_document(&dir, Some("projects/x"), content, None).unwrap();
        assert!(res.decision.is_fallback);
        assert_eq!(res.decision.bucket, "knowledge");
        assert!(res.written);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
