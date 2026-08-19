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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisteredPad {
    pub instance_id: String,
    pub friendly_name: String,
    pub is_ours: bool,
}

#[cfg_attr(not(windows), allow(dead_code))]
const OURS_MARKER: &str = "OPENDSUHIDPAD0";

#[cfg_attr(not(windows), allow(dead_code))]
fn classify(instance_id: &str) -> bool {
    instance_id.to_uppercase().contains(OURS_MARKER)
}

#[cfg(windows)]
pub use platform::{list_registered_pads, remove_pad};

#[cfg(windows)]
mod platform {
    use windows::core::PCWSTR;
    use windows::Win32::Devices::DeviceAndDriverInstallation::{
        SetupDiCallClassInstaller, SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInfo,
        SetupDiGetClassDevsW, SetupDiGetDeviceInstanceIdW, SetupDiGetDeviceRegistryPropertyW,
        DIF_REMOVE, DIGCF_ALLCLASSES, DIGCF_PRESENT, SPDRP_FRIENDLYNAME, SPDRP_HARDWAREID,
        SP_DEVINFO_DATA,
    };

    use opends_core::controller::vpad::XINPUT_HARDWARE_ID;

    use super::{classify, RegisteredPad};

    fn wide_property(
        set: windows::Win32::Devices::DeviceAndDriverInstallation::HDEVINFO,
        data: &SP_DEVINFO_DATA,
        property: windows::Win32::Devices::DeviceAndDriverInstallation::SETUP_DI_REGISTRY_PROPERTY,
    ) -> Option<String> {
        let mut buffer = [0u8; 1024];
        let mut kind = 0u32;

        unsafe {
            SetupDiGetDeviceRegistryPropertyW(
                set,
                data,
                property,
                Some(&mut kind),
                Some(&mut buffer),
                None,
            )
        }
        .ok()?;

        let units: Vec<u16> = buffer
            .chunks_exact(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .take_while(|unit| *unit != 0)
            .collect();

        Some(String::from_utf16_lossy(&units))
    }

    pub fn list_registered_pads() -> Vec<RegisteredPad> {
        let mut pads = Vec::new();

        let Ok(set) = (unsafe {
            SetupDiGetClassDevsW(None, PCWSTR::null(), None, DIGCF_ALLCLASSES | DIGCF_PRESENT)
        }) else {
            return pads;
        };

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

            let Some(hardware_id) = wide_property(set, &data, SPDRP_HARDWAREID) else {
                continue;
            };

            if !hardware_id.eq_ignore_ascii_case(XINPUT_HARDWARE_ID) {
                continue;
            }

            let mut instance_buffer = [0u16; 512];
            let mut needed = 0u32;

            let instance_id = if unsafe {
                SetupDiGetDeviceInstanceIdW(
                    set,
                    &data,
                    Some(&mut instance_buffer),
                    Some(&mut needed),
                )
            }
            .is_ok()
            {
                let end = instance_buffer
                    .iter()
                    .position(|unit| *unit == 0)
                    .unwrap_or(instance_buffer.len());
                String::from_utf16_lossy(&instance_buffer[..end])
            } else {
                String::new()
            };

            let friendly_name = wide_property(set, &data, SPDRP_FRIENDLYNAME)
                .unwrap_or_else(|| "Unknown device".to_string());

            pads.push(RegisteredPad {
                is_ours: classify(&instance_id),
                instance_id,
                friendly_name,
            });
        }

        let _ = unsafe { SetupDiDestroyDeviceInfoList(set) };

        pads
    }

    pub fn remove_pad(instance_id: &str) -> bool {
        let Ok(set) = (unsafe {
            SetupDiGetClassDevsW(None, PCWSTR::null(), None, DIGCF_ALLCLASSES | DIGCF_PRESENT)
        }) else {
            return false;
        };

        let mut index = 0;
        let mut removed = false;

        loop {
            let mut data = SP_DEVINFO_DATA {
                cbSize: size_of::<SP_DEVINFO_DATA>() as u32,
                ..Default::default()
            };

            if unsafe { SetupDiEnumDeviceInfo(set, index, &mut data) }.is_err() {
                break;
            }

            index += 1;

            let mut instance_buffer = [0u16; 512];
            let mut needed = 0u32;

            if unsafe {
                SetupDiGetDeviceInstanceIdW(
                    set,
                    &data,
                    Some(&mut instance_buffer),
                    Some(&mut needed),
                )
            }
            .is_err()
            {
                continue;
            }

            let end = instance_buffer
                .iter()
                .position(|unit| *unit == 0)
                .unwrap_or(instance_buffer.len());
            let found_id = String::from_utf16_lossy(&instance_buffer[..end]);

            if !found_id.eq_ignore_ascii_case(instance_id) {
                continue;
            }

            if unsafe {
                SetupDiCallClassInstaller(DIF_REMOVE, set, Some(&data as *const SP_DEVINFO_DATA))
            }
            .is_ok()
            {
                removed = true;
            }

            break;
        }

        let _ = unsafe { SetupDiDestroyDeviceInfoList(set) };

        removed
    }
}

#[cfg(not(windows))]
pub fn list_registered_pads() -> Vec<RegisteredPad> {
    Vec::new()
}

#[cfg(not(windows))]
pub fn remove_pad(_instance_id: &str) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn our_instance_id_marker_is_classified_as_ours() {
        assert!(classify(
            "HID\\VID_045E&PID_02FF&IG_00\\1&1c69b544&7&OpendsUHidPad0"
        ));
    }

    #[test]
    fn our_instance_id_marker_is_recognised_case_insensitively() {
        assert!(classify(
            "HID\\VID_045E&PID_02FF&IG_00\\1&1c69b544&7&opendsuhidpad0"
        ));
    }

    #[test]
    fn a_real_xbox_pad_with_no_marker_is_not_ours() {
        assert!(!classify("HID\\VID_045E&PID_02FF&IG_00\\2&6ecba92&0&0000"));
    }
}
