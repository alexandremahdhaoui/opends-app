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

use opends_core::controller::decode::{self, DecodeError};
use opends_core::controller::output::{self, PadOutput};
use opends_core::types::pad::{
    ButtonMask, DeviceKind, PadState, Stick, Touch, TouchPad, Transport,
};

use crate::adapter::hid_adapter::{HidDevice, HidDeviceInfo, HidError};

const STICK_CHANGE_THRESHOLD: f32 = 0.15;
const TOUCH_CHANGE_THRESHOLD: u16 = 200;

fn stick_moved(from: Stick, to: Stick) -> bool {
    let (fx, fy) = from.normalised();
    let (tx, ty) = to.normalised();

    (fx - tx).abs() > STICK_CHANGE_THRESHOLD || (fy - ty).abs() > STICK_CHANGE_THRESHOLD
}

fn finger_moved(from: Touch, to: Touch) -> bool {
    if from.active != to.active {
        return true;
    }

    if !to.active {
        return false;
    }

    from.x.abs_diff(to.x) > TOUCH_CHANGE_THRESHOLD || from.y.abs_diff(to.y) > TOUCH_CHANGE_THRESHOLD
}

fn touch_moved(from: TouchPad, to: TouchPad) -> bool {
    finger_moved(from.first, to.first) || finger_moved(from.second, to.second)
}

pub struct Session {
    device: Box<dyn HidDevice>,
    kind: DeviceKind,
    transport: Option<Transport>,
    state: PadState,
    previous: PadState,
    reported: PadState,
    decode_failures: u32,
    last_error: Option<DecodeError>,
    last_output_error: Option<HidError>,
    output: PadOutput,
}

impl Session {
    pub fn new(device: Box<dyn HidDevice>, kind: DeviceKind) -> Self {
        Self {
            device,
            kind,
            transport: None,
            state: PadState::default(),
            previous: PadState::default(),
            reported: PadState::default(),
            decode_failures: 0,
            last_error: None,
            last_output_error: None,
            output: PadOutput::default(),
        }
    }

    pub fn kind(&self) -> DeviceKind {
        self.kind
    }

    pub fn transport(&self) -> Option<Transport> {
        self.transport
    }

    pub fn state(&self) -> &PadState {
        &self.state
    }

    pub fn info(&self) -> &HidDeviceInfo {
        self.device.info()
    }

    pub fn decode_failures(&self) -> u32 {
        self.decode_failures
    }

    pub fn last_error(&self) -> Option<&DecodeError> {
        self.last_error.as_ref()
    }

    pub fn last_output_error(&self) -> Option<&HidError> {
        self.last_output_error.as_ref()
    }

    pub fn send_output(&mut self, wanted: &PadOutput) -> bool {
        let Some(transport) = self.transport else {
            return false;
        };

        let report = output::build(self.kind, transport, wanted);

        match self.device.write_report(&report) {
            Ok(()) => {
                self.output = *wanted;
                true
            }
            Err(error) => {
                self.last_output_error = Some(error);
                false
            }
        }
    }

    pub fn output(&self) -> &PadOutput {
        &self.output
    }

    fn poll(&mut self) -> Option<Update> {
        let report = self.device.read_latest()?;

        let caps_len = self.device.info().input_report_len;

        match decode::decode(self.kind, caps_len, &report) {
            Ok((transport, state)) => {
                self.previous = self.state;
                self.state = state;
                self.transport = Some(transport);

                let pressed = state.pressed_since(&self.previous);
                let released = state.released_since(&self.previous);
                let sticks_moved = stick_moved(self.reported.left_stick, state.left_stick)
                    || stick_moved(self.reported.right_stick, state.right_stick);
                let touch_moved = touch_moved(self.reported.touch, state.touch);

                if pressed != 0 || released != 0 || sticks_moved || touch_moved {
                    self.reported = state;
                }

                Some(Update {
                    kind: self.kind,
                    transport,
                    state,
                    pressed,
                    released,
                    sticks_moved,
                    touch_moved,
                })
            }
            Err(error) => {
                self.decode_failures += 1;
                self.last_error = Some(error);

                None
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Update {
    pub kind: DeviceKind,
    pub transport: Transport,
    pub state: PadState,
    pub pressed: ButtonMask,
    pub released: ButtonMask,
    pub sticks_moved: bool,
    pub touch_moved: bool,
}

impl Update {
    pub fn changed(&self) -> bool {
        self.pressed != 0 || self.released != 0 || self.sticks_moved || self.touch_moved
    }
}

pub struct PadController {
    sessions: Vec<Session>,
}

impl PadController {
    pub fn new() -> Self {
        Self {
            sessions: Vec::new(),
        }
    }

    pub fn attach(&mut self, device: Box<dyn HidDevice>, kind: DeviceKind) {
        self.sessions.push(Session::new(device, kind));
    }

    pub fn attached(&self) -> usize {
        self.sessions.len()
    }

    pub fn sessions(&self) -> &[Session] {
        &self.sessions
    }

    pub fn send_output_to_all(&mut self, wanted: &PadOutput) -> usize {
        let mut sent = 0;

        for session in self.sessions.iter_mut() {
            if session.transport().is_none() || session.output() == wanted {
                continue;
            }

            if session.send_output(wanted) {
                sent += 1;
            }
        }

        sent
    }

    pub fn poll(&mut self) -> Vec<Update> {
        self.sessions.iter_mut().filter_map(Session::poll).collect()
    }
}

impl Default for PadController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::hid_adapter::MockHidDevice;
    use opends_core::types::pad::{CIRCLE, CROSS, DUALSENSE, SONY_VENDOR};

    fn test_info() -> HidDeviceInfo {
        HidDeviceInfo {
            path: "\\\\?\\hid#test".into(),
            vendor: SONY_VENDOR,
            product: DUALSENSE,
            input_report_len: 64,
            output_report_len: 64,
        }
    }

    fn neutral_report() -> Vec<u8> {
        let mut report = vec![0u8; 64];
        report[0] = 0x01;
        report[1] = 128;
        report[2] = 128;
        report[3] = 128;
        report[4] = 128;
        report[8] = 0x08;
        report
    }

    fn report_holding(face_bit: u8) -> Vec<u8> {
        let mut report = neutral_report();
        report[8] = 0x08 | (1 << face_bit);
        report
    }

    fn report_with_left_stick(x: u8, y: u8) -> Vec<u8> {
        let mut report = neutral_report();
        report[1] = x;
        report[2] = y;
        report
    }

    fn report_with_first_finger(x: u16, y: u16) -> Vec<u8> {
        let mut report = neutral_report();
        report[34] = 0x00;
        report[35] = (x & 0xFF) as u8;
        report[36] = (((x >> 8) & 0x0F) as u8) | (((y & 0x0F) as u8) << 4);
        report[37] = ((y >> 4) & 0xFF) as u8;
        report
    }

    fn report_with_no_finger() -> Vec<u8> {
        let mut report = neutral_report();
        report[34] = 0x80;
        report
    }

    fn device_returning(reports: Vec<Option<Vec<u8>>>) -> Box<MockHidDevice> {
        let mut device = MockHidDevice::new();
        let mut queue = reports.into_iter();

        device.expect_info().return_const(test_info());
        device
            .expect_read_latest()
            .returning(move || queue.next().flatten());

        Box::new(device)
    }

    #[test]
    fn a_pad_with_nothing_to_report_produces_no_update() {
        let mut controller = PadController::new();
        controller.attach(device_returning(vec![None]), DeviceKind::DualSense);

        assert!(controller.poll().is_empty());
    }

    #[test]
    fn the_first_report_reports_every_held_button_as_newly_pressed() {
        let mut controller = PadController::new();
        controller.attach(
            device_returning(vec![Some(report_holding(6))]),
            DeviceKind::DualSense,
        );

        let updates = controller.poll();

        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].pressed, CIRCLE);
        assert_eq!(updates[0].released, 0);
        assert_eq!(updates[0].transport, Transport::Usb);
    }

    #[test]
    fn holding_the_same_button_across_two_polls_reports_it_pressed_once() {
        let mut controller = PadController::new();
        controller.attach(
            device_returning(vec![Some(report_holding(6)), Some(report_holding(6))]),
            DeviceKind::DualSense,
        );

        let first = controller.poll();
        let second = controller.poll();

        assert_eq!(first[0].pressed, CIRCLE);
        assert_eq!(second[0].pressed, 0);
        assert!(second[0].state.is_down(CIRCLE));
        assert!(!second[0].changed());
    }

    #[test]
    fn letting_go_reports_the_button_as_released() {
        let mut controller = PadController::new();
        controller.attach(
            device_returning(vec![Some(report_holding(6)), Some(neutral_report())]),
            DeviceKind::DualSense,
        );

        controller.poll();
        let second = controller.poll();

        assert_eq!(second[0].released, CIRCLE);
        assert_eq!(second[0].pressed, 0);
        assert!(second[0].changed());
    }

    #[test]
    fn bluetooth_sensor_jitter_around_the_last_reported_position_does_not_count_as_a_stick_move() {
        let mut controller = PadController::new();
        controller.attach(
            device_returning(vec![
                Some(report_with_left_stick(128, 128)),
                Some(report_with_left_stick(130, 126)),
                Some(report_with_left_stick(126, 129)),
            ]),
            DeviceKind::DualSense,
        );

        controller.poll();

        let jitter1 = controller.poll();
        let jitter2 = controller.poll();

        assert!(!jitter1[0].sticks_moved);
        assert!(!jitter1[0].changed());
        assert!(!jitter2[0].sticks_moved);
        assert!(!jitter2[0].changed());
    }

    #[test]
    fn a_hard_stick_push_is_reported_even_with_no_button_change() {
        let mut controller = PadController::new();
        controller.attach(
            device_returning(vec![
                Some(report_with_left_stick(128, 128)),
                Some(report_with_left_stick(0, 128)),
            ]),
            DeviceKind::DualSense,
        );

        controller.poll();
        let pushed = controller.poll();

        assert_eq!(pushed.len(), 1);
        assert!(pushed[0].sticks_moved);
        assert_eq!(pushed[0].pressed, 0);
        assert_eq!(pushed[0].released, 0);
        assert!(pushed[0].changed());
    }

    #[test]
    fn a_returned_stick_reports_again_once_it_crosses_the_threshold_back() {
        let mut controller = PadController::new();
        controller.attach(
            device_returning(vec![
                Some(report_with_left_stick(128, 128)),
                Some(report_with_left_stick(0, 128)),
                Some(report_with_left_stick(128, 128)),
            ]),
            DeviceKind::DualSense,
        );

        controller.poll();
        let pushed = controller.poll();
        let released = controller.poll();

        assert!(pushed[0].sticks_moved);
        assert_eq!(released.len(), 1);
        assert!(released[0].sticks_moved);
    }

    #[test]
    fn a_finger_lifting_off_is_reported_even_though_position_stays_wherever_it_last_was() {
        let mut controller = PadController::new();
        controller.attach(
            device_returning(vec![
                Some(report_with_first_finger(500, 500)),
                Some(report_with_no_finger()),
            ]),
            DeviceKind::DualSense,
        );

        controller.poll();
        let lifted = controller.poll();

        assert!(lifted[0].touch_moved);
        assert!(!lifted[0].state.touch.first.active);
    }

    #[test]
    fn a_small_drift_in_finger_position_is_not_reported_as_movement() {
        let mut controller = PadController::new();
        controller.attach(
            device_returning(vec![
                Some(report_with_first_finger(500, 500)),
                Some(report_with_first_finger(510, 495)),
            ]),
            DeviceKind::DualSense,
        );

        controller.poll();
        let jitter = controller.poll();

        assert!(!jitter[0].touch_moved);
        assert!(!jitter[0].changed());
    }

    #[test]
    fn dragging_a_finger_a_real_distance_is_reported() {
        let mut controller = PadController::new();
        controller.attach(
            device_returning(vec![
                Some(report_with_first_finger(200, 200)),
                Some(report_with_first_finger(1800, 200)),
            ]),
            DeviceKind::DualSense,
        );

        controller.poll();
        let dragged = controller.poll();

        assert!(dragged[0].touch_moved);
        assert!(dragged[0].changed());
        assert_eq!(dragged[0].state.touch.first.x, 1800);
    }

    #[test]
    fn a_second_button_going_down_does_not_re_report_the_first() {
        let mut report = report_holding(6);
        report[8] |= 1 << 5;

        let mut controller = PadController::new();
        controller.attach(
            device_returning(vec![Some(report_holding(6)), Some(report)]),
            DeviceKind::DualSense,
        );

        controller.poll();
        let second = controller.poll();

        assert_eq!(second[0].pressed, CROSS);
        assert!(second[0].state.is_down(CIRCLE));
    }

    #[test]
    fn a_corrupt_report_is_counted_and_does_not_stop_the_next_poll() {
        let mut controller = PadController::new();
        controller.attach(
            device_returning(vec![Some(vec![0x99u8; 64]), Some(report_holding(6))]),
            DeviceKind::DualSense,
        );

        assert!(controller.poll().is_empty());

        let recovered = controller.poll();

        assert_eq!(recovered[0].pressed, CIRCLE);
        assert_eq!(controller.sessions()[0].decode_failures(), 1);
        assert!(controller.sessions()[0].last_error().is_some());
    }

    #[test]
    fn two_pads_are_polled_independently() {
        let mut controller = PadController::new();
        controller.attach(
            device_returning(vec![Some(report_holding(6))]),
            DeviceKind::DualSense,
        );
        controller.attach(device_returning(vec![None]), DeviceKind::DualShock4V2);

        let updates = controller.poll();

        assert_eq!(controller.attached(), 2);
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].kind, DeviceKind::DualSense);
    }

    #[test]
    fn output_is_not_sent_before_the_transport_is_known() {
        let mut device = MockHidDevice::new();
        device.expect_info().return_const(test_info());
        device.expect_write_report().never();

        let mut session = Session::new(Box::new(device), DeviceKind::DualSense);

        assert!(!session.send_output(&PadOutput::default()));
    }

    #[test]
    fn a_lightbar_colour_reaches_the_pad_once_the_transport_is_known() {
        let mut device = MockHidDevice::new();
        device.expect_info().return_const(test_info());
        device
            .expect_read_latest()
            .returning(|| Some(neutral_report()));
        device
            .expect_write_report()
            .withf(|report| report.contains(&255))
            .times(1)
            .returning(|_| Ok(()));

        let mut controller = PadController::new();
        controller.attach(Box::new(device), DeviceKind::DualSense);
        controller.poll();

        let red = PadOutput {
            lightbar: opends_core::controller::output::Colour::new(255, 0, 0),
            ..PadOutput::default()
        };

        assert_eq!(controller.send_output_to_all(&red), 1);
    }

    #[test]
    fn the_same_output_is_not_written_twice() {
        let mut device = MockHidDevice::new();
        device.expect_info().return_const(test_info());
        device
            .expect_read_latest()
            .returning(|| Some(neutral_report()));
        device.expect_write_report().times(1).returning(|_| Ok(()));

        let mut controller = PadController::new();
        controller.attach(Box::new(device), DeviceKind::DualSense);
        controller.poll();

        let red = PadOutput {
            lightbar: opends_core::controller::output::Colour::new(255, 0, 0),
            ..PadOutput::default()
        };

        assert_eq!(controller.send_output_to_all(&red), 1);
        assert_eq!(controller.send_output_to_all(&red), 0);
    }

    #[test]
    fn a_pad_that_refuses_a_write_does_not_remember_the_output_as_sent() {
        let mut device = MockHidDevice::new();
        device.expect_info().return_const(test_info());
        device
            .expect_read_latest()
            .returning(|| Some(neutral_report()));
        device.expect_write_report().returning(|report| {
            Err(HidError::Write {
                len: report.len(),
                code: 5,
            })
        });

        let mut controller = PadController::new();
        controller.attach(Box::new(device), DeviceKind::DualSense);
        controller.poll();

        let red = PadOutput {
            lightbar: opends_core::controller::output::Colour::new(255, 0, 0),
            ..PadOutput::default()
        };

        assert_eq!(controller.send_output_to_all(&red), 0);
        assert_eq!(controller.sessions()[0].output(), &PadOutput::default());
        assert!(controller.sessions()[0]
            .last_output_error()
            .unwrap()
            .to_string()
            .contains("Windows error 5"));
    }

    #[test]
    fn a_controller_with_no_pads_polls_to_nothing() {
        let mut controller = PadController::new();

        assert_eq!(controller.attached(), 0);
        assert!(controller.poll().is_empty());
    }
}
