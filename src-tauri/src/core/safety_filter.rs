use once_cell::sync::Lazy;
use regex::Regex;

/// 敏感模式：(正規表達式, 人話規則名)
static SENSITIVE_PATTERNS: Lazy<Vec<(Regex, &'static str)>> = Lazy::new(|| {
    vec![
        (Regex::new(r"(?i)(password|passwd)\s*[:=]\s*\S+").unwrap(), "密碼（password）"),
        (Regex::new(r"(?i)(api[_-]?key|apikey)\s*[:=]\s*\S+").unwrap(), "API key"),
        (Regex::new(r"(?i)(secret|client_secret)\s*[:=]\s*\S+").unwrap(), "secret / client_secret"),
        (Regex::new(r"(?i)(access_token|refresh_token|bearer_token)\s*[:=]\s*\S+").unwrap(), "存取／更新 token"),
        (Regex::new(r"(?i)bearer\s+[A-Za-z0-9\-._~+/]+=*").unwrap(), "Bearer 權杖"),
        (Regex::new(r"(?i)authorization\s*[:=]\s*\S+").unwrap(), "Authorization 標頭"),
        (Regex::new(r"(?i)x-api-key\s*[:=]\s*\S+").unwrap(), "X-API-Key"),
        (Regex::new(r"(?i)(database_url|jdbc_url)\s*[:=]\s*\S+").unwrap(), "資料庫連線字串"),
        (Regex::new(r"(?i)private_key\s*[:=]\s*\S+").unwrap(), "私鑰欄位（private_key）"),
        (Regex::new(r"-----BEGIN (RSA |EC |DSA )?PRIVATE KEY-----").unwrap(), "PEM 私鑰"),
        (Regex::new(r"gh[pousp]_[A-Za-z0-9]{36,}").unwrap(), "GitHub Token"),
        (Regex::new(r"\b[0-9a-fA-F]{40,64}\b").unwrap(), "長十六進位字串（可能是金鑰，也可能是 commit SHA／雜湊，請確認）"),
    ]
});

/// 單一命中：規則名 + 遮罩片段（顯示用）+ 值摘要（身分用）。
/// `masked` 僅保留頭尾供辨識，**不得作為身分比對**（理論可碰撞）；
/// 身分一律用 `value_digest`＝SHA-256(完整命中字串) 小寫 hex（adr-007 D2）。
pub struct SafetyHit {
    pub label: String,
    pub masked: String,
    pub value_digest: String,
}

pub struct SafetyResult {
    pub is_safe: bool,
    pub hits: Vec<SafetyHit>,
}

/// 完整命中字串的不可逆摘要（灰名單身分鍵素材）。
pub fn value_digest(s: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(s.as_bytes());
    format!("{:x}", hasher.finalize())
}

/// 掃描文字：每條規則收集**全部** match（adr-007 D0(a)——僅取第一個會讓
/// 灰名單過濾誤判「全壓制」而漏報後續未灰名單新值）；同 (label, digest) 去重
/// 避免同值多次出現灌爆 UI。`is_safe` 語意不變：任一命中即 false。
pub fn check(text: &str) -> SafetyResult {
    let mut hits: Vec<SafetyHit> = Vec::new();
    let mut seen: std::collections::HashSet<(usize, String)> = std::collections::HashSet::new();
    for (idx, (pattern, label)) in SENSITIVE_PATTERNS.iter().enumerate() {
        for m in pattern.find_iter(text) {
            let digest = value_digest(m.as_str());
            if seen.insert((idx, digest.clone())) {
                hits.push(SafetyHit {
                    label: (*label).to_string(),
                    masked: mask(m.as_str()),
                    value_digest: digest,
                });
            }
        }
    }
    SafetyResult {
        is_safe: hits.is_empty(),
        hits,
    }
}

/// 遮罩命中片段：保留頭尾少數字元供辨識，中間以 … 隱去。
fn mask(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let n = chars.len();
    if n <= 8 {
        let head: String = chars.iter().take(2).collect();
        format!("{}{}", head, "*".repeat(n.saturating_sub(2)))
    } else {
        let head: String = chars.iter().take(6).collect();
        let tail: String = chars.iter().skip(n - 2).collect();
        format!("{}…{}（共 {} 字）", head, tail, n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detects_password() {
        let result = check("password=mysecret123");
        assert!(!result.is_safe);
        assert!(result.hits.iter().any(|h| h.label.contains("密碼")));
    }

    #[test]
    fn test_detects_api_key() {
        let result = check("api_key=sk-abc123xyz789");
        assert!(!result.is_safe);
    }

    #[test]
    fn test_detects_github_pat() {
        let result = check("token: ghp_abcdefghijklmnopqrstuvwxyz1234567890");
        assert!(!result.is_safe);
    }

    #[test]
    fn test_safe_content() {
        let result = check("README.md: updated project description");
        assert!(result.is_safe);
        assert!(result.hits.is_empty());
    }

    #[test]
    fn test_hex_minimum_40_chars() {
        let short = "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b"; // 39 chars - safe
        let long = "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2"; // 40 chars - flagged
        assert!(check(short).is_safe);
        assert!(!check(long).is_safe);
    }

    /// D0(a)：同規則多值須全數收集——「僅取第一個」會讓灰名單過濾漏報新值（adr-007 R1 發現①）。
    #[test]
    fn test_find_iter_collects_all_matches_per_rule() {
        let a = "a".repeat(40);
        let b = "b".repeat(40);
        let r = check(&format!("{a}\n{b}"));
        let hex_hits: Vec<_> = r.hits.iter().filter(|h| h.label.contains("十六進位")).collect();
        assert_eq!(hex_hits.len(), 2, "兩個不同 hex 值應各成一筆命中");
        assert_ne!(hex_hits[0].value_digest, hex_hits[1].value_digest);
    }

    /// D0(a)：同值多次出現於同段 → 去重一筆（避免 UI 過吵）。
    #[test]
    fn test_same_value_deduped_within_check() {
        let a = "c".repeat(40);
        let r = check(&format!("{a}\nsome text\n{a}"));
        let hex_hits: Vec<_> = r.hits.iter().filter(|h| h.label.contains("十六進位")).collect();
        assert_eq!(hex_hits.len(), 1, "同值同規則應去重為一筆");
    }

    /// D2：digest 為身分——同值跨次呼叫穩定、不同值必不同。
    #[test]
    fn test_value_digest_stable_and_distinct() {
        assert_eq!(value_digest("abc"), value_digest("abc"));
        assert_ne!(value_digest("abc"), value_digest("abd"));
        assert_eq!(value_digest("abc").len(), 64);
    }

    #[test]
    fn test_mask_redacts_middle() {
        // 長片段應遮罩中段、保留頭尾
        let r = check("api_key=sk-abcdefghijklmnopqrstuvwxyz");
        let hit = r.hits.first().unwrap();
        assert!(hit.masked.contains('…'));
        assert!(!hit.masked.contains("klmnopqrst")); // 中段不外洩
    }
}
