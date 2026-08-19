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

include!(concat!(env!("OUT_DIR"), "/payload.rs"));

#[derive(Debug, thiserror::Error)]
pub enum PayloadError {
    #[error("nothing is embedded in this build")]
    Empty,

    #[error("unpacking {0}")]
    Unpack(String),
}

pub fn is_embedded() -> bool {
    !FILES.is_empty()
}

pub fn embedded_names() -> Vec<&'static str> {
    FILES.iter().map(|(name, _)| *name).collect()
}

pub fn bytes_of_any(names: &[&str]) -> Option<u64> {
    if FILES.is_empty() {
        return None;
    }

    let total: u64 = FILES
        .iter()
        .filter(|(name, _)| names.contains(name))
        .map(|(_, bytes)| bytes.len() as u64)
        .sum();

    match total {
        0 => None,
        found => Some(found),
    }
}

pub fn total_bytes() -> usize {
    FILES.iter().map(|(_, bytes)| bytes.len()).sum()
}

pub fn unpack_to(dir: &Path) -> Result<(), PayloadError> {
    if FILES.is_empty() {
        return Err(PayloadError::Empty);
    }

    std::fs::create_dir_all(dir).map_err(|_| PayloadError::Unpack(dir.display().to_string()))?;

    for (name, bytes) in FILES {
        std::fs::write(dir.join(name), bytes)
            .map_err(|_| PayloadError::Unpack((*name).to_string()))?;
    }

    Ok(())
}

pub fn staging_dir() -> PathBuf {
    std::env::temp_dir().join("opends-setup-payload")
}

pub fn source_dir(beside_exe: PathBuf) -> PathBuf {
    match is_embedded() {
        true => staging_dir(),
        false => beside_exe,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_build_with_no_payload_says_so_rather_than_pretending() {
        if !is_embedded() {
            assert!(matches!(
                unpack_to(&std::env::temp_dir().join("opends-empty-unpack")),
                Err(PayloadError::Empty)
            ));
        }
    }

    #[test]
    fn the_source_directory_follows_whether_a_payload_is_embedded() {
        let beside = PathBuf::from("/some/where");

        match is_embedded() {
            true => assert_eq!(source_dir(beside), staging_dir()),
            false => assert_eq!(source_dir(beside.clone()), beside),
        }
    }

    #[test]
    fn a_build_with_no_payload_reports_no_size_rather_than_zero() {
        if !is_embedded() {
            assert_eq!(bytes_of_any(&["OpenDS.exe"]), None);
        }
    }

    #[test]
    fn an_embedded_build_reports_the_real_size_of_the_app() {
        if is_embedded() {
            let size = bytes_of_any(&["OpenDS.exe"]).unwrap();

            assert!(size > 1024 * 1024, "OpenDS.exe reported as {size} bytes");
        }
    }

    #[test]
    fn asking_for_a_file_that_is_not_embedded_reports_nothing() {
        assert_eq!(bytes_of_any(&["not-a-real-file.exe"]), None);
    }

    #[test]
    fn every_embedded_file_carries_bytes_rather_than_an_empty_slot() {
        for (name, bytes) in FILES {
            assert!(!bytes.is_empty(), "{name} embedded as zero bytes");
        }
    }

    #[test]
    fn an_embedded_build_carries_the_driver_and_the_app_together() {
        if is_embedded() {
            let names = embedded_names();

            assert!(names.contains(&"opends-uhid.dll"));
            assert!(names.contains(&"OpenDS.exe"));
            assert!(total_bytes() > 1024);
        }
    }

    #[test]
    fn unpacking_writes_every_embedded_file_where_it_was_asked_to() {
        if !is_embedded() {
            return;
        }

        let dir = std::env::temp_dir().join("opends-unpack-test");
        let _ = std::fs::remove_dir_all(&dir);

        unpack_to(&dir).unwrap();

        for name in embedded_names() {
            assert!(dir.join(name).exists(), "{name} was not written");
        }
    }
}
