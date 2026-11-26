use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// The program's version, for `app_version`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AppVersion {
    /// `major.minor.patch`, from `[workspace.package]` in the root
    /// `Cargo.toml`.
    pub version: String,
}

/// The program's version. Its one source is the workspace `Cargo.toml`.
pub fn version() -> AppVersion {
    todo!("issue #1: return the workspace version")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_the_workspace_version() {
        assert_eq!(version().version, env!("CARGO_PKG_VERSION"));
    }

    #[test]
    fn version_has_three_numeric_parts() {
        let v = version().version;
        let parts: Vec<&str> = v.split('.').collect();
        assert_eq!(parts.len(), 3, "{v}");
        assert!(parts.iter().all(|p| p.parse::<u32>().is_ok()), "{v}");
    }
}
