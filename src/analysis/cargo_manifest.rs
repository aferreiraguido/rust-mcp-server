//! Wrapper around the `toml` crate to parse `Cargo.toml`.
//!
//! Mirror of `EnvironmentInspector` in `cobol-mcp-server` (which parses
//! FILE-CONTROL, CONFIGURATION SECTION, SPECIAL-NAMES).
//!
//! In Rust the equivalent is `Cargo.toml`: crate metadata,
//! dependencies, features, workspace members.

use serde::{Deserialize, Serialize};
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error, Serialize, Deserialize)]
pub enum CargoManifestError {
    #[error("io error reading {path}: {message}")]
    Io { path: String, message: String },

    #[error("toml parse error in {path}: {message}")]
    Parse { path: String, message: String },
}

/// Subset of Cargo.toml that matters for the tools.
///
/// We use `serde::Deserialize` on the native TOML format to avoid
/// coupling to a specific version of the spec.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CargoManifest {
    pub package: Option<Package>,
    pub workspace: Option<Workspace>,
    pub dependencies: Option<toml::Value>,
    #[serde(rename = "dev-dependencies")]
    pub dev_dependencies: Option<toml::Value>,
    #[serde(rename = "build-dependencies")]
    pub build_dependencies: Option<toml::Value>,
    pub features: Option<toml::Value>,
    pub lib: Option<LibBinTarget>,
    pub bin: Option<Vec<LibBinTarget>>,
    #[serde(default)]
    pub _raw: Option<toml::Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Package {
    pub name: Option<String>,
    pub version: Option<String>,
    pub edition: Option<String>,
    pub authors: Option<Vec<String>>,
    pub description: Option<String>,
    pub license: Option<String>,
    pub repository: Option<String>,
    pub rust_version: Option<String>,
    #[serde(default)]
    pub metadata: Option<toml::Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Workspace {
    pub members: Option<Vec<String>>,
    pub exclude: Option<Vec<String>>,
    pub default_members: Option<Vec<String>>,
    pub resolver: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LibBinTarget {
    pub name: Option<String>,
    pub path: Option<String>,
    #[serde(default)]
    pub required_features: Vec<String>,
}

impl CargoManifest {
    /// Reads and parses a `Cargo.toml`.
    pub fn load(path: &Path) -> Result<Self, CargoManifestError> {
        let path_str = path.display().to_string();
        let raw = std::fs::read_to_string(path).map_err(|e| CargoManifestError::Io {
            path: path_str.clone(),
            message: e.to_string(),
        })?;
        Self::parse(&raw, &path_str)
    }

    /// Parses a `Cargo.toml` from a string.
    pub fn parse(raw: &str, path_hint: &str) -> Result<Self, CargoManifestError> {
        toml::from_str::<toml::Value>(raw)
            .map_err(|e| CargoManifestError::Parse {
                path: path_hint.to_string(),
                message: e.to_string(),
            })
            .and_then(|v| {
                // Keeps the full raw value in case tools want to display it.
                let mut manifest: CargoManifest =
                    v.clone().try_into().map_err(|e: toml::de::Error| {
                        CargoManifestError::Parse {
                            path: path_hint.to_string(),
                            message: e.to_string(),
                        }
                    })?;
                manifest._raw = Some(v);
                Ok(manifest)
            })
    }

    pub fn crate_name(&self) -> Option<&str> {
        self.package.as_ref().and_then(|p| p.name.as_deref())
    }

    pub fn crate_version(&self) -> Option<&str> {
        self.package.as_ref().and_then(|p| p.version.as_deref())
    }

    pub fn workspace_members(&self) -> Vec<String> {
        self.workspace
            .as_ref()
            .and_then(|w| w.members.clone())
            .unwrap_or_default()
    }

    /// Iterates over direct dependencies declared in `[dependencies]`,
    /// `[dev-dependencies]`, `[build-dependencies]`. Returns
    /// `(name, scope, value)` where scope ∈ {"normal", "dev", "build"}.
    pub fn direct_dependencies(&self) -> Vec<(String, &'static str, &toml::Value)> {
        let mut out = Vec::new();
        for (scope, opt) in [
            ("normal", &self.dependencies),
            ("dev", &self.dev_dependencies),
            ("build", &self.build_dependencies),
        ] {
            if let Some(toml::Value::Table(t)) = opt {
                for (k, v) in t {
                    out.push((k.clone(), scope, v));
                }
            }
        }
        out
    }

    /// Iterates over features declared in `[features]`.
    /// Returns `(feature_name, list_of_enabling_deps)`, sorted
    /// by name (so that output is deterministic — HashMap does not
    /// guarantee order).
    pub fn features(&self) -> Vec<(String, Vec<String>)> {
        let mut out = Vec::new();
        if let Some(toml::Value::Table(t)) = &self.features {
            for (k, v) in t {
                if let toml::Value::Array(arr) = v {
                    let deps: Vec<String> = arr
                        .iter()
                        .filter_map(|x| x.as_str().map(|s| s.to_string()))
                        .collect();
                    out.push((k.clone(), deps));
                }
            }
        }
        out.sort_by(|a, b| a.0.cmp(&b.0));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
[package]
name = "sample_crate"
version = "0.1.0"
edition = "2021"
authors = ["test <test@example.com>"]
description = "Test fixture"

[dependencies]
serde = "1"
tokio = { version = "1", features = ["full"] }
"#;

    #[test]
    fn parse_basic() {
        let m = CargoManifest::parse(SAMPLE, "Cargo.toml").unwrap();
        assert_eq!(m.crate_name(), Some("sample_crate"));
        assert_eq!(m.crate_version(), Some("0.1.0"));
        let deps = m.direct_dependencies();
        assert_eq!(deps.len(), 2);
        assert_eq!(deps[0].0, "serde");
        assert_eq!(deps[0].1, "normal");
        assert_eq!(deps[1].0, "tokio");
    }

    #[test]
    fn workspace_members() {
        let raw = r#"
[workspace]
members = ["crate-a", "crate-b", "crates/*"]
"#;
        let m = CargoManifest::parse(raw, "Cargo.toml").unwrap();
        assert_eq!(
            m.workspace_members(),
            vec!["crate-a", "crate-b", "crates/*"]
        );
    }

    #[test]
    fn features() {
        let raw = r#"
[features]
default = ["std"]
std = []
async = ["tokio"]
"#;
        let m = CargoManifest::parse(raw, "Cargo.toml").unwrap();
        let features = m.features();
        assert_eq!(features.len(), 3);
        // The order is alphabetical (see `features()`) for determinism.
        assert_eq!(features[0].0, "async");
        assert_eq!(features[0].1, vec!["tokio"]);
        assert_eq!(features[1].0, "default");
        assert_eq!(features[1].1, vec!["std"]);
        assert_eq!(features[2].0, "std");
        assert!(features[2].1.is_empty());
    }
}
