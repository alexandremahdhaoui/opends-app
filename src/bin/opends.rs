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

#![cfg_attr(windows, windows_subsystem = "windows")]

use opends_app::adapter::console_adapter as console;
use std::thread::sleep;
use std::time::Duration;

use opends_app::adapter::hid_adapter::{self, HidDevice, SetupApiEnumerator};
use opends_app::adapter::kbm_adapter::SendInputKbm;
use opends_app::adapter::profile_adapter::{self, FileProfiles, Profiles};
use opends_app::adapter::vpad_adapter::{UhidPad, VirtualPad};
use opends_app::controller::map_controller::MapController;
use opends_app::controller::pad_controller::PadController;
use opends_app::driver::cli_driver::{self, Command};
use opends_core::controller::mapping;
use opends_core::controller::output::{Colour, PadOutput, Rumble};

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();

    if arguments.is_empty() {
        opends_app::driver::app_gui::run();
        return;
    }

    console::attach();

    let invocation = cli_driver::parse_args(&arguments);
    let enumerator = SetupApiEnumerator::new();

    match invocation.command {
        Command::Gui => opends_app::driver::app_gui::run(),
        Command::Help => println!("{}", cli_driver::help_text()),
        Command::ListPads => println!("{}", cli_driver::list_pads(&enumerator)),
        Command::WatchPad => run(&enumerator, invocation.exclusive, None),
        Command::DumpReport => dump_report(&enumerator, invocation.exclusive),
        Command::VpadCheck => vpad_check(),
        Command::VpadReset => vpad_reset(),
        Command::Map => {
            let profile = match invocation.profile_path.as_deref() {
                Some(path) => match FileProfiles::new().load(path) {
                    Ok(profile) => profile,
                    Err(error) => {
                        println!("{error}");
                        return;
                    }
                },
                None => profile_adapter::default_profile(),
            };

            println!(
                "mapping with profile {:?}, {} button(s) bound",
                profile.name,
                profile.bindings.len()
            );

            run(
                &enumerator,
                invocation.exclusive,
                Some(MapController::new(profile)),
            )
        }
    }
}

fn open_virtual_pad() -> Result<UhidPad, opends_app::adapter::vpad_adapter::VpadError> {
    use opends_app::adapter::driver_install_adapter::DriverInstaller;
    use opends_app::adapter::driver_install_win::WindowsDriverInstaller;
    use opends_app::controller::vpad_recovery_controller::create_pad_with_recovery;

    let installer = WindowsDriverInstaller::new();

    create_pad_with_recovery(UhidPad::open, || {
        let removed = installer.remove_stray_pads();

        if removed > 0 {
            println!("cleared {removed} stray pad(s) left by an earlier run, retrying");
        }

        removed
    })
}

fn vpad_check() {
    use opends_app::adapter::xinput_adapter;
    use opends_core::types::pad::{PadState, Stick, CROSS};

    println!("vpad-check: this proves or disproves the whole XInput theory");
    println!();

    println!("--- what the driver logged ---");
    match driver_log() {
        Some(text) => println!("{text}"),
        None => println!(
            "  NO DRIVER LOG at C:\\Windows\\Temp\\opends-uhid.log\n  \
             WudfHost never loaded our DLL, so EvtDeviceAdd never ran.\n  \
             The package is installed but nothing is hosting it."
        ),
    }
    println!("--- end driver log ---");
    println!();

    let before = xinput_adapter::connected_slots();
    println!("before creating anything");
    println!("{}", xinput_adapter::describe(&before));
    println!();

    let mut pad = match open_virtual_pad() {
        Ok(pad) => {
            println!("virtual pad created");
            pad
        }
        Err(error) => {
            println!("could not create the virtual pad: {error}");
            println!();
            println!(
                "If this keeps happening, run --vpad-reset. If that finds nothing to \
                 remove, something is holding the pad open on purpose (Device Manager, \
                 a game, GameInputSvc) and closing that is the fix, not a reboot."
            );
            println!("Install the driver first with OpenDS-Setup.exe.");
            return;
        }
    };

    let held = PadState {
        buttons: CROSS,
        left_stick: Stick { x: 0, y: 128 },
        left_trigger: 255,
        ..PadState::default()
    };

    for _ in 0..40 {
        let _ = pad.submit(&held);
        sleep(Duration::from_millis(50));
    }

    let after = xinput_adapter::connected_slots();

    println!();
    println!("after submitting a report with Cross held and the stick hard left");
    println!("{}", xinput_adapter::describe(&after));
    println!();

    let appeared = after.len() > before.len();
    let reacted = after
        .iter()
        .any(|slot| slot.buttons != 0 || slot.left_trigger > 0);

    match (appeared, reacted) {
        (true, true) => println!("RESULT: PASS. XInput sees our pad and reads its input."),
        (true, false) => {
            println!("RESULT: PARTIAL. XInput sees a new pad but read no input from it.")
        }
        (false, _) => {
            println!("RESULT: FAIL. XInput did not see the virtual pad.");
            println!("The IG_00 hardware id did not make Windows load xinputhid.sys.");
        }
    }
}

fn vpad_reset() {
    use opends_app::adapter::driver_install_adapter::DriverInstaller;
    use opends_app::adapter::driver_install_win::WindowsDriverInstaller;

    let installer = WindowsDriverInstaller::new();
    let removed = installer.remove_stray_pads();

    match removed {
        0 => println!(
            "vpad-reset: nothing to remove. If --vpad-check still fails, something is \
             holding the pad open on purpose, not left over from an old run."
        ),
        n => println!("vpad-reset: removed {n} stray pad(s). No reboot needed."),
    }
}

fn driver_log() -> Option<String> {
    for path in [
        "C:\\Windows\\Temp\\opends-uhid.log",
        "C:\\Users\\Public\\opends-uhid.log",
        "C:\\Windows\\ServiceProfiles\\LocalService\\AppData\\Local\\Temp\\opends-uhid.log",
    ] {
        if let Ok(text) = std::fs::read_to_string(path) {
            if !text.trim().is_empty() {
                let tail: Vec<String> = text
                    .lines()
                    .rev()
                    .take(25)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .map(|line| format!("  {line}"))
                    .collect();

                return Some(tail.join("\n"));
            }
        }
    }

    None
}

fn dump_report(enumerator: &SetupApiEnumerator, exclusive: bool) {
    for info in hid_adapter::sony_gamepads(enumerator) {
        let Some(kind) = info.kind() else {
            continue;
        };

        let mut device = match hid_adapter::open(&info, exclusive) {
            Ok(device) => device,
            Err(error) => {
                println!("{}: {error}", kind.display_name());
                continue;
            }
        };

        println!("path={}", info.path);

        for attempt in 0..200 {
            if let Some(report) = device.read_latest() {
                println!(
                    "{}",
                    cli_driver::render_raw_report(kind, info.input_report_len, &report)
                );
                return;
            }

            if attempt == 199 {
                println!("no report arrived. Is the pad awake? Press a button and retry.");
            }

            sleep(Duration::from_millis(10));
        }
    }
}

fn run(enumerator: &SetupApiEnumerator, exclusive: bool, mut mapper: Option<MapController>) {
    let pads = hid_adapter::sony_gamepads(enumerator);

    if pads.is_empty() {
        println!("{}", cli_driver::list_pads(enumerator));
        return;
    }

    let mut controller = PadController::new();

    for info in &pads {
        let Some(kind) = info.kind() else {
            continue;
        };

        match hid_adapter::open(info, exclusive) {
            Ok(device) => {
                println!(
                    "watching {} exclusive={}",
                    kind.display_name(),
                    device.exclusive
                );
                controller.attach(Box::new(device) as Box<dyn HidDevice>, kind);
            }
            Err(error) => println!("skipping {}: {error}", kind.display_name()),
        }
    }

    if controller.attached() == 0 {
        println!("no pad could be opened");
        return;
    }

    let mut virtual_pad = match open_virtual_pad() {
        Ok(pad) => {
            println!("virtual Xbox pad created. XInput games will see it.");
            Some(pad)
        }
        Err(error) => {
            println!("{error}");
            None
        }
    };

    println!("press buttons. Ctrl+C to stop.");

    let mut kbm = SendInputKbm::new();
    let mut wanted = PadOutput {
        lightbar: Colour::new(0, 40, 120),
        left_trigger: mapper
            .as_ref()
            .map(|mapper| mapper.profile().left_trigger)
            .unwrap_or_default(),
        right_trigger: mapper
            .as_ref()
            .map(|mapper| mapper.profile().right_trigger)
            .unwrap_or_default(),
        ..PadOutput::default()
    };
    let mut lightbar_confirmed = false;
    let mut lightbar_error_shown = false;
    let mut throttle = cli_driver::AnalogThrottle::default();

    loop {
        if !lightbar_confirmed {
            if controller.send_output_to_all(&wanted) > 0 {
                println!("lightbar set");
                lightbar_confirmed = true;
            } else if !lightbar_error_shown {
                if let Some(error) = controller
                    .sessions()
                    .iter()
                    .find_map(|session| session.last_output_error())
                {
                    println!("could not set the lightbar: {error}");
                    lightbar_error_shown = true;
                }
            }
        }

        if let Some(pad) = virtual_pad.as_mut() {
            if let Some(rumble) = pad.take_rumble() {
                wanted.rumble = Rumble {
                    weak: rumble.right_motor,
                    strong: rumble.left_motor,
                };

                controller.send_output_to_all(&wanted);
            }
        }

        for update in controller.poll() {
            if throttle.should_show(&update) {
                if let Some(line) = cli_driver::render_update(&update) {
                    println!("{line}");
                }
            }

            if let Some(mapper) = mapper.as_mut() {
                mapper.apply(&update, &mut kbm);
            }

            if let Some(pad) = virtual_pad.as_mut() {
                let shaped = match mapper.as_ref() {
                    Some(mapper) => mapping::shape_sticks(mapper.profile(), &update.state),
                    None => update.state,
                };

                let _ = pad.submit(&shaped);
            }
        }

        sleep(Duration::from_millis(4));
    }
}
