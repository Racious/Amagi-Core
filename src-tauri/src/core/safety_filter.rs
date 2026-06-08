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

/// 單一命中：規則名 + 遮罩後的片段（供使用者判斷真偽，不完整曝光機密）
pub struct SafetyHit {
    pub label: String,
    pub masked: String,
}

pub struct SafetyResult {
    pub is_safe: bool,
    pub hits: Vec<SafetyHit>,
}

pub fn check(text: &str) -> SafetyResult {
    let mut hits = Vec::new();
    for (pattern, label) in SENSITIVE_PATTERNS.iter() {
        if let Some(m) = pattern.find(text) {
            hits.push(SafetyHit {
                label: (*label).to_string(),
                masked: mask(m.as_str()),
            });
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

    #[test]
    fn test_mask_redacts_middle() {
        // 長片段應遮罩中段、保留頭尾
        let r = check("api_key=sk-abcdefghijklmnopqrstuvwxyz");
        let hit = r.hits.first().unwrap();
        assert!(hit.masked.contains('…'));
        assert!(!hit.masked.contains("klmnopqrst")); // 中段不外洩
    }
}
