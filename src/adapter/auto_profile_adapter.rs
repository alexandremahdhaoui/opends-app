// Copyright 2026 Alexandre Mahdhaoui
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct AutoProfileFile {
    pub rules: Vec<(String, String)>,
    pub default_profile: Option<String>,
}

pub fn config_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("auto_profiles.json")
}

pub fn load(path: &Path) -> AutoProfileFile {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save(path: &Path, file: &AutoProfileFile) -> std::io::Result<()> {
    let text = serde_json::to_string_pretty(file)
        .map_err(|error| std::io::Error::other(error.to_string()))?;

    std::fs::write(path, text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_file_loads_as_an_empty_config_rather_than_erroring() {
        let path = std::env::temp_dir().join("opends-auto-profiles-missing.json");
        let _ = std::fs::remove_file(&path);

        assert_eq!(load(&path), AutoProfileFile::default());
    }

    #[test]
    fn a_corrupted_file_loads_as_an_empty_config_rather_than_erroring() {
        let path = std::env::temp_dir().join("opends-auto-profiles-corrupt.json");
        std::fs::write(&path, "not json at all").unwrap();

        assert_eq!(load(&path), AutoProfileFile::default());

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_saved_file_loads_back_to_the_same_config() {
        let path = std::env::temp_dir().join("opends-auto-profiles-roundtrip.json");
        let file = AutoProfileFile {
            rules: vec![("forza.exe".to_string(), "forza.json".to_string())],
            default_profile: Some("default.json".to_string()),
        };

        save(&path, &file).unwrap();

        assert_eq!(load(&path), file);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn config_path_sits_beside_the_running_exe() {
        assert!(config_path().ends_with("auto_profiles.json"));
    }
}
