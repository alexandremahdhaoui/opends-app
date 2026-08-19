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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayEvent {
    Show,
    Quit,
}

#[cfg(windows)]
pub use platform::{spawn, TrayHandle};

#[cfg(windows)]
mod platform {
    use std::sync::mpsc::{channel, Receiver, Sender};

    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::Shell::{
        Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
        NOTIFYICONDATAW,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        AppendMenuW, CreatePopupMenu, DefWindowProcW, DestroyMenu, DispatchMessageW, GetCursorPos,
        GetMessageW, LoadIconW, PostQuitMessage, RegisterClassExW, SetForegroundWindow,
        TrackPopupMenu, TranslateMessage, CW_USEDEFAULT, IDI_APPLICATION, MF_STRING, MSG,
        TPM_BOTTOMALIGN, TPM_LEFTALIGN, WM_APP, WM_CREATE, WM_DESTROY, WM_LBUTTONUP, WM_RBUTTONUP,
        WNDCLASSEXW, WS_OVERLAPPEDWINDOW,
    };

    use super::TrayEvent;

    const WM_TRAY_CALLBACK: u32 = WM_APP + 1;
    const MENU_SHOW: u32 = 1;
    const MENU_QUIT: u32 = 2;
    const ICON_ID: u32 = 1;

    thread_local! {
        static SENDER: std::cell::RefCell<Option<Sender<TrayEvent>>> =
            const { std::cell::RefCell::new(None) };
    }

    pub struct TrayHandle {
        hwnd: HWND,
    }

    impl TrayHandle {
        pub fn set_tooltip(&self, text: &str) {
            if self.hwnd.is_invalid() {
                return;
            }

            let mut tip = [0u16; 128];
            let tip_wide = wide(text);
            let copy_len = tip_wide.len().min(tip.len());
            tip[..copy_len].copy_from_slice(&tip_wide[..copy_len]);

            let data = NOTIFYICONDATAW {
                cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: self.hwnd,
                uID: ICON_ID,
                uFlags: NIF_TIP,
                szTip: tip,
                ..Default::default()
            };

            let _ = unsafe { Shell_NotifyIconW(NIM_MODIFY, &data) };
        }
    }

    impl Drop for TrayHandle {
        fn drop(&mut self) {
            let data = NOTIFYICONDATAW {
                cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: self.hwnd,
                uID: ICON_ID,
                ..Default::default()
            };

            let _ = unsafe { Shell_NotifyIconW(NIM_DELETE, &data) };
        }
    }

    unsafe impl Send for TrayHandle {}

    fn wide(text: &str) -> Vec<u16> {
        use std::os::windows::ffi::OsStrExt;

        std::ffi::OsStr::new(text)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    unsafe extern "system" fn wndproc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match msg {
            WM_TRAY_CALLBACK => {
                let clicked = lparam.0 as u32;

                if clicked == WM_LBUTTONUP {
                    SENDER.with(|sender| {
                        if let Some(sender) = sender.borrow().as_ref() {
                            let _ = sender.send(TrayEvent::Show);
                        }
                    });
                } else if clicked == WM_RBUTTONUP {
                    let menu = CreatePopupMenu().unwrap_or_default();

                    let _ = AppendMenuW(
                        menu,
                        MF_STRING,
                        MENU_SHOW as usize,
                        PCWSTR(wide("Show").as_ptr()),
                    );
                    let _ = AppendMenuW(
                        menu,
                        MF_STRING,
                        MENU_QUIT as usize,
                        PCWSTR(wide("Quit").as_ptr()),
                    );

                    let mut point = Default::default();
                    let _ = GetCursorPos(&mut point);

                    let _ = SetForegroundWindow(hwnd);

                    let chosen = TrackPopupMenu(
                        menu,
                        TPM_LEFTALIGN | TPM_BOTTOMALIGN,
                        point.x,
                        point.y,
                        Some(0),
                        hwnd,
                        None,
                    );

                    let _ = DestroyMenu(menu);

                    let event = match chosen.0 as u32 {
                        MENU_SHOW => Some(TrayEvent::Show),
                        MENU_QUIT => Some(TrayEvent::Quit),
                        _ => None,
                    };

                    if let Some(event) = event {
                        SENDER.with(|sender| {
                            if let Some(sender) = sender.borrow().as_ref() {
                                let _ = sender.send(event);
                            }
                        });
                    }
                }

                LRESULT(0)
            }
            WM_CREATE => LRESULT(0),
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }

    pub fn spawn(tooltip: &str) -> (Receiver<TrayEvent>, TrayHandle) {
        let (sender, receiver) = channel();
        let (hwnd_sender, hwnd_receiver) = channel();
        let tooltip = tooltip.to_string();

        std::thread::spawn(move || unsafe {
            SENDER.with(|slot| *slot.borrow_mut() = Some(sender));

            let class_name = wide("OpenDsTrayWindow");
            let instance = GetModuleHandleW(None).unwrap_or_default();

            let class = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(wndproc),
                hInstance: instance.into(),
                lpszClassName: PCWSTR(class_name.as_ptr()),
                ..Default::default()
            };

            RegisterClassExW(&class);

            let Ok(hwnd) = windows::Win32::UI::WindowsAndMessaging::CreateWindowExW(
                Default::default(),
                PCWSTR(class_name.as_ptr()),
                PCWSTR(wide("OpenDS").as_ptr()),
                WS_OVERLAPPEDWINDOW,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                None,
                None,
                Some(instance.into()),
                None,
            ) else {
                let _ = hwnd_sender.send(None);
                return;
            };

            let icon = LoadIconW(None, IDI_APPLICATION).unwrap_or_default();

            let mut tip = [0u16; 128];
            let tip_wide = wide(&tooltip);
            let copy_len = tip_wide.len().min(tip.len());
            tip[..copy_len].copy_from_slice(&tip_wide[..copy_len]);

            let data = NOTIFYICONDATAW {
                cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: hwnd,
                uID: ICON_ID,
                uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
                uCallbackMessage: WM_TRAY_CALLBACK,
                hIcon: icon,
                szTip: tip,
                ..Default::default()
            };

            let _ = Shell_NotifyIconW(NIM_ADD, &data);

            let _ = hwnd_sender.send(Some(hwnd.0 as isize));

            let mut message = MSG::default();

            while GetMessageW(&mut message, None, 0, 0).into() {
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }

            let cleanup = NOTIFYICONDATAW {
                cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: hwnd,
                uID: ICON_ID,
                ..Default::default()
            };

            let _ = Shell_NotifyIconW(NIM_DELETE, &cleanup);
        });

        let hwnd = hwnd_receiver
            .recv()
            .ok()
            .flatten()
            .map(|raw| HWND(raw as *mut core::ffi::c_void))
            .unwrap_or_default();

        (receiver, TrayHandle { hwnd })
    }
}

#[cfg(not(windows))]
pub use stub::{spawn, TrayHandle};

#[cfg(not(windows))]
mod stub {
    use std::sync::mpsc::{channel, Receiver};

    use super::TrayEvent;

    pub struct TrayHandle;

    pub fn spawn(_tooltip: &str) -> (Receiver<TrayEvent>, TrayHandle) {
        let (_sender, receiver) = channel();
        (receiver, TrayHandle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawning_and_dropping_the_tray_never_panics() {
        let (_receiver, _handle) = spawn("OpenDS");
    }
}
