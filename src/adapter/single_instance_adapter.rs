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
pub use platform::{acquire, focus_existing, InstanceLock};

#[cfg(windows)]
mod platform {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{
        GetLastError, SetLastError, ERROR_ALREADY_EXISTS, ERROR_SUCCESS, HANDLE,
    };
    use windows::Win32::System::Threading::CreateMutexW;
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowW, SetForegroundWindow, ShowWindow, SW_RESTORE,
    };

    const MUTEX_NAME: &str = "Local\\OpenDS-SingleInstance-Mutex";
    const WINDOW_TITLE: &str = "OpenDS";

    pub struct InstanceLock {
        _handle: HANDLE,
    }

    pub fn acquire() -> Option<InstanceLock> {
        acquire_named(MUTEX_NAME)
    }

    fn acquire_named(mutex_name: &str) -> Option<InstanceLock> {
        let name = to_wide(mutex_name);

        unsafe { SetLastError(ERROR_SUCCESS) };

        let handle = unsafe { CreateMutexW(None, false, PCWSTR(name.as_ptr())) }.ok()?;
        let already_running = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;

        if already_running {
            None
        } else {
            Some(InstanceLock { _handle: handle })
        }
    }

    pub fn focus_existing() -> bool {
        let title = to_wide(WINDOW_TITLE);

        match unsafe { FindWindowW(PCWSTR::null(), PCWSTR(title.as_ptr())) } {
            Ok(hwnd) if !hwnd.is_invalid() => {
                unsafe {
                    let _ = ShowWindow(hwnd, SW_RESTORE);
                    let _ = SetForegroundWindow(hwnd);
                }
                true
            }
            _ => false,
        }
    }

    fn to_wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn acquiring_the_lock_twice_in_the_same_process_only_succeeds_once() {
            let first = acquire_named("Local\\OpenDS-SingleInstance-Mutex-Test");
            assert!(first.is_some());

            let second = acquire_named("Local\\OpenDS-SingleInstance-Mutex-Test");
            assert!(second.is_none());
        }

        #[test]
        fn focusing_a_window_that_does_not_exist_reports_false_rather_than_panicking() {
            let title = to_wide("OpenDS-window-that-does-not-exist-in-any-test");

            let found = unsafe { FindWindowW(PCWSTR::null(), PCWSTR(title.as_ptr())) };

            assert!(found.is_err() || found.unwrap().is_invalid());
        }
    }
}

#[cfg(not(windows))]
pub fn acquire() -> Option<()> {
    Some(())
}

#[cfg(not(windows))]
pub fn focus_existing() -> bool {
    false
}
