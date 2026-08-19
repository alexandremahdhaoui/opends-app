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

use std::time::Instant;

use opends_core::controller::mapping::{self, Binding, Output, Profile, TimedStep};
use opends_core::types::pad::{ButtonMask, PadState, ALL_BUTTONS};

use crate::adapter::kbm_adapter::KeyboardMouse;
use crate::controller::pad_controller::Update;

struct PendingMacro {
    button: ButtonMask,
    steps: Vec<TimedStep>,
    started_at: Instant,
    fired_count: usize,
}

pub struct MapController {
    profile: Profile,
    emitted: u64,
    touch_baseline: Option<(u16, u16)>,
    pending_macro: Option<PendingMacro>,
}

impl MapController {
    pub fn new(profile: Profile) -> Self {
        Self {
            profile,
            emitted: 0,
            touch_baseline: None,
            pending_macro: None,
        }
    }

    pub fn profile(&self) -> &Profile {
        &self.profile
    }

    pub fn emitted(&self) -> u64 {
        self.emitted
    }

    pub fn outputs_for(&self, update: &Update) -> Vec<Output> {
        mapping::outputs_for(&self.profile, update.pressed, update.released)
    }

    pub fn apply(&mut self, update: &Update, kbm: &mut dyn KeyboardMouse) -> usize {
        self.apply_at(update, kbm, Instant::now())
    }

    fn apply_at(&mut self, update: &Update, kbm: &mut dyn KeyboardMouse, now: Instant) -> usize {
        let mut outputs = self.outputs_for(update);

        if let Some(mouse_move) = mapping::gyro_mouse_move(&self.profile, &update.state) {
            outputs.push(mouse_move);
        }

        let (touch_move, touch_baseline) =
            mapping::touch_mouse_move(&self.profile, self.touch_baseline, update.state.touch.first);
        self.touch_baseline = touch_baseline;

        if let Some(mouse_move) = touch_move {
            outputs.push(mouse_move);
        }

        outputs.extend(self.timed_macro_outputs(update, now));

        if outputs.is_empty() {
            return 0;
        }

        self.emitted += outputs.len() as u64;

        kbm.emit(&outputs)
    }

    fn timed_macro_outputs(&mut self, update: &Update, now: Instant) -> Vec<Output> {
        let mut outputs = Vec::new();

        if let Some(pending) = self.pending_macro.as_ref() {
            if update.released & pending.button != 0 {
                for step in &pending.steps[..pending.fired_count] {
                    push_release_step(&step.binding, &mut outputs);
                }

                self.pending_macro = None;
            }
        }

        for (mask, name) in ALL_BUTTONS {
            if update.pressed & mask == 0 {
                continue;
            }

            if let Some(Binding::TimedMacro { steps }) = self.profile.bindings.get(*name) {
                if let Some(pending) = self.pending_macro.take() {
                    for step in &pending.steps[..pending.fired_count] {
                        push_release_step(&step.binding, &mut outputs);
                    }
                }

                self.pending_macro = Some(PendingMacro {
                    button: *mask,
                    steps: steps.clone(),
                    started_at: now,
                    fired_count: 0,
                });
            }
        }

        if let Some(pending) = self.pending_macro.as_mut() {
            let elapsed_ms = now
                .saturating_duration_since(pending.started_at)
                .as_millis() as u64;
            let due = mapping::steps_due_by(&pending.steps, elapsed_ms);

            for step in &pending.steps[pending.fired_count..due] {
                push_press_step(&step.binding, &mut outputs);
            }

            pending.fired_count = due;
        }

        outputs
    }

    pub fn swap_profile(
        &mut self,
        next: Profile,
        held: &PadState,
        kbm: &mut dyn KeyboardMouse,
    ) -> usize {
        let mut releases = mapping::outputs_to_release(&self.profile, held);

        if let Some(pending) = self.pending_macro.take() {
            for step in &pending.steps[..pending.fired_count] {
                push_release_step(&step.binding, &mut releases);
            }
        }

        self.profile = next;

        if releases.is_empty() {
            return 0;
        }

        kbm.emit(&releases)
    }
}

fn push_press_step(binding: &Binding, outputs: &mut Vec<Output>) {
    match binding {
        Binding::Key { code } => outputs.push(Output::KeyDown(*code)),
        Binding::Mouse { button } => outputs.push(Output::MouseDown(*button)),
        Binding::Macro { .. } | Binding::TimedMacro { .. } | Binding::Unbound => {}
    }
}

fn push_release_step(binding: &Binding, outputs: &mut Vec<Output>) {
    match binding {
        Binding::Key { code } => outputs.push(Output::KeyUp(*code)),
        Binding::Mouse { button } => outputs.push(Output::MouseUp(*button)),
        Binding::Macro { .. } | Binding::TimedMacro { .. } | Binding::Unbound => {}
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::adapter::kbm_adapter::MockKeyboardMouse;
    use opends_core::controller::mapping::{Binding, TimedStep};
    use opends_core::types::pad::{DeviceKind, Transport, CIRCLE, CROSS};

    const ESCAPE: u16 = 0x1B;
    const SPACE: u16 = 0x20;

    fn profile() -> Profile {
        Profile::named("test")
            .bind("Circle", Binding::Key { code: ESCAPE })
            .bind("Cross", Binding::Key { code: SPACE })
    }

    fn update(pressed: u32, released: u32) -> Update {
        Update {
            kind: DeviceKind::DualSense,
            transport: Transport::Usb,
            state: PadState {
                buttons: pressed,
                ..PadState::default()
            },
            pressed,
            released,
            sticks_moved: false,
            touch_moved: false,
        }
    }

    #[test]
    fn a_mapped_press_reaches_the_keyboard_adapter() {
        let mut controller = MapController::new(profile());
        let mut kbm = MockKeyboardMouse::new();

        kbm.expect_emit()
            .withf(|outputs| outputs == [Output::KeyDown(ESCAPE)])
            .times(1)
            .returning(|outputs| outputs.len());

        assert_eq!(controller.apply(&update(CIRCLE, 0), &mut kbm), 1);
    }

    #[test]
    fn an_unmapped_press_never_touches_the_keyboard_adapter() {
        let mut controller = MapController::new(Profile::named("empty"));
        let mut kbm = MockKeyboardMouse::new();

        kbm.expect_emit().never();

        assert_eq!(controller.apply(&update(CIRCLE, 0), &mut kbm), 0);
    }

    #[test]
    fn the_count_of_emitted_events_tracks_what_was_sent() {
        let mut controller = MapController::new(profile());
        let mut kbm = MockKeyboardMouse::new();

        kbm.expect_emit().returning(|outputs| outputs.len());

        controller.apply(&update(CIRCLE, 0), &mut kbm);
        controller.apply(&update(CROSS, CIRCLE), &mut kbm);

        assert_eq!(controller.emitted(), 3);
    }

    #[test]
    fn swapping_a_profile_releases_every_key_the_old_one_was_holding() {
        let mut controller = MapController::new(profile());
        let mut kbm = MockKeyboardMouse::new();

        kbm.expect_emit()
            .withf(|outputs| {
                outputs.contains(&Output::KeyUp(ESCAPE)) && outputs.contains(&Output::KeyUp(SPACE))
            })
            .times(1)
            .returning(|outputs| outputs.len());

        let held = PadState {
            buttons: CIRCLE | CROSS,
            ..PadState::default()
        };

        controller.swap_profile(Profile::named("next"), &held, &mut kbm);

        assert_eq!(controller.profile().name, "next");
    }

    #[test]
    fn swapping_with_nothing_held_sends_nothing() {
        let mut controller = MapController::new(profile());
        let mut kbm = MockKeyboardMouse::new();

        kbm.expect_emit().never();

        controller.swap_profile(Profile::named("next"), &PadState::default(), &mut kbm);
    }

    fn tilted_update() -> Update {
        use opends_core::types::pad::Motion;

        Update {
            kind: DeviceKind::DualSense,
            transport: Transport::Usb,
            state: PadState {
                motion: Motion {
                    gyro_yaw: 200,
                    gyro_pitch: 50,
                    ..Motion::default()
                },
                ..PadState::default()
            },
            pressed: 0,
            released: 0,
            sticks_moved: false,
            touch_moved: false,
        }
    }

    #[test]
    fn a_profile_without_gyro_mouse_never_moves_the_mouse_from_tilting() {
        let mut controller = MapController::new(profile());
        let mut kbm = MockKeyboardMouse::new();

        kbm.expect_emit().never();

        assert_eq!(controller.apply(&tilted_update(), &mut kbm), 0);
    }

    #[test]
    fn a_profile_with_gyro_mouse_moves_the_mouse_on_every_tilted_tick() {
        let mut controller = MapController::new(profile().with_gyro_mouse(0.1));
        let mut kbm = MockKeyboardMouse::new();

        kbm.expect_emit()
            .withf(|outputs| outputs.contains(&Output::MouseMove { dx: 20, dy: 5 }))
            .times(1)
            .returning(|outputs| outputs.len());

        assert_eq!(controller.apply(&tilted_update(), &mut kbm), 1);
    }

    #[test]
    fn a_button_press_and_a_gyro_tilt_in_the_same_tick_both_reach_the_adapter() {
        let mut controller = MapController::new(profile().with_gyro_mouse(0.1));
        let mut kbm = MockKeyboardMouse::new();

        kbm.expect_emit()
            .withf(|outputs| {
                outputs.contains(&Output::KeyDown(ESCAPE))
                    && outputs.contains(&Output::MouseMove { dx: 20, dy: 5 })
            })
            .times(1)
            .returning(|outputs| outputs.len());

        let mut combined = tilted_update();
        combined.pressed = CIRCLE;
        combined.state.buttons = CIRCLE;

        assert_eq!(controller.apply(&combined, &mut kbm), 2);
    }

    fn touch_update(active: bool, x: u16, y: u16) -> Update {
        use opends_core::types::pad::{Touch, TouchPad};

        Update {
            kind: DeviceKind::DualSense,
            transport: Transport::Usb,
            state: PadState {
                touch: TouchPad {
                    first: Touch {
                        active,
                        id: 0,
                        x,
                        y,
                    },
                    ..TouchPad::default()
                },
                ..PadState::default()
            },
            pressed: 0,
            released: 0,
            sticks_moved: false,
            touch_moved: false,
        }
    }

    #[test]
    fn a_profile_without_touch_mouse_never_moves_the_mouse_from_a_drag() {
        let mut controller = MapController::new(profile());
        let mut kbm = MockKeyboardMouse::new();

        kbm.expect_emit().never();

        controller.apply(&touch_update(true, 300, 300), &mut kbm);
        assert_eq!(controller.apply(&touch_update(true, 400, 300), &mut kbm), 0);
    }

    #[test]
    fn a_profile_with_touch_mouse_moves_the_mouse_only_from_the_second_tick_onward() {
        let mut controller = MapController::new(profile().with_touch_mouse(1.0));
        let mut kbm = MockKeyboardMouse::new();

        kbm.expect_emit().never();
        assert_eq!(controller.apply(&touch_update(true, 300, 300), &mut kbm), 0);

        drop(kbm);
        let mut kbm = MockKeyboardMouse::new();
        kbm.expect_emit()
            .withf(|outputs| outputs.contains(&Output::MouseMove { dx: 50, dy: 0 }))
            .times(1)
            .returning(|outputs| outputs.len());

        assert_eq!(controller.apply(&touch_update(true, 350, 300), &mut kbm), 1);
    }

    #[test]
    fn lifting_and_relanding_the_finger_does_not_jump_the_mouse() {
        let mut controller = MapController::new(profile().with_touch_mouse(1.0));
        let mut kbm = MockKeyboardMouse::new();

        kbm.expect_emit().returning(|outputs| outputs.len());

        controller.apply(&touch_update(true, 300, 300), &mut kbm);
        controller.apply(&touch_update(false, 0, 0), &mut kbm);

        assert_eq!(controller.apply(&touch_update(true, 900, 900), &mut kbm), 0);
    }

    fn timed_macro_profile() -> Profile {
        Profile::named("timed macro").bind(
            "Circle",
            Binding::TimedMacro {
                steps: vec![
                    TimedStep {
                        binding: Binding::Key { code: ESCAPE },
                        delay_ms: 0,
                    },
                    TimedStep {
                        binding: Binding::Key { code: SPACE },
                        delay_ms: 50,
                    },
                ],
            },
        )
    }

    #[test]
    fn pressing_a_timed_macro_button_fires_only_the_zero_delay_step_immediately() {
        let mut controller = MapController::new(timed_macro_profile());
        let mut kbm = MockKeyboardMouse::new();

        kbm.expect_emit()
            .withf(|outputs| outputs == [Output::KeyDown(ESCAPE)])
            .times(1)
            .returning(|outputs| outputs.len());

        let now = Instant::now();
        assert_eq!(controller.apply_at(&update(CIRCLE, 0), &mut kbm, now), 1);
    }

    #[test]
    fn a_delayed_step_does_not_fire_before_its_delay_has_passed() {
        let mut controller = MapController::new(timed_macro_profile());
        let mut kbm = MockKeyboardMouse::new();
        kbm.expect_emit().returning(|outputs| outputs.len());

        let start = Instant::now();
        controller.apply_at(&update(CIRCLE, 0), &mut kbm, start);

        drop(kbm);
        let mut kbm = MockKeyboardMouse::new();
        kbm.expect_emit().never();

        assert_eq!(
            controller.apply_at(&update(0, 0), &mut kbm, start + Duration::from_millis(30)),
            0
        );
    }

    #[test]
    fn a_delayed_step_fires_once_its_delay_has_passed() {
        let mut controller = MapController::new(timed_macro_profile());
        let mut kbm = MockKeyboardMouse::new();
        kbm.expect_emit().returning(|outputs| outputs.len());

        let start = Instant::now();
        controller.apply_at(&update(CIRCLE, 0), &mut kbm, start);

        drop(kbm);
        let mut kbm = MockKeyboardMouse::new();
        kbm.expect_emit()
            .withf(|outputs| outputs == [Output::KeyDown(SPACE)])
            .times(1)
            .returning(|outputs| outputs.len());

        assert_eq!(
            controller.apply_at(&update(0, 0), &mut kbm, start + Duration::from_millis(50)),
            1
        );
    }

    #[test]
    fn releasing_before_the_delayed_step_fires_only_releases_what_actually_fired() {
        let mut controller = MapController::new(timed_macro_profile());
        let mut kbm = MockKeyboardMouse::new();
        kbm.expect_emit().returning(|outputs| outputs.len());

        let start = Instant::now();
        controller.apply_at(&update(CIRCLE, 0), &mut kbm, start);

        drop(kbm);
        let mut kbm = MockKeyboardMouse::new();
        kbm.expect_emit()
            .withf(|outputs| outputs == [Output::KeyUp(ESCAPE)])
            .times(1)
            .returning(|outputs| outputs.len());

        assert_eq!(
            controller.apply_at(
                &update(0, CIRCLE),
                &mut kbm,
                start + Duration::from_millis(10)
            ),
            1
        );
    }

    #[test]
    fn releasing_after_every_step_has_fired_releases_every_step() {
        let mut controller = MapController::new(timed_macro_profile());
        let mut kbm = MockKeyboardMouse::new();
        kbm.expect_emit().returning(|outputs| outputs.len());

        let start = Instant::now();
        controller.apply_at(&update(CIRCLE, 0), &mut kbm, start);
        controller.apply_at(&update(0, 0), &mut kbm, start + Duration::from_millis(50));

        drop(kbm);
        let mut kbm = MockKeyboardMouse::new();
        kbm.expect_emit()
            .withf(|outputs| {
                outputs.contains(&Output::KeyUp(ESCAPE)) && outputs.contains(&Output::KeyUp(SPACE))
            })
            .times(1)
            .returning(|outputs| outputs.len());

        assert_eq!(
            controller.apply_at(
                &update(0, CIRCLE),
                &mut kbm,
                start + Duration::from_millis(60)
            ),
            2
        );
    }

    #[test]
    fn a_button_with_no_timed_macro_binding_never_starts_one() {
        let mut controller = MapController::new(profile());
        let mut kbm = MockKeyboardMouse::new();

        kbm.expect_emit()
            .withf(|outputs| outputs == [Output::KeyDown(ESCAPE)])
            .times(1)
            .returning(|outputs| outputs.len());

        assert_eq!(
            controller.apply_at(&update(CIRCLE, 0), &mut kbm, Instant::now()),
            1
        );
    }
}
