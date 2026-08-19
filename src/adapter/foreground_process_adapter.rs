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

#[cfg_attr(test, mockall::automock)]
pub trait ForegroundProcess {
    fn current_process_name(&self) -> Option<String>;
}

pub struct Win32ForegroundProcess;

impl Win32ForegroundProcess {
    pub fn new() -> Self {
        Self
    }
}

impl Default for Win32ForegroundProcess {
    fn default() -> Self {
        Self::new()
    }
}

impl ForegroundProcess for Win32ForegroundProcess {
    fn current_process_name(&self) -> Option<String> {
        platform::current_process_name()
    }
}

#[cfg(windows)]
mod platform {
    use windows::Win32::Foundation::{CloseHandle, MAX_PATH};
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

    pub fn current_process_name() -> Option<String> {
        let hwnd = unsafe { GetForegroundWindow() };

        if hwnd.is_invalid() {
            return None;
        }

        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };

        if pid == 0 {
            return None;
        }

        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;

        let mut buffer = [0u16; MAX_PATH as usize];
        let mut size = buffer.len() as u32;

        let result = unsafe {
            QueryFullProcessImageNameW(
                handle,
                PROCESS_NAME_WIN32,
                windows::core::PWSTR(buffer.as_mut_ptr()),
                &mut size,
            )
        };

        let _ = unsafe { CloseHandle(handle) };

        if result.is_err() || size == 0 {
            return None;
        }

        let path = String::from_utf16_lossy(&buffer[..size as usize]);

        path.rsplit(['\\', '/']).next().map(str::to_string)
    }
}

#[cfg(not(windows))]
mod platform {
    pub fn current_process_name() -> Option<String> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asking_for_the_foreground_process_name_never_panics() {
        let adapter = Win32ForegroundProcess::new();

        let _ = adapter.current_process_name();
    }
}
