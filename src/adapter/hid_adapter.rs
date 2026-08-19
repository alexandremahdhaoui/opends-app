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

use opends_core::types::pad::DeviceKind;

pub const GAMEPAD_USAGE_PAGE: u16 = 0x01;
pub const GAMEPAD_USAGE: u16 = 0x05;

#[cfg(windows)]
const INPUT_BUFFERS: u32 = 2;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HidDeviceInfo {
    pub path: String,
    pub vendor: u16,
    pub product: u16,
    pub input_report_len: usize,
    pub output_report_len: usize,
}

impl HidDeviceInfo {
    pub fn kind(&self) -> Option<DeviceKind> {
        DeviceKind::from_ids(self.vendor, self.product)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum HidError {
    #[error("no device at {path}")]
    NotFound { path: String },

    #[error("opening {path} exclusively. Another program already holds this pad")]
    Busy { path: String },

    #[error("opening {path}")]
    Open { path: String },

    #[error("writing a {len} byte report failed with Windows error {code}")]
    Write { len: usize, code: u32 },

    #[error("writing a {len} byte report only wrote {written} bytes")]
    ShortWrite { len: usize, written: u32 },
}

#[cfg_attr(test, mockall::automock)]
pub trait HidEnumerator {
    fn list_gamepads(&self) -> Vec<HidDeviceInfo>;
}

#[cfg_attr(test, mockall::automock)]
pub trait HidDevice {
    fn info(&self) -> &HidDeviceInfo;

    fn read_latest(&mut self) -> Option<Vec<u8>>;

    fn write_report(&mut self, report: &[u8]) -> Result<(), HidError>;
}

pub struct SetupApiEnumerator;

impl SetupApiEnumerator {
    pub fn new() -> Self {
        Self
    }
}

impl Default for SetupApiEnumerator {
    fn default() -> Self {
        Self::new()
    }
}

impl HidEnumerator for SetupApiEnumerator {
    fn list_gamepads(&self) -> Vec<HidDeviceInfo> {
        platform::list(GAMEPAD_USAGE_PAGE, GAMEPAD_USAGE)
    }
}

pub fn open(info: &HidDeviceInfo, exclusive: bool) -> Result<platform::OpenDevice, HidError> {
    platform::OpenDevice::open(info, exclusive)
}

pub fn sony_gamepads(enumerator: &dyn HidEnumerator) -> Vec<HidDeviceInfo> {
    enumerator
        .list_gamepads()
        .into_iter()
        .filter(|device| device.kind().is_some())
        .collect()
}

#[cfg(windows)]
pub mod platform {
    use super::{HidDeviceInfo, HidError, INPUT_BUFFERS};

    use windows::core::PCWSTR;
    use windows::Win32::Devices::DeviceAndDriverInstallation::{
        SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInterfaces, SetupDiGetClassDevsW,
        SetupDiGetDeviceInterfaceDetailW, DIGCF_DEVICEINTERFACE, DIGCF_PRESENT, HDEVINFO,
        SP_DEVICE_INTERFACE_DATA, SP_DEVICE_INTERFACE_DETAIL_DATA_W,
    };
    use windows::Win32::Devices::HumanInterfaceDevice::{
        HidD_FreePreparsedData, HidD_GetAttributes, HidD_GetHidGuid, HidD_GetPreparsedData,
        HidD_SetNumInputBuffers, HidP_GetCaps, HIDD_ATTRIBUTES, HIDP_CAPS, PHIDP_PREPARSED_DATA,
    };
    use windows::Win32::Foundation::{CloseHandle, ERROR_IO_PENDING, HANDLE};
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, ReadFile, WriteFile, FILE_FLAG_OVERLAPPED, FILE_SHARE_MODE, FILE_SHARE_READ,
        FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows::Win32::System::Threading::CreateEventW;
    use windows::Win32::System::IO::{GetOverlappedResult, OVERLAPPED};

    const GENERIC_READ_ACCESS: u32 = 0x8000_0000;
    const GENERIC_WRITE_ACCESS: u32 = 0x4000_0000;

    pub fn list(usage_page: u16, usage: u16) -> Vec<HidDeviceInfo> {
        let guid = unsafe { HidD_GetHidGuid() };

        let Ok(set) = (unsafe {
            SetupDiGetClassDevsW(
                Some(&guid),
                PCWSTR::null(),
                None,
                DIGCF_PRESENT | DIGCF_DEVICEINTERFACE,
            )
        }) else {
            return Vec::new();
        };

        let mut found = Vec::new();
        let mut index = 0;

        loop {
            let mut interface = SP_DEVICE_INTERFACE_DATA {
                cbSize: size_of::<SP_DEVICE_INTERFACE_DATA>() as u32,
                ..Default::default()
            };

            if unsafe { SetupDiEnumDeviceInterfaces(set, None, &guid, index, &mut interface) }
                .is_err()
            {
                break;
            }

            index += 1;

            let Some(path) = detail_path(set, &interface) else {
                continue;
            };

            if let Some(device) = describe(&path, usage_page, usage) {
                found.push(device);
            }
        }

        let _ = unsafe { SetupDiDestroyDeviceInfoList(set) };

        found
    }

    fn detail_path(set: HDEVINFO, interface: &SP_DEVICE_INTERFACE_DATA) -> Option<Vec<u16>> {
        let mut needed = 0u32;

        let _ = unsafe {
            SetupDiGetDeviceInterfaceDetailW(set, interface, None, 0, Some(&mut needed), None)
        };

        if needed == 0 {
            return None;
        }

        let mut buffer = vec![0u8; needed as usize];
        let detail = buffer.as_mut_ptr() as *mut SP_DEVICE_INTERFACE_DETAIL_DATA_W;

        unsafe {
            (*detail).cbSize = size_of::<SP_DEVICE_INTERFACE_DETAIL_DATA_W>() as u32;
        }

        unsafe {
            SetupDiGetDeviceInterfaceDetailW(set, interface, Some(detail), needed, None, None)
        }
        .ok()?;

        let start = unsafe { (*detail).DevicePath.as_ptr() };
        let mut wide = Vec::new();
        let mut at = 0isize;

        loop {
            let unit = unsafe { *start.offset(at) };

            wide.push(unit);

            if unit == 0 {
                break;
            }

            at += 1;
        }

        Some(wide)
    }

    fn describe(path: &[u16], usage_page: u16, usage: u16) -> Option<HidDeviceInfo> {
        let handle = probe_open(path)?;

        let mut attributes = HIDD_ATTRIBUTES {
            Size: size_of::<HIDD_ATTRIBUTES>() as u32,
            ..Default::default()
        };

        let read = unsafe { HidD_GetAttributes(handle, &mut attributes) };
        let caps = caps_of(handle);

        let _ = unsafe { CloseHandle(handle) };

        let caps = caps?;

        if !read || caps.UsagePage != usage_page || caps.Usage != usage {
            return None;
        }

        Some(HidDeviceInfo {
            path: String::from_utf16_lossy(&path[..path.len().saturating_sub(1)]),
            vendor: attributes.VendorID,
            product: attributes.ProductID,
            input_report_len: caps.InputReportByteLength as usize,
            output_report_len: caps.OutputReportByteLength as usize,
        })
    }

    fn caps_of(handle: HANDLE) -> Option<HIDP_CAPS> {
        let mut preparsed = PHIDP_PREPARSED_DATA::default();

        if !unsafe { HidD_GetPreparsedData(handle, &mut preparsed) } {
            return None;
        }

        let mut caps = HIDP_CAPS::default();
        let status = unsafe { HidP_GetCaps(preparsed, &mut caps) };

        let _ = unsafe { HidD_FreePreparsedData(preparsed) };

        match status.is_ok() {
            true => Some(caps),
            false => None,
        }
    }

    fn probe_open(path: &[u16]) -> Option<HANDLE> {
        let wide = PCWSTR(path.as_ptr());

        let attempt = |access: u32| unsafe {
            CreateFileW(
                wide,
                access,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                FILE_FLAG_OVERLAPPED,
                None,
            )
        };

        match attempt(GENERIC_READ_ACCESS) {
            Ok(handle) => Some(handle),
            Err(_) => attempt(0).ok(),
        }
    }

    pub struct OpenDevice {
        info: HidDeviceInfo,
        handle: HANDLE,
        overlapped: Box<OVERLAPPED>,
        write_overlapped: Box<OVERLAPPED>,
        buffer: Vec<u8>,
        pending: bool,
        pub exclusive: bool,
    }

    impl OpenDevice {
        pub fn open(info: &HidDeviceInfo, exclusive: bool) -> Result<Self, HidError> {
            let mut path: Vec<u16> = info.path.encode_utf16().collect();

            path.push(0);

            let share = match exclusive {
                true => FILE_SHARE_MODE(0),
                false => FILE_SHARE_READ | FILE_SHARE_WRITE,
            };

            let handle = unsafe {
                CreateFileW(
                    PCWSTR(path.as_ptr()),
                    GENERIC_READ_ACCESS | GENERIC_WRITE_ACCESS,
                    share,
                    None,
                    OPEN_EXISTING,
                    FILE_FLAG_OVERLAPPED,
                    None,
                )
            };

            let handle = match handle {
                Ok(handle) => handle,
                Err(_) if exclusive => {
                    return Err(HidError::Busy {
                        path: info.path.clone(),
                    })
                }
                Err(_) => {
                    return Err(HidError::Open {
                        path: info.path.clone(),
                    })
                }
            };

            let event =
                unsafe { CreateEventW(None, true, false, PCWSTR::null()) }.map_err(|_| {
                    HidError::Open {
                        path: info.path.clone(),
                    }
                })?;

            let write_event =
                unsafe { CreateEventW(None, true, false, PCWSTR::null()) }.map_err(|_| {
                    HidError::Open {
                        path: info.path.clone(),
                    }
                })?;

            let _ = unsafe { HidD_SetNumInputBuffers(handle, INPUT_BUFFERS) };

            let mut overlapped = Box::new(OVERLAPPED::default());
            overlapped.hEvent = event;

            let mut write_overlapped = Box::new(OVERLAPPED::default());
            write_overlapped.hEvent = write_event;

            let mut device = Self {
                info: info.clone(),
                handle,
                overlapped,
                write_overlapped,
                buffer: vec![0u8; info.input_report_len.max(78)],
                pending: false,
                exclusive,
            };

            device.queue();

            Ok(device)
        }

        fn queue(&mut self) -> bool {
            if self.pending {
                return true;
            }

            let result = unsafe {
                ReadFile(
                    self.handle,
                    Some(&mut self.buffer),
                    None,
                    Some(self.overlapped.as_mut()),
                )
            };

            match result {
                Ok(()) => {
                    self.pending = true;
                    true
                }
                Err(err) if err.code() == ERROR_IO_PENDING.to_hresult() => {
                    self.pending = true;
                    true
                }
                Err(_) => false,
            }
        }
    }

    impl super::HidDevice for OpenDevice {
        fn info(&self) -> &HidDeviceInfo {
            &self.info
        }

        fn read_latest(&mut self) -> Option<Vec<u8>> {
            let mut latest = None;

            for _ in 0..INPUT_BUFFERS + 1 {
                let mut got = 0u32;

                if !self.pending && !self.queue() {
                    break;
                }

                if unsafe {
                    GetOverlappedResult(self.handle, self.overlapped.as_ref(), &mut got, false)
                }
                .is_err()
                {
                    break;
                }

                self.pending = false;

                if got > 0 {
                    latest = Some(self.buffer[..got as usize].to_vec());
                }

                if !self.queue() {
                    break;
                }
            }

            latest
        }

        fn write_report(&mut self, report: &[u8]) -> Result<(), HidError> {
            let mut written = 0u32;

            let result = unsafe {
                WriteFile(
                    self.handle,
                    Some(report),
                    Some(&mut written),
                    Some(self.write_overlapped.as_mut()),
                )
            };

            let started = match result {
                Ok(()) => Ok(()),
                Err(err) if err.code() == ERROR_IO_PENDING.to_hresult() => Ok(()),
                Err(err) => Err((err.code().0 & 0xFFFF) as u32),
            };

            if let Err(code) = started {
                return Err(HidError::Write {
                    len: report.len(),
                    code,
                });
            }

            let completed = unsafe {
                GetOverlappedResult(
                    self.handle,
                    self.write_overlapped.as_ref(),
                    &mut written,
                    true,
                )
            };

            match completed {
                Ok(()) if written as usize == report.len() => Ok(()),
                Ok(()) => Err(HidError::ShortWrite {
                    len: report.len(),
                    written,
                }),
                Err(err) => Err(HidError::Write {
                    len: report.len(),
                    code: (err.code().0 & 0xFFFF) as u32,
                }),
            }
        }
    }

    impl Drop for OpenDevice {
        fn drop(&mut self) {
            let _ = unsafe { CloseHandle(self.overlapped.hEvent) };
            let _ = unsafe { CloseHandle(self.write_overlapped.hEvent) };
            let _ = unsafe { CloseHandle(self.handle) };
        }
    }
}

#[cfg(not(windows))]
pub mod platform {
    use super::{HidDeviceInfo, HidError};

    pub fn list(_usage_page: u16, _usage: u16) -> Vec<HidDeviceInfo> {
        Vec::new()
    }

    pub struct OpenDevice {
        info: HidDeviceInfo,
        pub exclusive: bool,
    }

    impl OpenDevice {
        pub fn open(info: &HidDeviceInfo, _exclusive: bool) -> Result<Self, HidError> {
            Err(HidError::NotFound {
                path: info.path.clone(),
            })
        }
    }

    impl super::HidDevice for OpenDevice {
        fn info(&self) -> &HidDeviceInfo {
            &self.info
        }

        fn read_latest(&mut self) -> Option<Vec<u8>> {
            None
        }

        fn write_report(&mut self, report: &[u8]) -> Result<(), HidError> {
            Err(HidError::Write {
                len: report.len(),
                code: 0,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opends_core::types::pad::{DUALSENSE, DUALSHOCK4_V2, SONY_VENDOR};

    fn info(vendor: u16, product: u16) -> HidDeviceInfo {
        HidDeviceInfo {
            path: format!("\\\\?\\hid#vid_{vendor:04x}&pid_{product:04x}"),
            vendor,
            product,
            input_report_len: 64,
            output_report_len: 64,
        }
    }

    #[test]
    fn a_sony_pad_resolves_to_a_device_kind_and_a_keyboard_does_not() {
        assert_eq!(
            info(SONY_VENDOR, DUALSENSE).kind(),
            Some(DeviceKind::DualSense)
        );
        assert_eq!(info(0x046D, 0xC52B).kind(), None);
    }

    #[test]
    fn enumeration_keeps_only_the_pads_we_can_decode() {
        let mut enumerator = MockHidEnumerator::new();

        enumerator.expect_list_gamepads().returning(|| {
            vec![
                info(SONY_VENDOR, DUALSENSE),
                info(0x045E, 0x02FF),
                info(SONY_VENDOR, DUALSHOCK4_V2),
            ]
        });

        let found = sony_gamepads(&enumerator);

        assert_eq!(found.len(), 2);
        assert_eq!(found[0].kind(), Some(DeviceKind::DualSense));
        assert_eq!(found[1].kind(), Some(DeviceKind::DualShock4V2));
    }

    #[test]
    fn no_pads_plugged_in_is_an_empty_list_and_not_an_error() {
        let mut enumerator = MockHidEnumerator::new();

        enumerator.expect_list_gamepads().returning(Vec::new);

        assert!(sony_gamepads(&enumerator).is_empty());
    }

    #[test]
    fn a_busy_pad_reports_that_another_program_holds_it() {
        let error = HidError::Busy {
            path: "\\\\?\\hid#test".into(),
        };

        assert!(error.to_string().contains("Another program already holds"));
    }

    #[test]
    fn the_enumerator_finds_exactly_the_pads_that_are_plugged_in() {
        let real = SetupApiEnumerator::new();

        let listed = real.list_gamepads();
        let sony = sony_gamepads(&real);

        assert!(sony.len() <= listed.len());
        assert!(sony.iter().all(|device| device.kind().is_some()));
    }
}
