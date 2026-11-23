use anyhow::Context;
use serde::Deserialize;
use std::collections::HashSet;
use std::path::Path;

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct KeyEntry {
    #[serde(default)]
    name: Option<String>,
    #[serde(alias = "apiKey", alias = "api_key", alias = "key")]
    api_key: String,
}

#[derive(Debug, Clone)]
pub struct ApiKeyStore {
    keys: HashSet<String>,
}

impl ApiKeyStore {
    pub fn new(keys: HashSet<String>) -> Self {
        Self { keys }
    }

    pub fn count(&self) -> usize {
        self.keys.len()
    }

    pub fn validate(&self, token: &str) -> bool {
        let trimmed = token.trim();
        if trimmed.is_empty() {
            return false;
        }
        self.keys.contains(trimmed)
    }

    /// Load from a path or a direct comma-separated string.
    /// If `spec` points to an existing file, it reads and parses it (JSON or lines).
    /// Otherwise, treats `spec` as a comma-separated list of keys.
    pub fn load(spec: &str) -> anyhow::Result<Self> {
        let path = Path::new(spec);
        if path.is_file() {
            Self::load_from_file(path)
        } else {
            let mut keys = HashSet::new();
            for part in spec.split(',') {
                let trimmed = part.trim();
                if !trimmed.is_empty() {
                    keys.insert(trimmed.to_string());
                }
            }
            if keys.is_empty() {
                anyhow::bail!("no valid API keys found in specification: {}", spec);
            }
            Ok(Self::new(keys))
        }
    }

    pub fn load_from_file(path: &Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read API keys file: {}", path.display()))?;

        // 1. Try parsing as JSON array of objects [{"name": "...", "apiKey": "..."}]
        if let Ok(entries) = serde_json::from_str::<Vec<KeyEntry>>(&content) {
            let mut keys = HashSet::new();
            for e in entries {
                let k = e.api_key.trim().to_string();
                if !k.is_empty() {
                    keys.insert(k);
                }
            }
            if !keys.is_empty() {
                return Ok(Self::new(keys));
            }
        }

        // 2. Try parsing as JSON array of strings ["key1", "key2"]
        if let Ok(entries) = serde_json::from_str::<Vec<String>>(&content) {
            let mut keys = HashSet::new();
            for e in entries {
                let k = e.trim().to_string();
                if !k.is_empty() {
                    keys.insert(k);
                }
            }
            if !keys.is_empty() {
                return Ok(Self::new(keys));
            }
        }

        // 3. Fallback: line by line or comma-separated text
        let mut keys = HashSet::new();
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            for part in line.split(',') {
                let trimmed = part.trim();
                if !trimmed.is_empty() {
                    keys.insert(trimmed.to_string());
                }
            }
        }

        if keys.is_empty() {
            anyhow::bail!("no valid API keys found in file: {}", path.display());
        }

        Ok(Self::new(keys))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_comma_separated_keys() {
        let store = ApiKeyStore::load("sk-123, sk-456 ,sk-789").unwrap();
        assert_eq!(store.count(), 3);
        assert!(store.validate("sk-123"));
        assert!(store.validate("sk-456"));
        assert!(store.validate("sk-789"));
        assert!(!store.validate("wrong"));
        assert!(!store.validate(""));
    }

    #[test]
    fn test_json_file() {
        let temp = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(
            temp.path(),
            r#"[{"name": "smoke", "apiKey": "sk-smoke-123"}, {"name": "agent", "api_key": "sk-agent-456"}]"#,
        )
        .unwrap();

        let store = ApiKeyStore::load(temp.path().to_str().unwrap()).unwrap();
        assert_eq!(store.count(), 2);
        assert!(store.validate("sk-smoke-123"));
        assert!(store.validate("sk-agent-456"));
        assert!(!store.validate("sk-smoke"));
    }
}
