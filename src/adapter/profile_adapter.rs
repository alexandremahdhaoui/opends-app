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

use std::fs;
use std::path::Path;

use opends_core::controller::mapping::{Binding, Profile};

#[derive(Debug, thiserror::Error)]
pub enum ProfileError {
    #[error("reading profile {path}")]
    Read { path: String },

    #[error("parsing profile {path}: {reason}")]
    Parse { path: String, reason: String },

    #[error("profile {path} names buttons that do not exist: {unknown}")]
    UnknownButtons { path: String, unknown: String },
}

#[cfg_attr(test, mockall::automock)]
pub trait Profiles {
    fn load(&self, path: &str) -> Result<Profile, ProfileError>;
}

pub struct FileProfiles;

impl FileProfiles {
    pub fn new() -> Self {
        Self
    }
}

impl Default for FileProfiles {
    fn default() -> Self {
        Self::new()
    }
}

impl Profiles for FileProfiles {
    fn load(&self, path: &str) -> Result<Profile, ProfileError> {
        let text = fs::read_to_string(Path::new(path)).map_err(|_| ProfileError::Read {
            path: path.to_string(),
        })?;

        parse(path, &text)
    }
}

pub fn parse(path: &str, text: &str) -> Result<Profile, ProfileError> {
    let profile: Profile = serde_json::from_str(text).map_err(|error| ProfileError::Parse {
        path: path.to_string(),
        reason: error.to_string(),
    })?;

    let unknown = profile.unknown_button_names();

    if !unknown.is_empty() {
        return Err(ProfileError::UnknownButtons {
            path: path.to_string(),
            unknown: unknown.join(", "),
        });
    }

    Ok(profile)
}

pub fn default_profile() -> Profile {
    Profile::named("default")
        .bind("Cross", Binding::Key { code: 0x20 })
        .bind("Circle", Binding::Key { code: 0x1B })
        .bind("DpadUp", Binding::Key { code: 0x26 })
        .bind("DpadDown", Binding::Key { code: 0x28 })
        .bind("DpadLeft", Binding::Key { code: 0x25 })
        .bind("DpadRight", Binding::Key { code: 0x27 })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_well_formed_profile_parses() {
        let text = r#"{"name":"forza","bindings":{"Circle":{"kind":"key","code":27}}}"#;

        let profile = parse("test.json", text).unwrap();

        assert_eq!(profile.name, "forza");
        assert_eq!(profile.bindings.len(), 1);
    }

    #[test]
    fn a_typo_in_a_button_name_is_refused_with_the_name_in_the_message() {
        let text = r#"{"name":"typo","bindings":{"Trianlge":{"kind":"key","code":27}}}"#;

        let error = parse("test.json", text).unwrap_err();

        assert!(error.to_string().contains("Trianlge"));
    }

    #[test]
    fn malformed_json_names_the_file_it_failed_on() {
        let error = parse("broken.json", "{not json").unwrap_err();

        assert!(error.to_string().contains("broken.json"));
    }

    #[test]
    fn a_profile_with_no_bindings_is_valid_and_simply_maps_nothing() {
        let profile = parse("empty.json", r#"{"name":"empty","bindings":{}}"#).unwrap();

        assert_eq!(profile.bound_buttons(), 0);
    }

    #[test]
    fn the_default_profile_binds_only_buttons_that_exist() {
        assert!(default_profile().unknown_button_names().is_empty());
    }

    #[test]
    fn the_default_profile_survives_a_round_trip_through_the_parser() {
        let text = serde_json::to_string(&default_profile()).unwrap();

        assert_eq!(parse("default.json", &text).unwrap(), default_profile());
    }

    #[test]
    fn a_missing_file_reports_the_path_rather_than_an_io_error_code() {
        let error = FileProfiles::new()
            .load("/definitely/not/here.json")
            .unwrap_err();

        assert!(error.to_string().contains("/definitely/not/here.json"));
    }
}
