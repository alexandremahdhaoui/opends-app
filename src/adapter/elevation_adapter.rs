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

use thiserror::Error;

#[derive(Debug, Error)]
pub enum ElevationError {
    #[error("reading the process token: {0}")]
    Token(String),

    #[error("relaunching {path} as administrator: {0}", reason)]
    Relaunch { path: String, reason: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    AlreadyElevated,
    Relaunched,
}

#[cfg(windows)]
pub fn is_elevated() -> Result<bool, ElevationError> {
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::Security::{GetTokenInformation, TokenElevation, TOKEN_ELEVATION};
    use windows::Win32::Security::{TOKEN_ACCESS_MASK, TOKEN_QUERY};
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    unsafe {
        let mut token = Default::default();

        OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_ACCESS_MASK(TOKEN_QUERY.0),
            &mut token,
        )
        .map_err(|e| ElevationError::Token(e.to_string()))?;

        let mut elevation = TOKEN_ELEVATION::default();
        let mut returned = 0u32;

        let query = GetTokenInformation(
            token,
            TokenElevation,
            Some(&mut elevation as *mut _ as *mut _),
            std::mem::size_of::<TOKEN_ELEVATION>() as u32,
            &mut returned,
        );

        let _ = CloseHandle(token);

        query.map_err(|e| ElevationError::Token(e.to_string()))?;

        Ok(elevation.TokenIsElevated != 0)
    }
}

#[cfg(not(windows))]
pub fn is_elevated() -> Result<bool, ElevationError> {
    Ok(true)
}

#[cfg(windows)]
pub fn relaunch_as_admin(arguments: &[String]) -> Result<(), ElevationError> {
    use std::os::windows::ffi::OsStrExt;

    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let exe = std::env::current_exe().map_err(|e| ElevationError::Relaunch {
        path: "<unknown>".into(),
        reason: e.to_string(),
    })?;

    let wide = |text: &str| {
        std::ffi::OsStr::new(text)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect::<Vec<u16>>()
    };

    let verb = wide("runas");
    let file = wide(&exe.display().to_string());
    let parameters = wide(&arguments.join(" "));

    let mut info = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS,
        lpVerb: PCWSTR(verb.as_ptr()),
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: PCWSTR(parameters.as_ptr()),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };

    unsafe { ShellExecuteExW(&mut info) }.map_err(|e| ElevationError::Relaunch {
        path: exe.display().to_string(),
        reason: e.to_string(),
    })
}

#[cfg(not(windows))]
pub fn relaunch_as_admin(_arguments: &[String]) -> Result<(), ElevationError> {
    Err(ElevationError::Relaunch {
        path: "<not windows>".into(),
        reason: "elevation only exists on Windows".into(),
    })
}

pub fn decide(elevated: bool) -> Outcome {
    if elevated {
        Outcome::AlreadyElevated
    } else {
        Outcome::Relaunched
    }
}

pub fn ensure_elevated(arguments: &[String]) -> Result<Outcome, ElevationError> {
    match decide(is_elevated()?) {
        Outcome::AlreadyElevated => Ok(Outcome::AlreadyElevated),
        Outcome::Relaunched => {
            relaunch_as_admin(arguments)?;
            Ok(Outcome::Relaunched)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_process_that_is_already_elevated_is_not_relaunched() {
        assert_eq!(decide(true), Outcome::AlreadyElevated);
    }

    #[test]
    fn a_process_that_is_not_elevated_asks_windows_to_relaunch_it() {
        assert_eq!(decide(false), Outcome::Relaunched);
    }

    #[test]
    fn ensure_elevated_agrees_with_the_token_this_process_actually_has() {
        let elevated = is_elevated().unwrap();

        if elevated {
            assert_eq!(ensure_elevated(&[]).unwrap(), Outcome::AlreadyElevated);
        } else {
            assert_ne!(decide(elevated), Outcome::AlreadyElevated);
        }
    }

    #[cfg(not(windows))]
    #[test]
    fn off_windows_there_is_nothing_to_elevate_to() {
        let error = relaunch_as_admin(&["--uninstall".to_string()]).unwrap_err();

        assert!(error.to_string().contains("only exists on Windows"));
    }

    #[test]
    fn the_error_names_the_binary_it_could_not_relaunch() {
        let error = ElevationError::Relaunch {
            path: "C:\\opends\\OpenDS-Setup.exe".into(),
            reason: "the user declined".into(),
        };

        let text = error.to_string();

        assert!(text.contains("OpenDS-Setup.exe"));
        assert!(text.contains("the user declined"));
    }

    #[test]
    fn a_token_error_says_what_failed() {
        let error = ElevationError::Token("access denied".into());

        assert!(error.to_string().contains("access denied"));
    }
}
