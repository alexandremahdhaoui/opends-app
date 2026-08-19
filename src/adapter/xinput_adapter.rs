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
pub struct XinputSlot {
    pub slot: u32,
    pub packet: u32,
    pub buttons: u16,
    pub left_trigger: u8,
    pub right_trigger: u8,
    pub left_stick: (i16, i16),
}

pub const SLOTS: u32 = 4;

pub fn connected_slots() -> Vec<XinputSlot> {
    (0..SLOTS).filter_map(read_slot).collect()
}

pub fn describe(slots: &[XinputSlot]) -> String {
    if slots.is_empty() {
        return "XInput sees no controller in any of the four slots.".to_string();
    }

    let mut lines = vec![format!("XInput sees {} controller(s)", slots.len())];

    for slot in slots {
        lines.push(format!(
            "  slot {} packet={} buttons={:#06x} l2={} r2={} lstick={},{}",
            slot.slot,
            slot.packet,
            slot.buttons,
            slot.left_trigger,
            slot.right_trigger,
            slot.left_stick.0,
            slot.left_stick.1
        ));
    }

    lines.join("\n")
}

#[cfg(windows)]
fn read_slot(slot: u32) -> Option<XinputSlot> {
    use windows::Win32::UI::Input::XboxController::{XInputGetState, XINPUT_STATE};

    let mut state = XINPUT_STATE::default();

    if unsafe { XInputGetState(slot, &mut state) } != 0 {
        return None;
    }

    Some(XinputSlot {
        slot,
        packet: state.dwPacketNumber,
        buttons: state.Gamepad.wButtons.0,
        left_trigger: state.Gamepad.bLeftTrigger,
        right_trigger: state.Gamepad.bRightTrigger,
        left_stick: (state.Gamepad.sThumbLX, state.Gamepad.sThumbLY),
    })
}

#[cfg(not(windows))]
fn read_slot(_slot: u32) -> Option<XinputSlot> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_controllers_reads_as_a_sentence_not_an_empty_string() {
        assert!(describe(&[]).contains("no controller"));
    }

    #[test]
    fn a_slot_is_rendered_with_everything_needed_to_debug_it() {
        let slot = XinputSlot {
            slot: 0,
            packet: 42,
            buttons: 0x1000,
            left_trigger: 255,
            right_trigger: 0,
            left_stick: (-32768, 0),
        };

        let text = describe(&[slot]);

        assert!(text.contains("slot 0"));
        assert!(text.contains("packet=42"));
        assert!(text.contains("0x1000"));
        assert!(text.contains("l2=255"));
    }

    #[test]
    fn reading_every_slot_never_panics_whether_a_pad_is_present_or_not() {
        let slots = connected_slots();

        assert!(slots.len() <= SLOTS as usize);
        assert!(slots.iter().all(|slot| slot.slot < SLOTS));
    }
}
