//! Output style 分發（vault `_output-styles/` 單一正本 → `~/.claude/output-styles/` 副本）。
//!
//! 形態比照 doctrine 部署（單鈕、全域單目標），比 `_skills` 簡單：扁平 `*.md`、
//! 覆蓋式冪等寫入、無刪除面。副本不做 `.bak`（正本在 vault，副本非權威源）。
//! 另負責 `~/.claude/settings.json` 的 `outputStyle` 預設 ensure（A-2，fail-safe）。

use std::path::Path;
use crate::AppError;

/// 預設 output style 名（settings.json 缺 `outputStyle` 時補上）。要換預設款改這一行。
pub const DEFAULT_OUTPUT_STYLE: &str = "天城";

/// settings.json 的處理結果（A-2 情境窮舉）。
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SettingsAction {
    /// 檔案不存在（新機常態）→ 建立最小 JSON 帶預設。
    CreatedWithDefault,
    /// 缺 `outputStyle` 欄位 → 補預設（其他欄位與順序保留）。
    AddedDefault,
    /// 已有值 → 位元組級不動（完全不寫檔）。
    AlreadySet,
    /// JSON 解析失敗／頂層非物件（含空檔）→ 不寫、回報警告（fail-safe，
    /// 寧可不補預設也不冒清空老爺設定的險）。
    ParseFailedSkipped,
    /// 無任何 style 可分發 → settings 不動（避免把預設指向不存在的款式）。
    SkippedNoStyles,
}

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputStyleDistributeResult {
    /// 已分發款式的顯示名（frontmatter `name:`；缺則以檔名代）。
    pub distributed: Vec<String>,
    /// 缺 `name:` frontmatter 的檔名（Claude Code 切換依賴 name，須提醒補上）。
    pub missing_name: Vec<String>,
    pub settings_action: SettingsAction,
}

/// 檔名是否為合法可分發的 style 檔：`.md`、非 README（大小寫不敏感）、
/// 非 dot-prefixed、不含路徑分隔（比照 `is_valid_skill_slug` 精神）。
fn is_distributable_file_name(file_name: &str) -> bool {
    let lower = file_name.to_ascii_lowercase();
    lower.ends_with(".md")
        && lower != "readme.md"
        && !file_name.starts_with('.')
        && !file_name.contains('/')
        && !file_name.contains('\\')
}

/// 從 style 檔內容解析 frontmatter `name:`（與 skill_library::parse_name 同慣例：前 15 行內）。
fn parse_name(content: &str) -> Option<String> {
    for line in content.lines().take(15) {
        if let Some(v) = line.trim().strip_prefix("name:") {
            let v = v.trim().trim_matches('"').trim_matches('\'');
            if !v.is_empty() {
                return Some(v.to_string());
            }
        }
    }
    None
}

/// 收集 vault `_output-styles/` 下可分發的 style 檔，回傳 (檔名, 內容)，依檔名排序（回報確定性）。
/// 「目錄不存在」是合法空狀態（vault 尚未建 `_output-styles/`）→ Ok(空)；
/// 目錄存在但讀取失敗（權限、或路徑其實是檔案）→ Err，不得與空清單混同（Codex 低-1）。
fn collect_styles(dir: &Path) -> Result<Vec<(String, String)>, AppError> {
    let mut out = Vec::new();
    if !dir.exists() {
        return Ok(out);
    }
    let entries = std::fs::read_dir(dir)
        .map_err(|e| AppError::Io(format!("讀取 _output-styles 失敗（非空清單，屬 IO 異常）：{e}")))?;
    for e in entries.flatten() {
        let p = e.path();
        if !p.is_file() {
            continue;
        }
        let file_name = match p.file_name().and_then(|s| s.to_str()) {
            Some(s) => s.to_string(),
            None => continue,
        };
        if !is_distributable_file_name(&file_name) {
            continue;
        }
        if let Ok(content) = std::fs::read_to_string(&p) {
            out.push((file_name, content));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(out)
}

/// 原子寫檔（temp + rename，同目錄暫存）：失敗清暫存。style 副本與 settings 共用，
/// 確保中斷不留截斷檔（Codex 低-2）；rename 撞到同名「目錄」會乾淨失敗、不誤刪。
fn write_atomic(path: &Path, content: &str) -> Result<(), AppError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| AppError::Io(e.to_string()))?;
    }
    let mut tmp = path.as_os_str().to_os_string();
    tmp.push(".tmp");
    let tmp = std::path::PathBuf::from(tmp);
    std::fs::write(&tmp, content).map_err(|e| AppError::Io(e.to_string()))?;
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(AppError::Io(e.to_string()));
    }
    Ok(())
}

/// 原子寫 JSON：UTF-8 無 BOM、2 空格縮排（與 Claude Code 既有格式一致）。
fn write_json_atomic(path: &Path, value: &serde_json::Value) -> Result<(), AppError> {
    let text = serde_json::to_string_pretty(value).map_err(|e| AppError::Io(e.to_string()))?;
    write_atomic(path, &text)
}

/// A-2：確保 settings.json 有 `outputStyle`。四情境見 `SettingsAction`。
/// 讀取剝 BOM（PS 5.1 等工具寫入的 BOM 會讓 serde 解析失敗，屬假性壞檔）；
/// 已有值時**完全不寫檔**（位元組級不動）。
pub fn ensure_settings_output_style(settings_path: &Path) -> Result<SettingsAction, AppError> {
    if !settings_path.exists() {
        let mut map = serde_json::Map::new();
        map.insert(
            "outputStyle".to_string(),
            serde_json::Value::String(DEFAULT_OUTPUT_STYLE.to_string()),
        );
        write_json_atomic(settings_path, &serde_json::Value::Object(map))?;
        return Ok(SettingsAction::CreatedWithDefault);
    }
    let raw = std::fs::read_to_string(settings_path).map_err(|e| AppError::Io(e.to_string()))?;
    let mut value: serde_json::Value = match serde_json::from_str(raw.trim_start_matches('\u{feff}')) {
        Ok(v) => v,
        Err(_) => return Ok(SettingsAction::ParseFailedSkipped),
    };
    let obj = match value.as_object_mut() {
        Some(o) => o,
        None => return Ok(SettingsAction::ParseFailedSkipped),
    };
    if obj.contains_key("outputStyle") {
        return Ok(SettingsAction::AlreadySet);
    }
    obj.insert(
        "outputStyle".to_string(),
        serde_json::Value::String(DEFAULT_OUTPUT_STYLE.to_string()),
    );
    write_json_atomic(settings_path, &value)?;
    Ok(SettingsAction::AddedDefault)
}

/// A-1＋A-2 主流程：分發 vault `_output-styles/*.md` 到 `styles_dest`（覆蓋、冪等），
/// 成功後 ensure settings 預設。無 style 可分發 → 不動 settings、回報空清單（不靜默成功）。
pub fn distribute_output_styles(
    vault_root: &Path,
    styles_dest: &Path,
    settings_path: &Path,
) -> Result<OutputStyleDistributeResult, AppError> {
    let styles = collect_styles(&vault_root.join("_output-styles"))?;
    if styles.is_empty() {
        return Ok(OutputStyleDistributeResult {
            distributed: vec![],
            missing_name: vec![],
            settings_action: SettingsAction::SkippedNoStyles,
        });
    }
    std::fs::create_dir_all(styles_dest).map_err(|e| AppError::Io(e.to_string()))?;
    let mut distributed = Vec::new();
    let mut missing_name = Vec::new();
    for (file_name, content) in &styles {
        write_atomic(&styles_dest.join(file_name), content)
            .map_err(|e| AppError::Io(format!("寫入 {file_name} 失敗：{e:?}")))?;
        match parse_name(content) {
            Some(name) => distributed.push(name),
            None => {
                missing_name.push(file_name.clone());
                distributed.push(file_name.clone());
            }
        }
    }
    // settings ensure 在分發成功後執行；IO 失敗時明講「style 已分發」，避免誤判整批失敗。
    let settings_action = ensure_settings_output_style(settings_path)
        .map_err(|e| AppError::Io(format!("output styles 已分發，但 settings.json 寫入失敗：{e:?}")))?;
    Ok(OutputStyleDistributeResult { distributed, missing_name, settings_action })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn sandbox(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("amagi-ostyle-{}-{}", tag, uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn mk_vault(root: &Path, files: &[(&str, &str)]) {
        let dir = root.join("_output-styles");
        std::fs::create_dir_all(&dir).unwrap();
        for (name, content) in files {
            std::fs::write(dir.join(name), content).unwrap();
        }
    }

    #[test]
    fn test_collect_filters_readme_dot_and_non_md() {
        let root = sandbox("collect");
        mk_vault(&root, &[
            ("amagi.md", "---\nname: 天城\n---\n本體"),
            ("README.md", "說明"),
            ("ReadMe.MD", "大小寫變體說明"),
            (".hidden.md", "隱藏"),
            ("distribute.ps1", "腳本"),
            ("plain.md", "---\nname: 白話\n---\n"),
        ]);
        let got = collect_styles(&root.join("_output-styles")).unwrap();
        let names: Vec<&str> = got.iter().map(|(f, _)| f.as_str()).collect();
        assert_eq!(names, vec!["amagi.md", "plain.md"], "README（含大小寫變體）、dot-prefixed、非 .md 均排除");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_distribute_writes_overwrites_and_is_idempotent() {
        let root = sandbox("dist");
        mk_vault(&root, &[("amagi.md", "---\nname: 天城\n---\nv1")]);
        let dest = root.join("dest");
        let settings = root.join("settings.json");

        let r1 = distribute_output_styles(&root, &dest, &settings).unwrap();
        assert_eq!(r1.distributed, vec!["天城"]);
        assert!(r1.missing_name.is_empty());
        assert_eq!(std::fs::read_to_string(dest.join("amagi.md")).unwrap(), "---\nname: 天城\n---\nv1");

        // 正本更新 → 再分發即覆蓋（vault 為正本）
        mk_vault(&root, &[("amagi.md", "---\nname: 天城\n---\nv2")]);
        let r2 = distribute_output_styles(&root, &dest, &settings).unwrap();
        assert_eq!(std::fs::read_to_string(dest.join("amagi.md")).unwrap(), "---\nname: 天城\n---\nv2");
        // 冪等：第二次 settings 已有值 → 不動
        assert_eq!(r2.settings_action, SettingsAction::AlreadySet);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_distribute_reports_missing_name() {
        let root = sandbox("noname");
        mk_vault(&root, &[("anon.md", "沒有 frontmatter 的檔")]);
        let dest = root.join("dest");
        let settings = root.join("settings.json");
        let r = distribute_output_styles(&root, &dest, &settings).unwrap();
        assert_eq!(r.missing_name, vec!["anon.md"], "缺 name: 須回報警告");
        assert_eq!(r.distributed, vec!["anon.md"], "仍照常分發，以檔名代顯示名");
        assert!(dest.join("anon.md").is_file());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_no_styles_reports_skipped_and_touches_nothing() {
        let root = sandbox("empty");
        // _output-styles 不存在
        let dest = root.join("dest");
        let settings = root.join("settings.json");
        let r = distribute_output_styles(&root, &dest, &settings).unwrap();
        assert!(r.distributed.is_empty());
        assert_eq!(r.settings_action, SettingsAction::SkippedNoStyles);
        assert!(!dest.exists(), "無 style 不建目錄");
        assert!(!settings.exists(), "無 style 不動 settings（避免預設指向不存在的款式）");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_collect_io_error_is_err_not_empty_list() {
        // Codex 低-1：_output-styles「存在但讀不了」（此處以檔案冒充目錄模擬）
        // 須回 Err，不得與「目錄不存在＝合法空清單」混同。
        let root = sandbox("ioerr");
        std::fs::write(root.join("_output-styles"), "我是檔案不是目錄").unwrap();
        let dest = root.join("dest");
        let settings = root.join("settings.json");
        let r = distribute_output_styles(&root, &dest, &settings);
        assert!(r.is_err(), "IO 異常須 Err，不得靜默降級成空清單");
        assert!(!dest.exists() && !settings.exists(), "fail-closed：不寫任何目標");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_distribute_dest_name_collision_with_dir_errs_cleanly() {
        // Codex 低-2/3：目的端已有「同名目錄」→ rename 乾淨失敗、回報含檔名、不留 .tmp 殘檔。
        let root = sandbox("collide");
        mk_vault(&root, &[("amagi.md", "---\nname: 天城\n---\n本體")]);
        let dest = root.join("dest");
        std::fs::create_dir_all(dest.join("amagi.md")).unwrap();
        let settings = root.join("settings.json");
        let err = distribute_output_styles(&root, &dest, &settings).unwrap_err();
        assert!(format!("{err:?}").contains("amagi.md"), "錯誤訊息須含撞名檔名");
        assert!(dest.join("amagi.md").is_dir(), "同名目錄不得被誤刪");
        assert!(!dest.join("amagi.md.tmp").exists(), "失敗須清暫存檔");
        assert!(!settings.exists(), "分發失敗不動 settings");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_distribute_leaves_no_tmp_residue() {
        let root = sandbox("notmp");
        mk_vault(&root, &[("amagi.md", "---\nname: 天城\n---\n本體")]);
        let dest = root.join("dest");
        let settings = root.join("settings.json");
        distribute_output_styles(&root, &dest, &settings).unwrap();
        let residue: Vec<_> = std::fs::read_dir(&dest).unwrap().flatten()
            .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
            .collect();
        assert!(residue.is_empty(), "成功路徑不得留 .tmp 殘檔");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_settings_missing_file_creates_default() {
        let root = sandbox("s-create");
        let settings = root.join("settings.json");
        let act = ensure_settings_output_style(&settings).unwrap();
        assert_eq!(act, SettingsAction::CreatedWithDefault);
        let raw = std::fs::read(&settings).unwrap();
        assert_ne!(&raw[..3], b"\xef\xbb\xbf", "寫回不得帶 BOM");
        let v: serde_json::Value = serde_json::from_slice(&raw).unwrap();
        assert_eq!(v["outputStyle"], DEFAULT_OUTPUT_STYLE);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_settings_missing_field_adds_and_preserves_others() {
        let root = sandbox("s-add");
        let settings = root.join("settings.json");
        std::fs::write(&settings, "{\n  \"zeta\": 1,\n  \"alpha\": {\"k\": true}\n}").unwrap();
        let act = ensure_settings_output_style(&settings).unwrap();
        assert_eq!(act, SettingsAction::AddedDefault);
        let raw = std::fs::read_to_string(&settings).unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(v["outputStyle"], DEFAULT_OUTPUT_STYLE);
        assert_eq!(v["zeta"], 1, "既有欄位保留");
        assert_eq!(v["alpha"]["k"], true);
        // preserve_order：zeta 在 alpha 前的原順序不被字典序重排
        let zi = raw.find("zeta").unwrap();
        let ai = raw.find("alpha").unwrap();
        assert!(zi < ai, "既有鍵順序須保留（preserve_order）");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_settings_already_set_is_byte_identical() {
        let root = sandbox("s-keep");
        let settings = root.join("settings.json");
        // 刻意用怪格式（tab 縮排、尾空行）——位元組級不動的鐵證
        let original = "{\n\t\"outputStyle\": \"自訂款\",\n\t\"other\": 2\n}\n\n";
        std::fs::write(&settings, original).unwrap();
        let act = ensure_settings_output_style(&settings).unwrap();
        assert_eq!(act, SettingsAction::AlreadySet);
        assert_eq!(std::fs::read_to_string(&settings).unwrap(), original, "已有值 → 檔案一位元組都不動");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_settings_parse_failure_is_fail_safe() {
        let root = sandbox("s-bad");
        for (tag, content) in [("broken", "{ not json"), ("empty", ""), ("nonobj", "[1,2,3]")] {
            let settings = root.join(format!("{tag}.json"));
            std::fs::write(&settings, content).unwrap();
            let act = ensure_settings_output_style(&settings).unwrap();
            assert_eq!(act, SettingsAction::ParseFailedSkipped, "{tag} 應 fail-safe 跳過");
            assert_eq!(std::fs::read_to_string(&settings).unwrap(), content, "{tag} 原檔不得被動到");
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn test_settings_bom_prefixed_valid_json_is_parsed() {
        let root = sandbox("s-bom");
        let settings = root.join("settings.json");
        // PS 5.1 寫出的 BOM 檔：剝 BOM 後應正常解析、補欄位、寫回無 BOM
        let mut bytes = vec![0xef, 0xbb, 0xbf];
        bytes.extend_from_slice(b"{\n  \"keep\": \"me\"\n}");
        std::fs::write(&settings, &bytes).unwrap();
        let act = ensure_settings_output_style(&settings).unwrap();
        assert_eq!(act, SettingsAction::AddedDefault, "BOM 不得被誤判為壞檔");
        let raw = std::fs::read(&settings).unwrap();
        assert_ne!(&raw[..3], b"\xef\xbb\xbf", "寫回須為 UTF-8 無 BOM");
        let v: serde_json::Value = serde_json::from_slice(&raw).unwrap();
        assert_eq!(v["keep"], "me");
        assert_eq!(v["outputStyle"], DEFAULT_OUTPUT_STYLE);
        let _ = std::fs::remove_dir_all(&root);
    }
}
