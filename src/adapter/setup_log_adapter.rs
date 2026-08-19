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

#[cfg(windows)]
mod platform {
    use std::fs::OpenOptions;
    use std::io::Write;

    const PATHS: &[&str] = &[
        "C:\\Windows\\Temp\\opends-setup.log",
        "C:\\Users\\Public\\opends-setup.log",
    ];

    pub fn log(text: &str) {
        for path in PATHS {
            if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
                let _ = writeln!(file, "{text}");
            }
        }
    }
}

#[cfg(not(windows))]
mod platform {
    pub fn log(_text: &str) {}
}

pub use platform::log;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logging_never_panics_even_when_no_path_is_writable() {
        log("a line that may or may not land anywhere");
    }
}
