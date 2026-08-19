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

use opends_core::controller::mapping::FeatureFlags;

pub fn config_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("feature_flags.json")
}

pub fn load(path: &Path) -> FeatureFlags {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save(path: &Path, flags: &FeatureFlags) -> std::io::Result<()> {
    let text = serde_json::to_string_pretty(flags)
        .map_err(|error| std::io::Error::other(error.to_string()))?;

    std::fs::write(path, text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_file_loads_as_everything_on_rather_than_erroring() {
        let path = std::env::temp_dir().join("opends-feature-flags-missing.json");
        let _ = std::fs::remove_file(&path);

        assert_eq!(load(&path), FeatureFlags::default());
    }

    #[test]
    fn a_corrupted_file_loads_as_everything_on_rather_than_erroring() {
        let path = std::env::temp_dir().join("opends-feature-flags-corrupt.json");
        std::fs::write(&path, "not json at all").unwrap();

        assert_eq!(load(&path), FeatureFlags::default());

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn a_saved_file_loads_back_to_the_same_flags() {
        let path = std::env::temp_dir().join("opends-feature-flags-roundtrip.json");
        let flags = FeatureFlags {
            gyro_to_mouse: false,
            touchpad_to_mouse: true,
            adaptive_triggers: false,
            turbo: true,
            shift_layer: false,
            auto_profile_switching: true,
        };

        save(&path, &flags).unwrap();

        assert_eq!(load(&path), flags);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn config_path_sits_beside_the_running_exe() {
        assert!(config_path().ends_with("feature_flags.json"));
    }
}
