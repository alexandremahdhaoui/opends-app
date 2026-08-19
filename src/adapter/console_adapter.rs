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

    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_GENERIC_READ, FILE_GENERIC_WRITE, FILE_SHARE_READ, FILE_SHARE_WRITE,
        OPEN_EXISTING,
    };
    use windows::Win32::System::Console::{
        AttachConsole, GetStdHandle, SetStdHandle, ATTACH_PARENT_PROCESS, STD_ERROR_HANDLE,
        STD_OUTPUT_HANDLE,
    };

    pub fn attach() {
        if already_has_output() {
            return;
        }

        if unsafe { AttachConsole(ATTACH_PARENT_PROCESS) }.is_err() {
            return;
        }

        redirect(STD_OUTPUT_HANDLE);
        redirect(STD_ERROR_HANDLE);
    }

    fn already_has_output() -> bool {
        match unsafe { GetStdHandle(STD_OUTPUT_HANDLE) } {
            Ok(handle) => !handle.is_invalid() && handle != HANDLE::default(),
            Err(_) => false,
        }
    }

    fn redirect(which: windows::Win32::System::Console::STD_HANDLE) {
        let name = windows::core::w!("CONOUT$");

        let handle = unsafe {
            CreateFileW(
                name,
                (FILE_GENERIC_READ | FILE_GENERIC_WRITE).0,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                Default::default(),
                None,
            )
        };

        if let Ok(handle) = handle {
            let _ = unsafe { SetStdHandle(which, handle) };
        }
    }
}

#[cfg(not(windows))]
mod platform {
    pub fn attach() {}
}

pub use platform::attach;

#[cfg(test)]
mod tests {
    #[test]
    fn attaching_twice_is_harmless() {
        super::attach();
        super::attach();
    }
}
