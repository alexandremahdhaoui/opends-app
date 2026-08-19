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

use crate::adapter::driver_install_adapter::{
    install_dir, install_selected, missing_files, package_dir, uninstall, DriverInstaller,
    InstallError, Progress, Selection, Step, PRODUCT_NAME,
};
use crate::adapter::payload_adapter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Install,
    Uninstall,
}

pub fn self_test_requested<I, S>(args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    args.into_iter().any(|arg| arg.as_ref() == "--self-test")
}

pub fn probe_requested<I, S>(args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    args.into_iter().any(|arg| arg.as_ref() == "--probe")
}

pub fn mode_from_args<I, S>(args: I) -> Mode
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    for arg in args {
        match arg.as_ref() {
            "/uninstall" | "--uninstall" | "remove" => return Mode::Uninstall,
            _ => {}
        }
    }

    Mode::Install
}

pub fn welcome_text(mode: Mode) -> String {
    match mode {
        Mode::Install => [
            PRODUCT_NAME,
            "",
            "This adds a virtual Xbox gamepad so games that only read XInput can",
            "see your DualSense or DualShock 4.",
            "",
            "It installs one user mode driver written by us. It opens no socket,",
            "downloads nothing, and can be removed from Add or Remove Programs.",
        ]
        .join("\n"),
        Mode::Uninstall => [
            PRODUCT_NAME,
            "",
            "This removes the virtual gamepad driver, its device, and the",
            "certificate that was added to trust it.",
            "",
            "Your pad keeps working with OpenDS.exe for keyboard and mouse.",
        ]
        .join("\n"),
    }
}

pub fn action_label(mode: Mode) -> &'static str {
    match mode {
        Mode::Install => "Install",
        Mode::Uninstall => "Remove",
    }
}

pub fn source_dir() -> std::path::PathBuf {
    payload_adapter::source_dir(package_dir())
}

pub fn prepare_payload() -> Result<(), String> {
    if !payload_adapter::is_embedded() {
        return Ok(());
    }

    payload_adapter::unpack_to(&payload_adapter::staging_dir())
        .map_err(|error| format!("Could not unpack the installer. {error}"))
}

pub fn precheck(mode: Mode, installer: &dyn DriverInstaller) -> Option<String> {
    if !installer.is_elevated() {
        return Some(
            "This needs administrator rights. Close this and run it again with Run as administrator."
                .to_string(),
        );
    }

    if mode == Mode::Install {
        if let Err(problem) = prepare_payload() {
            return Some(problem);
        }

        let missing = missing_files(&source_dir());

        if !missing.is_empty() {
            return Some(format!(
                "These files are missing next to the installer: {}",
                missing.join(", ")
            ));
        }
    }

    None
}

pub fn run(
    mode: Mode,
    installer: &dyn DriverInstaller,
    report: &mut dyn FnMut(Step, &str),
) -> Result<(), InstallError> {
    run_with(mode, installer, &Selection::default(), report)
}

pub fn run_with(
    mode: Mode,
    installer: &dyn DriverInstaller,
    selection: &Selection,
    report: &mut dyn FnMut(Step, &str),
) -> Result<(), InstallError> {
    let mut progress = Progress { report };

    match mode {
        Mode::Install => install_selected(installer, &source_dir(), selection, &mut progress),
        Mode::Uninstall => {
            let target = installer.installed_at().unwrap_or_else(install_dir);

            uninstall(installer, &target, &mut progress)
        }
    }
}

pub fn outcome_text(mode: Mode, result: &Result<(), InstallError>) -> String {
    match (mode, result) {
        (Mode::Install, Ok(())) => [
            "Installed.",
            "",
            "Run OpenDS.exe and your pad will appear to games as an Xbox controller.",
        ]
        .join("\n"),
        (Mode::Uninstall, Ok(())) => {
            "Removed. Nothing of ours is left on this machine.".to_string()
        }
        (_, Err(error)) => format!("Failed.\n\n{error}\n\nNothing else was attempted."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::driver_install_adapter::MockDriverInstaller;

    #[test]
    fn probe_is_recognised_so_smart_app_control_can_be_tested_without_elevating() {
        assert!(probe_requested(["--probe"]));
    }

    #[test]
    fn probe_is_off_unless_asked_for() {
        let empty: [&str; 0] = [];
        assert!(!probe_requested(empty));
        assert!(!probe_requested(["--self-test", "--uninstall"]));
    }

    #[test]
    fn no_arguments_means_install() {
        let empty: [&str; 0] = [];

        assert_eq!(mode_from_args(empty), Mode::Install);
    }

    #[test]
    fn the_uninstall_string_windows_writes_selects_uninstall() {
        assert_eq!(mode_from_args(["/uninstall"]), Mode::Uninstall);
        assert_eq!(mode_from_args(["--uninstall"]), Mode::Uninstall);
        assert_eq!(mode_from_args(["remove"]), Mode::Uninstall);
    }

    #[test]
    fn the_self_test_flag_is_off_unless_it_is_passed() {
        let empty: [&str; 0] = [];

        assert!(!self_test_requested(empty));
        assert!(self_test_requested(["--self-test"]));
        assert!(!self_test_requested(["--uninstall"]));
    }

    #[test]
    fn the_welcome_text_says_it_opens_no_socket() {
        assert!(welcome_text(Mode::Install).contains("no socket"));
    }

    #[test]
    fn the_uninstall_welcome_says_the_pad_keeps_working() {
        assert!(welcome_text(Mode::Uninstall).contains("keeps working"));
    }

    #[test]
    fn the_button_says_what_it_will_do() {
        assert_eq!(action_label(Mode::Install), "Install");
        assert_eq!(action_label(Mode::Uninstall), "Remove");
    }

    #[test]
    fn a_non_elevated_run_is_caught_before_the_user_presses_anything() {
        let mut installer = MockDriverInstaller::new();
        installer.expect_is_elevated().return_const(false);

        let message = precheck(Mode::Install, &installer).unwrap();

        assert!(message.contains("Run as administrator"));
    }

    #[test]
    fn an_uninstall_does_not_demand_the_package_files_be_present() {
        let mut installer = MockDriverInstaller::new();
        installer.expect_is_elevated().return_const(true);

        assert!(precheck(Mode::Uninstall, &installer).is_none());
    }

    #[test]
    fn a_failure_message_says_nothing_else_was_attempted() {
        let text = outcome_text(Mode::Install, &Err(InstallError::CreateDevice));

        assert!(text.contains("Failed"));
        assert!(text.contains("Nothing else was attempted"));
    }

    #[test]
    fn a_successful_install_tells_the_user_what_to_run_next() {
        let text = outcome_text(Mode::Install, &Ok(()));

        assert!(text.contains("OpenDS.exe"));
    }

    #[test]
    fn every_step_reported_during_a_run_reaches_the_caller() {
        let mut installer = MockDriverInstaller::new();

        installer.expect_is_elevated().return_const(true);
        installer.expect_installed_at().returning(|| None);
        installer.expect_remove_stray_pads().returning(|| 0);
        installer.expect_remove_device().returning(|| Ok(()));
        installer.expect_untrust_certificate().returning(|| 1);
        installer.expect_unregister_uninstall().returning(|| Ok(()));
        installer.expect_remove_driver().returning(|_| Ok(false));
        installer.expect_remove_shortcuts().returning(|| 0);

        let mut seen = Vec::new();
        let mut report = |step: Step, _: &str| seen.push(step);

        run(Mode::Uninstall, &installer, &mut report).unwrap();

        assert!(seen.contains(&Step::Done));
        assert!(seen.len() >= 4);
    }

    #[test]
    fn uninstall_removes_the_driver_package_from_where_it_was_actually_installed() {
        let custom_target = std::env::temp_dir().join("opends-uninstall-custom-location");
        let _ = std::fs::remove_dir_all(&custom_target);
        std::fs::create_dir_all(&custom_target).unwrap();
        std::fs::write(custom_target.join("opends-uhid.inf"), b"x").unwrap();

        let mut installer = MockDriverInstaller::new();
        let expected_inf = custom_target.join("opends-uhid.inf");
        let recorded_target = custom_target.clone();

        installer.expect_is_elevated().return_const(true);
        installer
            .expect_installed_at()
            .returning(move || Some(recorded_target.clone()));
        installer.expect_remove_stray_pads().returning(|| 0);
        installer.expect_remove_device().returning(|| Ok(()));
        installer.expect_untrust_certificate().returning(|| 1);
        installer.expect_unregister_uninstall().returning(|| Ok(()));
        installer.expect_remove_shortcuts().returning(|| 0);
        installer
            .expect_remove_driver()
            .withf(move |inf| inf == expected_inf)
            .times(1)
            .returning(|_| Ok(false));

        let mut report = |_: Step, _: &str| {};

        run(Mode::Uninstall, &installer, &mut report).unwrap();

        let _ = std::fs::remove_dir_all(&custom_target);
    }

    #[test]
    fn falling_back_to_the_fixed_default_never_errors_whether_or_not_that_path_is_real() {
        let mut installer = MockDriverInstaller::new();

        installer.expect_is_elevated().return_const(true);
        installer.expect_installed_at().returning(|| None);
        installer.expect_remove_stray_pads().returning(|| 0);
        installer.expect_remove_device().returning(|| Ok(()));
        installer.expect_untrust_certificate().returning(|| 1);
        installer.expect_unregister_uninstall().returning(|| Ok(()));
        installer.expect_remove_shortcuts().returning(|| 0);
        installer.expect_remove_driver().returning(|_| Ok(false));

        let mut report = |_: Step, _: &str| {};

        run(Mode::Uninstall, &installer, &mut report).unwrap();
    }
}
