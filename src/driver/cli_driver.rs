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

use std::time::{Duration, Instant};

use opends_core::types::pad::{button_name, ALL_BUTTONS};

use crate::adapter::hid_adapter::{self, HidEnumerator};
use crate::controller::pad_controller::Update;

pub struct AnalogThrottle {
    interval: Duration,
    last_shown: Option<Instant>,
}

impl AnalogThrottle {
    pub fn new(interval: Duration) -> Self {
        Self {
            interval,
            last_shown: None,
        }
    }

    pub fn should_show(&mut self, update: &Update) -> bool {
        if update.pressed != 0 || update.released != 0 {
            return true;
        }

        if !update.sticks_moved && !update.touch_moved {
            return false;
        }

        let now = Instant::now();
        let due = self
            .last_shown
            .is_none_or(|last| now.duration_since(last) >= self.interval);

        if due {
            self.last_shown = Some(now);
        }

        due
    }
}

impl Default for AnalogThrottle {
    fn default() -> Self {
        Self::new(Duration::from_secs(1))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    ListPads,
    WatchPad,
    DumpReport,
    VpadCheck,
    VpadReset,
    Map,
    Gui,
    Help,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    pub command: Command,
    pub exclusive: bool,
    pub profile_path: Option<String>,
}

pub fn parse_args<I, S>(args: I) -> Invocation
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut command = Command::Help;
    let mut exclusive = false;
    let mut profile_path = None;
    let mut expecting_profile = false;

    for arg in args {
        let arg = arg.as_ref();

        if expecting_profile {
            profile_path = Some(arg.to_string());
            expecting_profile = false;
            continue;
        }

        match arg {
            "--list-pads" => command = Command::ListPads,
            "--watch-pad" => command = Command::WatchPad,
            "--dump-report" => command = Command::DumpReport,
            "--vpad-check" => command = Command::VpadCheck,
            "--vpad-reset" => command = Command::VpadReset,
            "--map" => command = Command::Map,
            "--gui" => command = Command::Gui,
            "--exclusive" => exclusive = true,
            "--profile" => expecting_profile = true,
            _ => {}
        }
    }

    Invocation {
        command,
        exclusive,
        profile_path,
    }
}

pub fn help_text() -> String {
    [
        "opends reads a DualSense or a DualShock 4 and maps it.",
        "",
        "  --list-pads        show every Sony pad this machine can see",
        "  --watch-pad        print button presses until Ctrl+C",
        "  --dump-report      print one raw report and how it decoded",
        "  --vpad-check       create the virtual pad and ask XInput if it can see it",
        "  --vpad-reset       remove any stray virtual pads left by earlier runs. No reboot.",
        "  --map              turn pad buttons into keystrokes",
        "  --gui              show a window with pad status, minimizes to the tray",
        "  --profile <path>   the profile to map with",
        "  --exclusive        open the pad so nothing else can read it",
        "",
        "It opens no socket and installs nothing.",
    ]
    .join("\n")
}

pub fn render_pad_list(devices: &[hid_adapter::HidDeviceInfo]) -> String {
    if devices.is_empty() {
        return "no Sony pad found. Plug one in over USB or pair it over Bluetooth.".into();
    }

    let mut lines = vec![format!("{} pad(s) found", devices.len())];

    for device in devices {
        let name = device
            .kind()
            .map(|kind| kind.display_name())
            .unwrap_or("unknown");

        lines.push(format!(
            "  {name}  vendor={:04x} product={:04x} input_report={} path={}",
            device.vendor, device.product, device.input_report_len, device.path
        ));
    }

    lines.join("\n")
}

pub fn render_update(update: &Update) -> Option<String> {
    if !update.changed() {
        return None;
    }

    let mut parts = Vec::new();

    for (bit, name) in ALL_BUTTONS {
        if update.pressed & bit != 0 {
            parts.push(format!("+{name}"));
        }
        if update.released & bit != 0 {
            parts.push(format!("-{name}"));
        }
    }

    let (lx, ly) = update.state.left_stick.normalised();
    let (rx, ry) = update.state.right_stick.normalised();
    let motion = update.state.motion;

    Some(format!(
        "{} {:?} {} l2={} r2={} lstick={:.2},{:.2} rstick={:.2},{:.2} {} {} \
         gyro={},{},{} accel={},{},{}",
        update.kind.display_name(),
        update.transport,
        parts.join(" "),
        update.state.left_trigger,
        update.state.right_trigger,
        lx,
        ly,
        rx,
        ry,
        render_battery(&update.state.battery),
        render_touch(&update.state.touch),
        motion.gyro_pitch,
        motion.gyro_yaw,
        motion.gyro_roll,
        motion.accel_x,
        motion.accel_y,
        motion.accel_z,
    ))
}

pub fn render_touch(touch: &opends_core::types::pad::TouchPad) -> String {
    let finger = |touch: &opends_core::types::pad::Touch, label: &str| match touch.active {
        true => format!("{label}={},{}", touch.x, touch.y),
        false => format!("{label}=up"),
    };

    format!(
        "{} {}",
        finger(&touch.first, "touch1"),
        finger(&touch.second, "touch2")
    )
}

pub fn list_pads(enumerator: &dyn HidEnumerator) -> String {
    render_pad_list(&hid_adapter::sony_gamepads(enumerator))
}

pub fn render_battery(battery: &Option<opends_core::types::pad::Battery>) -> String {
    match battery {
        None => "battery=unreported".to_string(),
        Some(battery) => format!(
            "battery={}%{}",
            battery.percent,
            match battery.charging {
                true => " charging",
                false => "",
            }
        ),
    }
}

pub fn render_raw_report(
    kind: opends_core::types::pad::DeviceKind,
    caps_len: usize,
    report: &[u8],
) -> String {
    let hex: Vec<String> = report.iter().map(|byte| format!("{byte:02x}")).collect();

    let decoded = match opends_core::controller::decode::decode(kind, caps_len, report) {
        Ok((transport, state)) => format!(
            "transport={transport:?} buttons={:?} l2={} r2={} {}",
            state.held_names(),
            state.left_trigger,
            state.right_trigger,
            render_battery(&state.battery)
        ),
        Err(error) => format!("decode failed: {error}"),
    };

    format!(
        "{} caps_report_len={} got={} bytes\n  {}\n  {}",
        kind.display_name(),
        caps_len,
        report.len(),
        hex.join(" "),
        decoded
    )
}

pub fn describe_button(mask: u32) -> &'static str {
    button_name(mask).unwrap_or("unknown")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::hid_adapter::{HidDeviceInfo, MockHidEnumerator};
    use opends_core::types::pad::{
        DeviceKind, PadState, Transport, CIRCLE, CROSS, DUALSENSE, SONY_VENDOR,
    };

    fn info(vendor: u16, product: u16) -> HidDeviceInfo {
        HidDeviceInfo {
            path: "\\\\?\\hid#test".into(),
            vendor,
            product,
            input_report_len: 64,
            output_report_len: 64,
        }
    }

    #[test]
    fn no_arguments_shows_help_rather_than_doing_something_surprising() {
        let empty: [&str; 0] = [];

        assert_eq!(parse_args(empty).command, Command::Help);
    }

    #[test]
    fn the_exclusive_flag_is_off_unless_asked_for() {
        assert!(!parse_args(["--list-pads"]).exclusive);
        assert!(parse_args(["--list-pads", "--exclusive"]).exclusive);
    }

    #[test]
    fn an_unknown_flag_is_ignored_rather_than_fatal() {
        let parsed = parse_args(["--list-pads", "--not-a-flag"]);

        assert_eq!(parsed.command, Command::ListPads);
    }

    #[test]
    fn an_empty_machine_says_how_to_connect_a_pad_instead_of_printing_nothing() {
        let mut enumerator = MockHidEnumerator::new();
        enumerator.expect_list_gamepads().returning(Vec::new);

        let rendered = list_pads(&enumerator);

        assert!(rendered.contains("no Sony pad found"));
        assert!(rendered.contains("Bluetooth"));
    }

    #[test]
    fn a_listed_pad_shows_its_name_and_its_ids() {
        let mut enumerator = MockHidEnumerator::new();
        enumerator
            .expect_list_gamepads()
            .returning(|| vec![info(SONY_VENDOR, DUALSENSE)]);

        let rendered = list_pads(&enumerator);

        assert!(rendered.contains("1 pad(s) found"));
        assert!(rendered.contains("DualSense"));
        assert!(rendered.contains("vendor=054c"));
        assert!(rendered.contains("product=0ce6"));
    }

    #[test]
    fn a_non_sony_gamepad_is_left_out_of_the_list() {
        let mut enumerator = MockHidEnumerator::new();
        enumerator
            .expect_list_gamepads()
            .returning(|| vec![info(0x045E, 0x02FF)]);

        assert!(list_pads(&enumerator).contains("no Sony pad found"));
    }

    #[test]
    fn an_update_with_no_change_prints_nothing() {
        let update = Update {
            kind: DeviceKind::DualSense,
            transport: Transport::Usb,
            state: PadState::default(),
            pressed: 0,
            released: 0,
            sticks_moved: false,
            touch_moved: false,
        };

        assert!(render_update(&update).is_none());
    }

    #[test]
    fn a_stick_pushed_hard_prints_even_with_no_button_change() {
        let update = Update {
            kind: DeviceKind::DualSense,
            transport: Transport::Usb,
            state: PadState::default(),
            pressed: 0,
            released: 0,
            sticks_moved: true,
            touch_moved: false,
        };

        assert!(render_update(&update).is_some());
    }

    fn stick_only_update() -> Update {
        Update {
            kind: DeviceKind::DualSense,
            transport: Transport::Usb,
            state: PadState::default(),
            pressed: 0,
            released: 0,
            sticks_moved: true,
            touch_moved: false,
        }
    }

    fn button_update() -> Update {
        Update {
            kind: DeviceKind::DualSense,
            transport: Transport::Usb,
            state: PadState::default(),
            pressed: CIRCLE,
            released: 0,
            sticks_moved: false,
            touch_moved: false,
        }
    }

    #[test]
    fn a_button_is_never_throttled_no_matter_how_fast_it_repeats() {
        let mut throttle = AnalogThrottle::new(Duration::from_secs(60));

        assert!(throttle.should_show(&button_update()));
        assert!(throttle.should_show(&button_update()));
        assert!(throttle.should_show(&button_update()));
    }

    #[test]
    fn a_second_analog_only_update_within_the_interval_is_throttled() {
        let mut throttle = AnalogThrottle::new(Duration::from_secs(60));

        assert!(throttle.should_show(&stick_only_update()));
        assert!(!throttle.should_show(&stick_only_update()));
    }

    #[test]
    fn an_analog_only_update_shows_again_once_the_interval_passes() {
        let mut throttle = AnalogThrottle::new(Duration::from_millis(5));

        assert!(throttle.should_show(&stick_only_update()));
        std::thread::sleep(Duration::from_millis(15));
        assert!(throttle.should_show(&stick_only_update()));
    }

    #[test]
    fn a_button_alongside_a_throttled_stick_still_shows() {
        let mut throttle = AnalogThrottle::new(Duration::from_secs(60));

        throttle.should_show(&stick_only_update());

        let both = Update {
            sticks_moved: true,
            ..button_update()
        };

        assert!(throttle.should_show(&both));
    }

    #[test]
    fn a_touch_only_move_prints_even_with_no_button_or_stick_change() {
        let update = Update {
            kind: DeviceKind::DualSense,
            transport: Transport::Usb,
            state: PadState::default(),
            pressed: 0,
            released: 0,
            sticks_moved: false,
            touch_moved: true,
        };

        assert!(render_update(&update).is_some());
    }

    #[test]
    fn an_active_finger_reports_its_position() {
        use opends_core::types::pad::{Touch, TouchPad};

        let touch = TouchPad {
            first: Touch {
                active: true,
                id: 0,
                x: 500,
                y: 300,
            },
            ..TouchPad::default()
        };

        let rendered = render_touch(&touch);

        assert!(rendered.contains("touch1=500,300"));
        assert!(rendered.contains("touch2=up"));
    }

    #[test]
    fn an_inactive_finger_reports_up_rather_than_a_stale_position() {
        use opends_core::types::pad::TouchPad;

        assert!(render_touch(&TouchPad::default()).contains("touch1=up"));
    }

    #[test]
    fn a_printed_line_carries_the_gyro_and_accel_readings() {
        use opends_core::types::pad::Motion;

        let update = Update {
            kind: DeviceKind::DualSense,
            transport: Transport::Usb,
            state: PadState {
                motion: Motion {
                    gyro_pitch: 111,
                    ..Motion::default()
                },
                ..PadState::default()
            },
            pressed: CIRCLE,
            released: 0,
            sticks_moved: false,
            touch_moved: false,
        };

        let rendered = render_update(&update).unwrap();

        assert!(rendered.contains("gyro=111,"));
    }

    #[test]
    fn a_press_prints_a_plus_and_a_release_prints_a_minus() {
        let update = Update {
            kind: DeviceKind::DualSense,
            transport: Transport::Usb,
            state: PadState::default(),
            pressed: CIRCLE,
            released: CROSS,
            sticks_moved: false,
            touch_moved: false,
        };

        let line = render_update(&update).unwrap();

        assert!(line.contains("+Circle"));
        assert!(line.contains("-Cross"));
        assert!(line.contains("DualSense"));
    }

    #[test]
    fn the_help_text_says_it_opens_no_socket() {
        assert!(help_text().contains("no socket"));
    }

    #[test]
    fn the_profile_flag_takes_the_next_argument_as_its_path() {
        let parsed = parse_args(["--map", "--profile", "C:\\pads\\forza.json"]);

        assert_eq!(parsed.command, Command::Map);
        assert_eq!(parsed.profile_path.as_deref(), Some("C:\\pads\\forza.json"));
    }

    #[test]
    fn a_profile_path_that_looks_like_a_flag_is_still_taken_as_the_path() {
        let parsed = parse_args(["--profile", "--list-pads"]);

        assert_eq!(parsed.profile_path.as_deref(), Some("--list-pads"));
        assert_eq!(parsed.command, Command::Help);
    }

    #[test]
    fn a_dangling_profile_flag_leaves_no_path_rather_than_panicking() {
        let parsed = parse_args(["--map", "--profile"]);

        assert_eq!(parsed.command, Command::Map);
        assert!(parsed.profile_path.is_none());
    }

    #[test]
    fn the_help_text_names_every_command_that_can_be_parsed() {
        let help = help_text();

        for flag in [
            "--list-pads",
            "--watch-pad",
            "--map",
            "--gui",
            "--profile",
            "--exclusive",
        ] {
            assert!(help.contains(flag), "help does not mention {flag}");
        }
    }

    #[test]
    fn a_battery_that_was_never_reported_says_so_rather_than_showing_zero() {
        assert_eq!(render_battery(&None), "battery=unreported");
    }

    #[test]
    fn a_charging_battery_says_charging_and_a_resting_one_does_not() {
        use opends_core::types::pad::Battery;

        let charging = Some(Battery {
            percent: 50,
            charging: true,
            full: false,
        });
        let resting = Some(Battery {
            percent: 90,
            charging: false,
            full: false,
        });

        assert!(render_battery(&charging).contains("charging"));
        assert!(!render_battery(&resting).contains("charging"));
        assert!(render_battery(&resting).contains("90%"));
    }

    #[test]
    fn two_pads_are_both_listed_with_their_own_line() {
        let mut enumerator = MockHidEnumerator::new();
        enumerator.expect_list_gamepads().returning(|| {
            vec![
                info(SONY_VENDOR, DUALSENSE),
                info(SONY_VENDOR, opends_core::types::pad::DUALSHOCK4_V2),
            ]
        });

        let rendered = list_pads(&enumerator);

        assert!(rendered.contains("2 pad(s) found"));
        assert!(rendered.contains("DualSense"));
        assert!(rendered.contains("DualShock 4 v2"));
    }

    #[test]
    fn a_raw_report_dump_shows_the_hex_and_how_it_decoded() {
        let mut report = vec![0u8; 78];
        report[0] = 0x01;
        report[5] = 0x08;

        let rendered = render_raw_report(DeviceKind::DualSense, 78, &report);

        assert!(rendered.contains("caps_report_len=78"));
        assert!(rendered.contains("got=78"));
        assert!(rendered.contains("BluetoothBasic"));
    }

    #[test]
    fn a_report_that_cannot_decode_says_so_instead_of_pretending() {
        let rendered = render_raw_report(DeviceKind::DualSense, 78, &[0x99, 0x00]);

        assert!(rendered.contains("decode failed"));
    }

    #[test]
    fn an_unknown_button_mask_renders_as_unknown_rather_than_panicking() {
        assert_eq!(describe_button(1 << 30), "unknown");
    }

    #[test]
    fn every_command_flag_selects_its_own_command() {
        assert_eq!(parse_args(["--list-pads"]).command, Command::ListPads);
        assert_eq!(parse_args(["--watch-pad"]).command, Command::WatchPad);
        assert_eq!(parse_args(["--dump-report"]).command, Command::DumpReport);
        assert_eq!(parse_args(["--vpad-check"]).command, Command::VpadCheck);
        assert_eq!(parse_args(["--vpad-reset"]).command, Command::VpadReset);
        assert_eq!(parse_args(["--map"]).command, Command::Map);
        assert_eq!(parse_args(["--gui"]).command, Command::Gui);
    }

    #[test]
    fn a_later_command_flag_wins_over_an_earlier_one() {
        assert_eq!(
            parse_args(["--list-pads", "--watch-pad"]).command,
            Command::WatchPad
        );
    }

    #[test]
    fn every_button_has_a_printable_name() {
        for (bit, name) in ALL_BUTTONS {
            assert_eq!(describe_button(*bit), *name);
        }
    }
}
