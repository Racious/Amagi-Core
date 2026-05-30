use once_cell::sync::Lazy;
use regex::Regex;

static SENSITIVE_PATTERNS: Lazy<Vec<Regex>> = Lazy::new(|| {
    vec![
        Regex::new(r"(?i)(password|passwd)\s*[:=]\s*\S+").unwrap(),
        Regex::new(r"(?i)(api[_-]?key|apikey)\s*[:=]\s*\S+").unwrap(),
        Regex::new(r"(?i)(secret|client_secret)\s*[:=]\s*\S+").unwrap(),
        Regex::new(r"(?i)(access_token|refresh_token|bearer_token)\s*[:=]\s*\S+").unwrap(),
        Regex::new(r"(?i)bearer\s+[A-Za-z0-9\-._~+/]+=*").unwrap(),
        Regex::new(r"(?i)authorization\s*[:=]\s*\S+").unwrap(),
        Regex::new(r"(?i)x-api-key\s*[:=]\s*\S+").unwrap(),
        Regex::new(r"(?i)(database_url|jdbc_url)\s*[:=]\s*\S+").unwrap(),
        Regex::new(r"(?i)private_key\s*[:=]\s*\S+").unwrap(),
        Regex::new(r"-----BEGIN (RSA |EC |DSA )?PRIVATE KEY-----").unwrap(),
        Regex::new(r"gh[pousp]_[A-Za-z0-9]{36,}").unwrap(),
        Regex::new(r"\b[0-9a-fA-F]{40,64}\b").unwrap(),
    ]
});

pub struct SafetyResult {
    pub is_safe: bool,
    pub matched_patterns: Vec<String>,
}

pub fn check(text: &str) -> SafetyResult {
    let mut matched = Vec::new();
    for pattern in SENSITIVE_PATTERNS.iter() {
        if pattern.is_match(text) {
            matched.push(pattern.as_str().to_string());
        }
    }
    SafetyResult {
        is_safe: matched.is_empty(),
        matched_patterns: matched,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detects_password() {
        let result = check("password=mysecret123");
        assert!(!result.is_safe);
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
    }

    #[test]
    fn test_hex_minimum_40_chars() {
        let short = "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b"; // 39 chars - safe
        let long = "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2"; // 40 chars - flagged
        assert!(check(short).is_safe);
        assert!(!check(long).is_safe);
    }
}
