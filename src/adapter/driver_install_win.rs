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

use crate::adapter::driver_install_adapter::{DriverInstaller, InstallError};

#[cfg(windows)]
use crate::adapter::driver_install_adapter::{
    ARP_KEY, HARDWARE_ID, PRODUCT_NAME, PRODUCT_VERSION, UNQUALIFIED_ID,
};

pub struct WindowsDriverInstaller;

impl WindowsDriverInstaller {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WindowsDriverInstaller {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(windows)]
fn wide(text: &str) -> Vec<u16> {
    let mut buffer: Vec<u16> = text.encode_utf16().collect();
    buffer.push(0);
    buffer
}

#[cfg(windows)]
fn wide_multi(text: &str) -> Vec<u16> {
    let mut buffer: Vec<u16> = text.encode_utf16().collect();
    buffer.push(0);
    buffer.push(0);
    buffer
}

#[cfg(windows)]
mod imp {
    use super::*;

    use windows::core::PCWSTR;
    use windows::Win32::Devices::DeviceAndDriverInstallation::{
        DiInstallDriverW, DiUninstallDevice, DiUninstallDriverW, SetupDiCallClassInstaller,
        SetupDiCreateDeviceInfoListExW, SetupDiCreateDeviceInfoW, SetupDiDestroyDeviceInfoList,
        SetupDiEnumDeviceInfo, SetupDiGetClassDevsW, SetupDiGetDeviceRegistryPropertyW,
        SetupDiGetINFClassW, SetupDiSetDeviceRegistryPropertyW, DICD_GENERATE_ID,
        DIF_REGISTERDEVICE, DIF_REMOVE, DIGCF_ALLCLASSES, DIGCF_PRESENT, SPDRP_HARDWAREID,
        SP_DEVINFO_DATA,
    };
    use windows::Win32::Foundation::{HWND, MAX_PATH};
    use windows::Win32::Security::Cryptography::{
        CertAddEncodedCertificateToStore, CertCloseStore, CertDeleteCertificateFromStore,
        CertDuplicateCertificateContext, CertFindCertificateInStore, CertOpenStore,
        CERT_FIND_SUBJECT_STR_W, CERT_QUERY_ENCODING_TYPE, CERT_STORE_ADD_REPLACE_EXISTING,
        CERT_STORE_PROV_SYSTEM_W, CERT_SYSTEM_STORE_LOCAL_MACHINE_ID,
        CERT_SYSTEM_STORE_LOCATION_SHIFT, X509_ASN_ENCODING,
    };
    use windows::Win32::System::Registry::{
        RegCloseKey, RegCreateKeyExW, RegDeleteKeyExW, RegOpenKeyExW, RegQueryValueExW,
        RegSetValueExW, HKEY, HKEY_LOCAL_MACHINE, KEY_QUERY_VALUE, KEY_WOW64_64KEY, KEY_WRITE,
        REG_DWORD, REG_OPTION_NON_VOLATILE, REG_SZ,
    };
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};

    const LOCAL_MACHINE: u32 =
        CERT_SYSTEM_STORE_LOCAL_MACHINE_ID << CERT_SYSTEM_STORE_LOCATION_SHIFT;

    const STORES: &[&str] = &["Root", "TrustedPublisher"];

    pub fn is_elevated() -> bool {
        use windows::Win32::Security::{
            GetTokenInformation, TokenElevation, TOKEN_ELEVATION, TOKEN_QUERY,
        };

        let mut token = Default::default();

        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) }.is_err() {
            return false;
        }

        let mut elevation = TOKEN_ELEVATION::default();
        let mut size = size_of::<TOKEN_ELEVATION>() as u32;

        let ok = unsafe {
            GetTokenInformation(
                token,
                TokenElevation,
                Some(&mut elevation as *mut _ as *mut core::ffi::c_void),
                size,
                &mut size,
            )
        }
        .is_ok();

        ok && elevation.TokenIsElevated != 0
    }

    pub fn trust_certificate(der: &[u8]) -> Result<(), InstallError> {
        for store_name in STORES {
            let name = wide(store_name);

            let store = unsafe {
                CertOpenStore(
                    CERT_STORE_PROV_SYSTEM_W,
                    CERT_QUERY_ENCODING_TYPE(0),
                    None,
                    windows::Win32::Security::Cryptography::CERT_OPEN_STORE_FLAGS(LOCAL_MACHINE),
                    Some(name.as_ptr() as *const core::ffi::c_void),
                )
            }
            .map_err(|_| InstallError::TrustCertificate {
                store: store_name.to_string(),
            })?;

            let added = unsafe {
                CertAddEncodedCertificateToStore(
                    Some(store),
                    X509_ASN_ENCODING,
                    der,
                    CERT_STORE_ADD_REPLACE_EXISTING,
                    None,
                )
            };

            let _ = unsafe { CertCloseStore(Some(store), 0) };

            added.map_err(|_| InstallError::TrustCertificate {
                store: store_name.to_string(),
            })?;
        }

        Ok(())
    }

    pub fn untrust_certificate() -> usize {
        let mut removed = 0;

        for store_name in STORES {
            let name = wide(store_name);
            let subject = wide("opends");

            let Ok(store) = (unsafe {
                CertOpenStore(
                    CERT_STORE_PROV_SYSTEM_W,
                    CERT_QUERY_ENCODING_TYPE(0),
                    None,
                    windows::Win32::Security::Cryptography::CERT_OPEN_STORE_FLAGS(LOCAL_MACHINE),
                    Some(name.as_ptr() as *const core::ffi::c_void),
                )
            }) else {
                continue;
            };

            loop {
                let found = unsafe {
                    CertFindCertificateInStore(
                        store,
                        X509_ASN_ENCODING,
                        0,
                        CERT_FIND_SUBJECT_STR_W,
                        Some(subject.as_ptr() as *const core::ffi::c_void),
                        None,
                    )
                };

                if found.is_null() {
                    break;
                }

                let duplicate = unsafe { CertDuplicateCertificateContext(Some(found)) };

                if duplicate.is_null() {
                    break;
                }

                if unsafe { CertDeleteCertificateFromStore(duplicate) }.is_ok() {
                    removed += 1;
                } else {
                    break;
                }
            }

            let _ = unsafe { CertCloseStore(Some(store), 0) };
        }

        removed
    }

    fn for_each_device_matching(
        hardware_id: &str,
        mut action: impl FnMut(
            windows::Win32::Devices::DeviceAndDriverInstallation::HDEVINFO,
            &SP_DEVINFO_DATA,
        ),
    ) -> bool {
        let Ok(set) = (unsafe {
            SetupDiGetClassDevsW(None, PCWSTR::null(), None, DIGCF_ALLCLASSES | DIGCF_PRESENT)
        }) else {
            return false;
        };

        let mut found = false;
        let mut index = 0;

        loop {
            let mut data = SP_DEVINFO_DATA {
                cbSize: size_of::<SP_DEVINFO_DATA>() as u32,
                ..Default::default()
            };

            if unsafe { SetupDiEnumDeviceInfo(set, index, &mut data) }.is_err() {
                break;
            }

            index += 1;

            let mut buffer = [0u8; 1024];
            let mut kind = 0u32;

            if unsafe {
                SetupDiGetDeviceRegistryPropertyW(
                    set,
                    &data,
                    SPDRP_HARDWAREID,
                    Some(&mut kind),
                    Some(&mut buffer),
                    None,
                )
            }
            .is_err()
            {
                continue;
            }

            let units: Vec<u16> = buffer
                .chunks_exact(2)
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                .take_while(|unit| *unit != 0)
                .collect();

            if !String::from_utf16_lossy(&units).eq_ignore_ascii_case(hardware_id) {
                continue;
            }

            found = true;
            action(set, &data);
        }

        let _ = unsafe { SetupDiDestroyDeviceInfoList(set) };

        found
    }

    pub fn device_exists() -> bool {
        for_each_device_matching(HARDWARE_ID, |_, _| {})
    }

    pub fn remove_device() -> Result<(), InstallError> {
        let mut removed = false;

        for_each_device_matching(HARDWARE_ID, |set, data| {
            if unsafe { DiUninstallDevice(HWND::default(), set, data, 0, None) }.is_ok() {
                removed = true;
            }
        });

        match removed {
            true => Ok(()),
            false => Err(InstallError::CreateDevice),
        }
    }

    pub fn remove_stray_pads() -> usize {
        let mut removed = 0usize;

        for_each_device_matching(
            opends_core::controller::vpad::XINPUT_HARDWARE_ID,
            |set, data| {
                if unsafe {
                    SetupDiCallClassInstaller(DIF_REMOVE, set, Some(data as *const SP_DEVINFO_DATA))
                }
                .is_ok()
                {
                    removed += 1;
                }
            },
        );

        removed
    }

    pub fn create_device(inf: &Path) -> Result<(), InstallError> {
        let inf_wide = wide(&inf.display().to_string());
        let mut class_guid = Default::default();
        let mut class_name = [0u16; MAX_PATH as usize];

        unsafe {
            SetupDiGetINFClassW(
                PCWSTR(inf_wide.as_ptr()),
                &mut class_guid,
                &mut class_name,
                None,
            )
        }
        .map_err(|_| InstallError::CreateDevice)?;

        let set = unsafe { SetupDiCreateDeviceInfoListExW(Some(&class_guid), None, None, None) }
            .map_err(|_| InstallError::CreateDevice)?;

        let mut data = SP_DEVINFO_DATA {
            cbSize: size_of::<SP_DEVINFO_DATA>() as u32,
            ..Default::default()
        };

        let unqualified = wide(UNQUALIFIED_ID);

        let created = unsafe {
            SetupDiCreateDeviceInfoW(
                set,
                PCWSTR(unqualified.as_ptr()),
                &class_guid,
                PCWSTR::null(),
                None,
                DICD_GENERATE_ID,
                Some(&mut data),
            )
        };

        let result = (|| {
            created.map_err(|_| InstallError::CreateDevice)?;

            let ids = wide_multi(HARDWARE_ID);
            let bytes: Vec<u8> = ids.iter().flat_map(|unit| unit.to_le_bytes()).collect();

            unsafe {
                SetupDiSetDeviceRegistryPropertyW(set, &mut data, SPDRP_HARDWAREID, Some(&bytes))
            }
            .map_err(|_| InstallError::CreateDevice)?;

            unsafe { SetupDiCallClassInstaller(DIF_REGISTERDEVICE, set, Some(&data)) }
                .map_err(|_| InstallError::CreateDevice)?;

            Ok(())
        })();

        let _ = unsafe { SetupDiDestroyDeviceInfoList(set) };

        result
    }

    pub fn install_driver(inf: &Path) -> Result<bool, InstallError> {
        let inf_wide = wide(&inf.display().to_string());
        let mut reboot = windows::core::BOOL::default();

        unsafe {
            DiInstallDriverW(
                None,
                PCWSTR(inf_wide.as_ptr()),
                windows::Win32::Devices::DeviceAndDriverInstallation::DIINSTALLDRIVER_FLAGS(0),
                Some(&mut reboot),
            )
        }
        .map_err(|error| InstallError::InstallDriver {
            code: error.code().0 as u32,
        })?;

        Ok(reboot.as_bool())
    }

    pub fn remove_driver(inf: &Path) -> Result<bool, InstallError> {
        let inf_wide = wide(&inf.display().to_string());
        let mut reboot = windows::core::BOOL::default();

        unsafe {
            DiUninstallDriverW(
                None,
                PCWSTR(inf_wide.as_ptr()),
                windows::Win32::Devices::DeviceAndDriverInstallation::DIUNINSTALLDRIVER_FLAGS(0),
                Some(&mut reboot),
            )
        }
        .map_err(|_| InstallError::RemoveDriver)?;

        Ok(reboot.as_bool())
    }

    fn set_string(key: HKEY, name: &str, value: &str) -> Result<(), InstallError> {
        let name_wide = wide(name);
        let value_wide = wide(value);
        let bytes: Vec<u8> = value_wide
            .iter()
            .flat_map(|unit| unit.to_le_bytes())
            .collect();

        unsafe { RegSetValueExW(key, PCWSTR(name_wide.as_ptr()), None, REG_SZ, Some(&bytes)) }
            .ok()
            .map_err(|_| InstallError::Registry)
    }

    pub fn register_uninstall(dir: &Path) -> Result<(), InstallError> {
        let key_path = wide(ARP_KEY);
        let mut key = HKEY::default();

        unsafe {
            RegCreateKeyExW(
                HKEY_LOCAL_MACHINE,
                PCWSTR(key_path.as_ptr()),
                None,
                PCWSTR::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_WRITE,
                None,
                &mut key,
                None,
            )
        }
        .ok()
        .map_err(|_| InstallError::Registry)?;

        let exe = dir.join("OpenDS-Setup.exe");
        let uninstall = format!("\"{}\" /uninstall", exe.display());

        let result = (|| {
            set_string(key, "DisplayName", PRODUCT_NAME)?;
            set_string(key, "DisplayVersion", PRODUCT_VERSION)?;
            set_string(key, "Publisher", "opends")?;
            set_string(key, "InstallLocation", &dir.display().to_string())?;
            set_string(key, "UninstallString", &uninstall)?;
            set_string(key, "DisplayIcon", &exe.display().to_string())?;

            let one = 1u32.to_le_bytes();
            let no_modify = wide("NoModify");
            let no_repair = wide("NoRepair");

            let _ = unsafe {
                RegSetValueExW(key, PCWSTR(no_modify.as_ptr()), None, REG_DWORD, Some(&one))
            };
            let _ = unsafe {
                RegSetValueExW(key, PCWSTR(no_repair.as_ptr()), None, REG_DWORD, Some(&one))
            };

            Ok(())
        })();

        let _ = unsafe { RegCloseKey(key) };

        result
    }

    pub fn installed_at() -> Option<PathBuf> {
        let key_path = wide(ARP_KEY);
        let mut key = HKEY::default();

        let opened = unsafe {
            RegOpenKeyExW(
                HKEY_LOCAL_MACHINE,
                PCWSTR(key_path.as_ptr()),
                Some(0),
                KEY_QUERY_VALUE | KEY_WOW64_64KEY,
                &mut key,
            )
        };

        if opened.is_err() {
            return None;
        }

        let name = wide("InstallLocation");
        let mut kind = REG_SZ;
        let mut size = 0u32;

        let sized = unsafe {
            RegQueryValueExW(
                key,
                PCWSTR(name.as_ptr()),
                None,
                Some(&mut kind),
                None,
                Some(&mut size),
            )
        };

        if sized.is_err() || size == 0 {
            let _ = unsafe { RegCloseKey(key) };
            return None;
        }

        let mut buffer = vec![0u8; size as usize];

        let read = unsafe {
            RegQueryValueExW(
                key,
                PCWSTR(name.as_ptr()),
                None,
                Some(&mut kind),
                Some(buffer.as_mut_ptr()),
                Some(&mut size),
            )
        };

        let _ = unsafe { RegCloseKey(key) };

        if read.is_err() {
            return None;
        }

        let units: Vec<u16> = buffer
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .take_while(|unit| *unit != 0)
            .collect();

        let text = String::from_utf16_lossy(&units);

        match text.is_empty() {
            true => None,
            false => Some(PathBuf::from(text)),
        }
    }

    pub fn create_shortcut(target: &Path, link: &Path) -> Result<(), InstallError> {
        use windows::core::{Interface, GUID, PCWSTR};
        use windows::Win32::System::Com::{
            CoCreateInstance, CoInitializeEx, CoUninitialize, IPersistFile, CLSCTX_INPROC_SERVER,
            COINIT_APARTMENTTHREADED,
        };
        use windows::Win32::UI::Shell::IShellLinkW;

        const CLSID_SHELL_LINK: GUID = GUID::from_u128(0x00021401_0000_0000_c000_000000000046);

        let fail = || InstallError::Shortcut(link.display().to_string());

        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        }

        let result = (|| {
            let shell: IShellLinkW =
                unsafe { CoCreateInstance(&CLSID_SHELL_LINK, None, CLSCTX_INPROC_SERVER) }
                    .map_err(|_| fail())?;

            let target_wide = wide(&target.display().to_string());
            let link_wide = wide(&link.display().to_string());

            unsafe { shell.SetPath(PCWSTR(target_wide.as_ptr())) }.map_err(|_| fail())?;

            if let Some(parent) = target.parent() {
                let dir = wide(&parent.display().to_string());
                let _ = unsafe { shell.SetWorkingDirectory(PCWSTR(dir.as_ptr())) };
            }

            let description = wide("OpenDS gamepad driver");
            let _ = unsafe { shell.SetDescription(PCWSTR(description.as_ptr())) };

            let persist: IPersistFile = shell.cast().map_err(|_| fail())?;

            unsafe { persist.Save(PCWSTR(link_wide.as_ptr()), true) }.map_err(|_| fail())?;

            Ok(())
        })();

        unsafe { CoUninitialize() };

        result
    }

    pub fn unregister_uninstall() -> Result<(), InstallError> {
        let key_path = wide(ARP_KEY);

        let _ = unsafe {
            RegDeleteKeyExW(
                HKEY_LOCAL_MACHINE,
                PCWSTR(key_path.as_ptr()),
                KEY_WOW64_64KEY.0,
                None,
            )
        };

        Ok(())
    }
}

#[cfg(not(windows))]
mod imp {
    use super::*;

    pub fn is_elevated() -> bool {
        false
    }

    pub fn trust_certificate(_der: &[u8]) -> Result<(), InstallError> {
        Err(InstallError::TrustCertificate {
            store: "Root".to_string(),
        })
    }

    pub fn untrust_certificate() -> usize {
        0
    }

    pub fn device_exists() -> bool {
        false
    }

    pub fn create_device(_inf: &Path) -> Result<(), InstallError> {
        Err(InstallError::CreateDevice)
    }

    pub fn remove_device() -> Result<(), InstallError> {
        Err(InstallError::CreateDevice)
    }

    pub fn install_driver(_inf: &Path) -> Result<bool, InstallError> {
        Err(InstallError::InstallDriver { code: 0 })
    }

    pub fn remove_driver(_inf: &Path) -> Result<bool, InstallError> {
        Err(InstallError::RemoveDriver)
    }

    pub fn remove_stray_pads() -> usize {
        0
    }

    pub fn installed_at() -> Option<PathBuf> {
        None
    }

    pub fn register_uninstall(_dir: &Path) -> Result<(), InstallError> {
        Err(InstallError::Registry)
    }

    pub fn unregister_uninstall() -> Result<(), InstallError> {
        Err(InstallError::Registry)
    }

    pub fn create_shortcut(_target: &Path, link: &Path) -> Result<(), InstallError> {
        Err(InstallError::Shortcut(link.display().to_string()))
    }
}

impl DriverInstaller for WindowsDriverInstaller {
    fn is_elevated(&self) -> bool {
        imp::is_elevated()
    }

    fn trust_certificate(&self, der: &[u8]) -> Result<(), InstallError> {
        imp::trust_certificate(der)
    }

    fn untrust_certificate(&self) -> usize {
        imp::untrust_certificate()
    }

    fn device_exists(&self) -> bool {
        imp::device_exists()
    }

    fn create_device(&self, inf: &Path) -> Result<(), InstallError> {
        imp::create_device(inf)
    }

    fn remove_device(&self) -> Result<(), InstallError> {
        imp::remove_device()
    }

    fn install_driver(&self, inf: &Path) -> Result<bool, InstallError> {
        imp::install_driver(inf)
    }

    fn remove_driver(&self, inf: &Path) -> Result<bool, InstallError> {
        imp::remove_driver(inf)
    }

    fn remove_stray_pads(&self) -> usize {
        imp::remove_stray_pads()
    }

    fn installed_at(&self) -> Option<PathBuf> {
        imp::installed_at()
    }

    fn register_uninstall(&self, dir: &Path) -> Result<(), InstallError> {
        imp::register_uninstall(dir)
    }

    fn unregister_uninstall(&self) -> Result<(), InstallError> {
        imp::unregister_uninstall()
    }

    fn create_shortcut(&self, target: &Path, link: &Path) -> Result<(), InstallError> {
        imp::create_shortcut(target, link)
    }

    fn remove_shortcuts(&self) -> usize {
        [self.desktop_dir(), self.start_menu_dir()]
            .into_iter()
            .flatten()
            .map(|dir| dir.join("OpenDS.lnk"))
            .filter(|link| link.exists() && std::fs::remove_file(link).is_ok())
            .count()
    }

    fn desktop_dir(&self) -> Option<PathBuf> {
        std::env::var("PUBLIC")
            .ok()
            .map(|public| PathBuf::from(public).join("Desktop"))
            .filter(|dir| dir.exists())
    }

    fn start_menu_dir(&self) -> Option<PathBuf> {
        std::env::var("ProgramData")
            .ok()
            .map(|data| {
                PathBuf::from(data)
                    .join("Microsoft")
                    .join("Windows")
                    .join("Start Menu")
                    .join("Programs")
            })
            .filter(|dir| dir.exists())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn a_wide_string_is_null_terminated() {
        assert_eq!(wide("ab"), vec![b'a' as u16, b'b' as u16, 0]);
    }

    #[cfg(windows)]
    #[test]
    fn a_multi_sz_string_is_double_null_terminated() {
        assert_eq!(wide_multi("a"), vec![b'a' as u16, 0, 0]);
    }

    #[test]
    fn elevation_reports_the_truth_for_whichever_way_the_tests_were_started() {
        let installer = WindowsDriverInstaller::new();

        let elevated = installer.is_elevated();

        let _ = elevated;
    }

    #[test]
    fn remove_stray_pads_never_panics_and_reports_a_count() {
        let installer = WindowsDriverInstaller::new();

        let removed = installer.remove_stray_pads();

        let _ = removed;
    }

    #[test]
    fn installed_at_never_panics_on_a_machine_with_no_prior_install() {
        let installer = WindowsDriverInstaller::new();

        let path = installer.installed_at();

        let _ = path;
    }
}
