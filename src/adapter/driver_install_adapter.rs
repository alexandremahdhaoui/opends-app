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

use std::path::{Path, PathBuf};

pub const HARDWARE_ID: &str = "Root\\OpenDsUHid";
pub const UNQUALIFIED_ID: &str = "OpenDsUHid";
pub const ARP_KEY: &str = "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\opends-uhid";
pub const PRODUCT_NAME: &str = "opends virtual gamepad driver";
pub const PRODUCT_VERSION: &str = "1.0.0.0";

pub const PACKAGE_FILES: &[&str] = &[
    "opends-uhid.dll",
    "opends-uhid.inf",
    "opends-uhid.cat",
    "opends.cer",
];

pub const APP_FILES: &[&str] = &["OpenDS.exe"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Component {
    Driver,
    App,
    DesktopShortcut,
    StartMenuShortcut,
}

impl Component {
    pub const ALL: &'static [Component] = &[
        Component::Driver,
        Component::App,
        Component::DesktopShortcut,
        Component::StartMenuShortcut,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Component::Driver => "Virtual gamepad driver",
            Component::App => "OpenDS",
            Component::DesktopShortcut => "Desktop shortcut",
            Component::StartMenuShortcut => "Start Menu shortcut",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Component::Driver => {
                "Lets games that only read XInput see your DualSense. Needs administrator once."
            }
            Component::App => "Reads the pad and maps it. The program you actually run.",
            Component::DesktopShortcut => "Put a shortcut to OpenDS on the desktop.",
            Component::StartMenuShortcut => "Put a shortcut to OpenDS in the Start Menu.",
        }
    }

    pub fn required(self) -> bool {
        matches!(self, Component::Driver | Component::App)
    }

    pub fn approximate_bytes(self) -> u64 {
        match self {
            Component::Driver => crate::adapter::payload_adapter::bytes_of_any(&[
                "opends-uhid.dll",
                "opends-uhid.inf",
                "opends-uhid.cat",
                "opends.cer",
            ])
            .unwrap_or(22 * 1024),
            Component::App => crate::adapter::payload_adapter::bytes_of_any(&["OpenDS.exe"])
                .unwrap_or(6 * 1024 * 1024),
            Component::DesktopShortcut | Component::StartMenuShortcut => 2 * 1024,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    pub driver: bool,
    pub app: bool,
    pub desktop_shortcut: bool,
    pub start_menu_shortcut: bool,
    pub install_dir: PathBuf,
}

impl Default for Selection {
    fn default() -> Self {
        Self {
            driver: true,
            app: true,
            desktop_shortcut: false,
            start_menu_shortcut: true,
            install_dir: install_dir(),
        }
    }
}

impl Selection {
    pub fn wants(&self, component: Component) -> bool {
        match component {
            Component::Driver => self.driver,
            Component::App => self.app,
            Component::DesktopShortcut => self.desktop_shortcut,
            Component::StartMenuShortcut => self.start_menu_shortcut,
        }
    }

    pub fn set(&mut self, component: Component, wanted: bool) {
        if component.required() && !wanted {
            return;
        }

        match component {
            Component::Driver => self.driver = wanted,
            Component::App => self.app = wanted,
            Component::DesktopShortcut => self.desktop_shortcut = wanted,
            Component::StartMenuShortcut => self.start_menu_shortcut = wanted,
        }
    }

    pub fn total_bytes(&self) -> u64 {
        Component::ALL
            .iter()
            .filter(|component| self.wants(**component))
            .map(|component| component.approximate_bytes())
            .sum()
    }

    pub fn nothing_selected(&self) -> bool {
        !Component::ALL.iter().any(|c| self.wants(*c))
    }
}

pub fn human_size(bytes: u64) -> String {
    match bytes {
        0..=1023 => format!("{bytes} B"),
        1024..=1_048_575 => format!("{:.0} KB", bytes as f64 / 1024.0),
        _ => format!("{:.1} MB", bytes as f64 / 1_048_576.0),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum InstallError {
    #[error("creating the shortcut {0}")]
    Shortcut(String),

    #[error("this needs administrator. Right click and Run as administrator.")]
    NotElevated,

    #[error("the package is incomplete. {0} is missing next to the installer.")]
    MissingFile(String),

    #[error("trusting our certificate in {store}")]
    TrustCertificate { store: String },

    #[error("creating the virtual device")]
    CreateDevice,

    #[error("installing the driver package. Windows said {code:#010x}")]
    InstallDriver { code: u32 },

    #[error("removing the driver package")]
    RemoveDriver,

    #[error("copying {name} into Program Files: {reason}")]
    Copy { name: String, reason: String },

    #[error("writing the uninstall entry")]
    Registry,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    CheckPackage,
    TrustCertificate,
    CreateDevice,
    InstallDriver,
    CopyFiles,
    Register,
    Done,
}

impl Step {
    pub const ORDER: &'static [Step] = &[
        Step::CheckPackage,
        Step::TrustCertificate,
        Step::CreateDevice,
        Step::InstallDriver,
        Step::CopyFiles,
        Step::Register,
        Step::Done,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Step::CheckPackage => "Checking the package",
            Step::TrustCertificate => "Trusting the certificate",
            Step::CreateDevice => "Creating the virtual device",
            Step::InstallDriver => "Installing the driver",
            Step::CopyFiles => "Copying to Program Files",
            Step::Register => "Registering the uninstaller",
            Step::Done => "Done",
        }
    }

    pub fn percent(self) -> u32 {
        let position = Self::ORDER
            .iter()
            .position(|step| *step == self)
            .unwrap_or(0);
        let last = Self::ORDER.len().saturating_sub(1).max(1);

        (position * 100 / last) as u32
    }
}

pub fn package_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn install_dir() -> PathBuf {
    let program_files =
        std::env::var("ProgramFiles").unwrap_or_else(|_| "C:\\Program Files".to_string());

    PathBuf::from(program_files).join("OpenDS")
}

pub fn missing_files(dir: &Path) -> Vec<String> {
    PACKAGE_FILES
        .iter()
        .filter(|leaf| !dir.join(leaf).exists())
        .map(|leaf| (*leaf).to_string())
        .collect()
}

#[cfg_attr(test, mockall::automock)]
pub trait DriverInstaller {
    fn is_elevated(&self) -> bool;

    fn trust_certificate(&self, der: &[u8]) -> Result<(), InstallError>;

    fn untrust_certificate(&self) -> usize;

    fn device_exists(&self) -> bool;

    fn create_device(&self, inf: &Path) -> Result<(), InstallError>;

    fn remove_device(&self) -> Result<(), InstallError>;

    fn install_driver(&self, inf: &Path) -> Result<bool, InstallError>;

    fn remove_driver(&self, inf: &Path) -> Result<bool, InstallError>;

    fn remove_stray_pads(&self) -> usize;

    fn installed_at(&self) -> Option<PathBuf>;

    fn register_uninstall(&self, dir: &Path) -> Result<(), InstallError>;

    fn unregister_uninstall(&self) -> Result<(), InstallError>;

    fn create_shortcut(&self, target: &Path, link: &Path) -> Result<(), InstallError>;

    fn remove_shortcuts(&self) -> usize;

    fn desktop_dir(&self) -> Option<PathBuf>;

    fn start_menu_dir(&self) -> Option<PathBuf>;
}

pub struct Progress<'a> {
    pub report: &'a mut dyn FnMut(Step, &str),
}

impl Progress<'_> {
    fn say(&mut self, step: Step, detail: &str) {
        (self.report)(step, detail);
    }
}

pub fn install(
    installer: &dyn DriverInstaller,
    package: &Path,
    progress: &mut Progress,
) -> Result<(), InstallError> {
    install_selected(installer, package, &Selection::default(), progress)
}

fn recreate_device(
    installer: &dyn DriverInstaller,
    inf: &Path,
    progress: &mut Progress,
) -> Result<(), InstallError> {
    if installer.device_exists() {
        progress.say(
            Step::CreateDevice,
            "removing the existing device so it is recreated fresh, not reconfigured",
        );

        let _ = installer.remove_device();
    }

    progress.say(Step::CreateDevice, "creating Root\\OpenDsUHid");

    installer.create_device(inf)
}

pub fn install_selected(
    installer: &dyn DriverInstaller,
    package: &Path,
    selection: &Selection,
    progress: &mut Progress,
) -> Result<(), InstallError> {
    if !installer.is_elevated() {
        return Err(InstallError::NotElevated);
    }

    progress.say(Step::CheckPackage, "looking for the driver files");

    if let Some(missing) = missing_files(package).first() {
        return Err(InstallError::MissingFile(missing.clone()));
    }

    match selection.driver {
        false => {
            progress.say(
                Step::TrustCertificate,
                "skipped, the driver was not selected",
            );
            progress.say(Step::CreateDevice, "skipped");
            progress.say(Step::InstallDriver, "skipped");
        }
        true => {
            progress.say(
                Step::TrustCertificate,
                "adding our certificate to the store",
            );

            let der = std::fs::read(package.join("opends.cer"))
                .map_err(|_| InstallError::MissingFile("opends.cer".to_string()))?;

            installer.trust_certificate(&der)?;

            let inf = package.join("opends-uhid.inf");

            recreate_device(installer, &inf, progress)?;

            progress.say(Step::InstallDriver, "handing the package to Windows");

            if installer.install_driver(&inf)? {
                progress.say(Step::InstallDriver, "Windows asked for a reboot");
            }
        }
    }

    let target = selection.install_dir.clone();

    progress.say(
        Step::CopyFiles,
        &format!("copying into {}", target.display()),
    );

    copy_package(package, &target)?;

    progress.say(Step::Register, "writing the uninstall entry");

    installer.register_uninstall(&target)?;

    let app = target.join(APP_FILES[0]);

    if selection.desktop_shortcut && app.exists() {
        if let Some(desktop) = installer.desktop_dir() {
            progress.say(Step::Register, "adding the desktop shortcut");
            installer.create_shortcut(&app, &desktop.join("OpenDS.lnk"))?;
        }
    }

    if selection.start_menu_shortcut && app.exists() {
        if let Some(menu) = installer.start_menu_dir() {
            progress.say(Step::Register, "adding the Start Menu shortcut");
            installer.create_shortcut(&app, &menu.join("OpenDS.lnk"))?;
        }
    }

    progress.say(Step::Done, "installed");

    Ok(())
}

pub fn uninstall(
    installer: &dyn DriverInstaller,
    installed_at: &Path,
    progress: &mut Progress,
) -> Result<(), InstallError> {
    if !installer.is_elevated() {
        return Err(InstallError::NotElevated);
    }

    progress.say(
        Step::CreateDevice,
        "removing stray virtual pads left by earlier runs",
    );

    let stray = installer.remove_stray_pads();

    progress.say(
        Step::CreateDevice,
        &format!("removed {stray} stray pad(s), removing the virtual device"),
    );

    let _ = installer.remove_device();

    let inf = installed_at.join("opends-uhid.inf");

    progress.say(Step::InstallDriver, "removing the driver package");

    if inf.exists() {
        match installer.remove_driver(&inf) {
            Ok(true) => progress.say(
                Step::InstallDriver,
                "Windows could not finish removing the driver package while something is \
                 still using it. It will finish after your next restart.",
            ),
            Ok(false) => {}
            Err(_) => {}
        }
    }

    progress.say(Step::TrustCertificate, "removing our certificate");

    let removed = installer.untrust_certificate();

    progress.say(
        Step::TrustCertificate,
        &format!("removed {removed} certificate(s)"),
    );

    progress.say(Step::Register, "removing the shortcuts");

    let removed_links = installer.remove_shortcuts();

    progress.say(
        Step::Register,
        &format!("removed {removed_links} shortcut(s), removing the uninstall entry"),
    );

    installer.unregister_uninstall()?;

    progress.say(Step::Done, "removed");

    Ok(())
}

const COPY_RETRIES: u32 = 60;
const COPY_RETRY_DELAY: std::time::Duration = std::time::Duration::from_millis(500);

fn retry_permission_denied<T>(
    retries: u32,
    delay: std::time::Duration,
    mut attempt_fn: impl FnMut() -> std::io::Result<T>,
) -> std::io::Result<T> {
    let mut attempt = 0;

    loop {
        match attempt_fn() {
            Ok(value) => return Ok(value),
            Err(error)
                if error.kind() == std::io::ErrorKind::PermissionDenied && attempt < retries =>
            {
                attempt += 1;
                std::thread::sleep(delay);
            }
            Err(error) => return Err(error),
        }
    }
}

fn copy_with_retry(source: &Path, target: &Path) -> std::io::Result<u64> {
    retry_permission_denied(COPY_RETRIES, COPY_RETRY_DELAY, || {
        std::fs::copy(source, target)
    })
}

fn copy_package(from: &Path, to: &Path) -> Result<(), InstallError> {
    std::fs::create_dir_all(to).map_err(|error| InstallError::Copy {
        name: to.display().to_string(),
        reason: error.to_string(),
    })?;

    let mut leaves: Vec<String> = PACKAGE_FILES
        .iter()
        .chain(APP_FILES.iter())
        .map(|leaf| leaf.to_string())
        .collect();

    if let Some(name) = std::env::current_exe().ok().and_then(|exe| {
        exe.file_name()
            .map(|name| name.to_string_lossy().to_string())
    }) {
        leaves.push(name);
    }

    for leaf in leaves {
        let source = from.join(&leaf);
        let target = to.join(&leaf);

        if !source.exists() || source == target {
            continue;
        }

        copy_with_retry(&source, &target).map_err(|error| InstallError::Copy {
            name: leaf.clone(),
            reason: error.to_string(),
        })?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_required_component_cannot_be_unticked() {
        let mut selection = Selection::default();

        selection.set(Component::Driver, false);
        selection.set(Component::App, false);

        assert!(selection.driver);
        assert!(selection.app);
    }

    #[test]
    fn an_optional_component_can_be_turned_on_and_off() {
        let mut selection = Selection::default();

        selection.set(Component::DesktopShortcut, true);
        assert!(selection.desktop_shortcut);

        selection.set(Component::DesktopShortcut, false);
        assert!(!selection.desktop_shortcut);
    }

    #[test]
    fn the_defaults_install_the_driver_and_the_app_and_a_start_menu_entry() {
        let selection = Selection::default();

        assert!(selection.driver);
        assert!(selection.app);
        assert!(selection.start_menu_shortcut);
        assert!(!selection.desktop_shortcut);
    }

    #[test]
    fn the_required_components_are_exactly_the_driver_and_the_app() {
        let required: Vec<&str> = Component::ALL
            .iter()
            .filter(|c| c.required())
            .map(|c| c.label())
            .collect();

        assert_eq!(required, vec!["Virtual gamepad driver", "OpenDS"]);
    }

    #[test]
    fn the_total_grows_when_a_component_is_added() {
        let mut selection = Selection::default();
        let before = selection.total_bytes();

        selection.set(Component::DesktopShortcut, true);

        assert!(selection.total_bytes() > before);
    }

    #[test]
    fn a_selection_can_never_be_empty_because_two_components_are_required() {
        let mut selection = Selection::default();

        for component in Component::ALL {
            selection.set(*component, false);
        }

        assert!(!selection.nothing_selected());
    }

    #[test]
    fn every_component_has_a_label_and_a_description_a_user_can_read() {
        for component in Component::ALL {
            assert!(!component.label().is_empty());
            assert!(component.description().len() > 20);
        }
    }

    #[test]
    fn sizes_are_rendered_in_units_a_person_reads() {
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(2 * 1024 * 1024), "2.0 MB");
        assert_eq!(human_size(22 * 1024), "22 KB");
    }

    #[test]
    fn the_app_binary_is_named_the_way_the_user_sees_it() {
        assert_eq!(APP_FILES, &["OpenDS.exe"]);
    }

    #[test]
    fn skipping_the_driver_never_touches_the_driver_apis() {
        let mut installer = MockDriverInstaller::new();

        installer.expect_is_elevated().return_const(true);
        installer.expect_trust_certificate().never();
        installer.expect_create_device().never();
        installer.expect_install_driver().never();

        let package = std::env::temp_dir().join("opends-skip-driver");
        std::fs::create_dir_all(&package).unwrap();

        for leaf in PACKAGE_FILES {
            std::fs::write(package.join(leaf), b"x").unwrap();
        }

        let selection = Selection {
            driver: false,
            app: true,
            desktop_shortcut: false,
            start_menu_shortcut: false,
            install_dir: std::env::temp_dir().join("opends-skip-target"),
        };

        installer.expect_register_uninstall().returning(|_| Ok(()));

        let mut seen = |_: Step, _: &str| {};
        let mut progress = Progress { report: &mut seen };

        let _ = install_selected(&installer, &package, &selection, &mut progress);
    }

    #[test]
    fn install_selected_copies_into_the_directory_the_selection_names_not_the_fixed_one() {
        let mut installer = MockDriverInstaller::new();

        installer.expect_is_elevated().return_const(true);

        let package = std::env::temp_dir().join("opends-custom-dir-package");
        std::fs::create_dir_all(&package).unwrap();

        for leaf in PACKAGE_FILES {
            std::fs::write(package.join(leaf), b"x").unwrap();
        }

        let custom_target = std::env::temp_dir().join("opends-custom-install-location");
        let _ = std::fs::remove_dir_all(&custom_target);

        let selection = Selection {
            driver: false,
            app: false,
            desktop_shortcut: false,
            start_menu_shortcut: false,
            install_dir: custom_target.clone(),
        };

        installer.expect_register_uninstall().returning(|_| Ok(()));

        let mut seen = |_: Step, _: &str| {};
        let mut progress = Progress { report: &mut seen };

        install_selected(&installer, &package, &selection, &mut progress).unwrap();

        assert!(
            custom_target.join(PACKAGE_FILES[0]).exists(),
            "expected the package to land in the selected directory, not the fixed default"
        );

        let _ = std::fs::remove_dir_all(&custom_target);
        let _ = std::fs::remove_dir_all(&package);
    }

    fn permission_denied() -> std::io::Error {
        std::io::Error::from(std::io::ErrorKind::PermissionDenied)
    }

    #[test]
    fn a_transient_permission_denial_recovers_within_the_retry_budget() {
        let mut attempts = 0;

        let result = retry_permission_denied(COPY_RETRIES, std::time::Duration::ZERO, || {
            attempts += 1;

            match attempts {
                1 | 2 => Err(permission_denied()),
                _ => Ok(attempts),
            }
        });

        assert_eq!(result.unwrap(), 3);
    }

    #[test]
    fn permission_denial_past_the_retry_budget_still_fails() {
        let result: std::io::Result<()> =
            retry_permission_denied(2, std::time::Duration::ZERO, || Err(permission_denied()));

        assert_eq!(
            result.unwrap_err().kind(),
            std::io::ErrorKind::PermissionDenied
        );
    }

    #[test]
    fn a_different_kind_of_error_is_never_retried() {
        let mut attempts = 0;

        let result: std::io::Result<()> =
            retry_permission_denied(5, std::time::Duration::ZERO, || {
                attempts += 1;
                Err(std::io::Error::from(std::io::ErrorKind::NotFound))
            });

        assert!(result.is_err());
        assert_eq!(attempts, 1);
    }

    #[test]
    fn an_existing_device_is_removed_and_recreated_rather_than_reconfigured_in_place() {
        let mut installer = MockDriverInstaller::new();

        installer.expect_device_exists().return_const(true);
        installer
            .expect_remove_device()
            .times(1)
            .returning(|| Ok(()));
        installer
            .expect_create_device()
            .times(1)
            .returning(|_| Ok(()));

        let mut seen = |_: Step, _: &str| {};
        let mut progress = Progress { report: &mut seen };

        let result = recreate_device(&installer, Path::new("opends-uhid.inf"), &mut progress);

        assert!(result.is_ok(), "{result:?}");
    }

    #[test]
    fn a_device_that_does_not_exist_yet_is_created_without_being_removed_first() {
        let mut installer = MockDriverInstaller::new();

        installer.expect_device_exists().return_const(false);
        installer.expect_remove_device().never();
        installer
            .expect_create_device()
            .times(1)
            .returning(|_| Ok(()));

        let mut seen = |_: Step, _: &str| {};
        let mut progress = Progress { report: &mut seen };

        let result = recreate_device(&installer, Path::new("opends-uhid.inf"), &mut progress);

        assert!(result.is_ok(), "{result:?}");
    }

    #[test]
    fn the_steps_run_from_zero_to_one_hundred_percent() {
        assert_eq!(Step::CheckPackage.percent(), 0);
        assert_eq!(Step::Done.percent(), 100);
    }

    #[test]
    fn every_step_has_a_label_and_a_percentage_that_never_goes_backwards() {
        let mut last = 0;

        for step in Step::ORDER {
            assert!(!step.label().is_empty());
            assert!(step.percent() >= last, "{:?} went backwards", step);
            last = step.percent();
        }
    }

    #[test]
    fn a_non_elevated_install_refuses_before_touching_anything() {
        let mut installer = MockDriverInstaller::new();

        installer.expect_is_elevated().return_const(false);
        installer.expect_trust_certificate().never();
        installer.expect_create_device().never();
        installer.expect_install_driver().never();

        let mut seen = |_: Step, _: &str| {};
        let mut progress = Progress { report: &mut seen };

        let error = install(&installer, Path::new("."), &mut progress).unwrap_err();

        assert!(matches!(error, InstallError::NotElevated));
    }

    #[test]
    fn a_missing_package_file_is_named_and_nothing_is_installed() {
        let mut installer = MockDriverInstaller::new();

        installer.expect_is_elevated().return_const(true);
        installer.expect_trust_certificate().never();
        installer.expect_install_driver().never();

        let mut seen = |_: Step, _: &str| {};
        let mut progress = Progress { report: &mut seen };

        let empty = std::env::temp_dir().join("opends-empty-package");
        std::fs::create_dir_all(&empty).unwrap();

        let error = install(&installer, &empty, &mut progress).unwrap_err();

        match error {
            InstallError::MissingFile(name) => assert!(PACKAGE_FILES.contains(&name.as_str())),
            other => panic!("expected a missing file, got {other}"),
        }
    }

    #[test]
    fn missing_files_lists_everything_that_is_absent() {
        let empty = std::env::temp_dir().join("opends-empty-package-2");
        std::fs::create_dir_all(&empty).unwrap();

        assert_eq!(missing_files(&empty).len(), PACKAGE_FILES.len());
    }

    #[test]
    fn an_uninstall_that_is_not_elevated_refuses_before_removing_anything() {
        let mut installer = MockDriverInstaller::new();

        installer.expect_is_elevated().return_const(false);
        installer.expect_remove_device().never();
        installer.expect_untrust_certificate().never();

        let mut seen = |_: Step, _: &str| {};
        let mut progress = Progress { report: &mut seen };

        assert!(matches!(
            uninstall(&installer, Path::new("."), &mut progress).unwrap_err(),
            InstallError::NotElevated
        ));
    }

    #[test]
    fn an_uninstall_carries_on_past_a_device_that_is_already_gone() {
        let mut installer = MockDriverInstaller::new();

        installer.expect_is_elevated().return_const(true);
        installer.expect_remove_stray_pads().returning(|| 0);
        installer
            .expect_remove_device()
            .returning(|| Err(InstallError::CreateDevice));
        installer.expect_untrust_certificate().returning(|| 2);
        installer
            .expect_unregister_uninstall()
            .times(1)
            .returning(|| Ok(()));
        installer.expect_remove_driver().never();
        installer.expect_remove_shortcuts().returning(|| 0);

        let mut steps = Vec::new();
        let mut seen = |step: Step, _: &str| steps.push(step);
        let mut progress = Progress { report: &mut seen };

        let nowhere = std::env::temp_dir().join("opends-not-installed-here");

        assert!(uninstall(&installer, &nowhere, &mut progress).is_ok());
        assert!(steps.contains(&Step::Done));
    }

    #[test]
    fn uninstall_removes_stray_pads_before_the_device_itself() {
        let mut installer = MockDriverInstaller::new();

        installer.expect_is_elevated().return_const(true);
        installer
            .expect_remove_stray_pads()
            .times(1)
            .returning(|| 2);
        installer.expect_remove_device().returning(|| Ok(()));
        installer.expect_untrust_certificate().returning(|| 0);
        installer.expect_unregister_uninstall().returning(|| Ok(()));
        installer.expect_remove_shortcuts().returning(|| 0);

        let mut messages = Vec::new();
        let mut seen = |step: Step, detail: &str| messages.push((step, detail.to_string()));
        let mut progress = Progress { report: &mut seen };

        let nowhere = std::env::temp_dir().join("opends-not-installed-here");

        assert!(uninstall(&installer, &nowhere, &mut progress).is_ok());
        assert!(messages
            .iter()
            .any(|(_, text)| text.contains("removed 2 stray pad")));
    }

    #[test]
    fn uninstall_tells_the_user_when_windows_still_needs_a_reboot_to_finish() {
        let mut installer = MockDriverInstaller::new();

        installer.expect_is_elevated().return_const(true);
        installer.expect_remove_stray_pads().returning(|| 0);
        installer.expect_remove_device().returning(|| Ok(()));
        installer.expect_untrust_certificate().returning(|| 0);
        installer.expect_unregister_uninstall().returning(|| Ok(()));
        installer.expect_remove_shortcuts().returning(|| 0);
        installer.expect_remove_driver().returning(|_| Ok(true));

        let mut messages = Vec::new();
        let mut seen = |step: Step, detail: &str| messages.push((step, detail.to_string()));
        let mut progress = Progress { report: &mut seen };

        let dir = std::env::temp_dir().join(format!(
            "opends-uninstall-reboot-test-{:x}",
            std::ptr::addr_of!(installer) as usize
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("opends-uhid.inf"), b"test").unwrap();

        assert!(uninstall(&installer, &dir, &mut progress).is_ok());

        let _ = std::fs::remove_dir_all(&dir);

        assert!(messages
            .iter()
            .any(|(_, text)| text.contains("after your next restart")));
    }

    #[test]
    fn the_uninstall_entry_points_at_the_program_files_copy() {
        assert!(install_dir().ends_with("OpenDS"));
        assert!(ARP_KEY.contains("Uninstall"));
    }

    #[test]
    fn the_package_list_matches_what_the_driver_repo_produces() {
        assert!(PACKAGE_FILES.contains(&"opends-uhid.dll"));
        assert!(PACKAGE_FILES.contains(&"opends-uhid.inf"));
        assert!(PACKAGE_FILES.contains(&"opends-uhid.cat"));
        assert!(PACKAGE_FILES.contains(&"opends.cer"));
    }
}
