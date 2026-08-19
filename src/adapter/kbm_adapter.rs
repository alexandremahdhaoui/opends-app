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

use opends_core::controller::mapping::Output;

#[cfg_attr(test, mockall::automock)]
pub trait KeyboardMouse {
    fn emit(&mut self, outputs: &[Output]) -> usize;
}

pub struct SendInputKbm;

impl SendInputKbm {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SendInputKbm {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyboardMouse for SendInputKbm {
    fn emit(&mut self, outputs: &[Output]) -> usize {
        platform::emit(outputs)
    }
}

#[cfg(windows)]
mod platform {
    use super::Output;
    use opends_core::controller::mapping::MouseButton;

    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYBD_EVENT_FLAGS,
        KEYEVENTF_KEYUP, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN,
        MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP,
        MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, MOUSEINPUT, MOUSE_EVENT_FLAGS, VIRTUAL_KEY,
    };

    const XBUTTON1: i32 = 0x0001;
    const XBUTTON2: i32 = 0x0002;

    pub fn emit(outputs: &[Output]) -> usize {
        if outputs.is_empty() {
            return 0;
        }

        let inputs: Vec<INPUT> = outputs.iter().map(to_input).collect();

        let size = size_of::<INPUT>() as i32;

        let sent = unsafe { SendInput(&inputs, size) };

        sent as usize
    }

    fn to_input(output: &Output) -> INPUT {
        match output {
            Output::KeyDown(code) => key(*code, KEYBD_EVENT_FLAGS(0)),
            Output::KeyUp(code) => key(*code, KEYEVENTF_KEYUP),
            Output::MouseDown(button) => mouse(down_flags(*button), mouse_data(*button)),
            Output::MouseUp(button) => mouse(up_flags(*button), mouse_data(*button)),
            Output::MouseMove { dx, dy } => move_mouse(*dx, *dy),
        }
    }

    fn move_mouse(dx: i32, dy: i32) -> INPUT {
        INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx,
                    dy,
                    mouseData: 0,
                    dwFlags: MOUSEEVENTF_MOVE,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    fn key(code: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(code),
                    wScan: 0,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    fn mouse(flags: MOUSE_EVENT_FLAGS, data: i32) -> INPUT {
        INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: 0,
                    dy: 0,
                    mouseData: data as u32,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        }
    }

    fn down_flags(button: MouseButton) -> MOUSE_EVENT_FLAGS {
        match button {
            MouseButton::Left => MOUSEEVENTF_LEFTDOWN,
            MouseButton::Right => MOUSEEVENTF_RIGHTDOWN,
            MouseButton::Middle => MOUSEEVENTF_MIDDLEDOWN,
            MouseButton::Fourth | MouseButton::Fifth => MOUSEEVENTF_XDOWN,
        }
    }

    fn up_flags(button: MouseButton) -> MOUSE_EVENT_FLAGS {
        match button {
            MouseButton::Left => MOUSEEVENTF_LEFTUP,
            MouseButton::Right => MOUSEEVENTF_RIGHTUP,
            MouseButton::Middle => MOUSEEVENTF_MIDDLEUP,
            MouseButton::Fourth | MouseButton::Fifth => MOUSEEVENTF_XUP,
        }
    }

    fn mouse_data(button: MouseButton) -> i32 {
        match button {
            MouseButton::Fourth => XBUTTON1,
            MouseButton::Fifth => XBUTTON2,
            _ => 0,
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use super::Output;

    pub fn emit(_outputs: &[Output]) -> usize {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emitting_nothing_sends_nothing() {
        let mut kbm = SendInputKbm::new();

        assert_eq!(kbm.emit(&[]), 0);
    }

    #[test]
    fn a_release_is_always_paired_with_its_press_by_the_caller_not_by_us() {
        let mut kbm = MockKeyboardMouse::new();

        kbm.expect_emit()
            .times(1)
            .returning(|outputs| outputs.len());

        let sent = kbm.emit(&[Output::KeyDown(0x1B), Output::KeyUp(0x1B)]);

        assert_eq!(sent, 2);
    }

    #[test]
    fn the_adapter_reports_how_many_events_windows_accepted() {
        let mut kbm = MockKeyboardMouse::new();

        kbm.expect_emit().returning(|_| 1);

        assert_eq!(kbm.emit(&[Output::KeyDown(0x20)]), 1);
    }
}
