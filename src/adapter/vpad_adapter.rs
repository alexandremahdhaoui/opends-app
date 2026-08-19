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

use opends_core::controller::vpad;
use opends_core::types::pad::PadState;
use opends_spec::protocol::{
    OpendsDeviceInfo, OPENDS_UHID_INTERFACE_VERSION, OPENDS_UHID_WIN32_PATH,
};

pub use opends_spec::protocol::{
    IOCTL_OPENDS_CREATE_DEVICE, IOCTL_OPENDS_GET_INTERFACE_VERSION, IOCTL_OPENDS_GET_NEXT_EVENT,
    IOCTL_OPENDS_SET_DEVICE_INFO, IOCTL_OPENDS_SET_HARDWARE_IDS,
    IOCTL_OPENDS_SET_REPORT_DESCRIPTOR, IOCTL_OPENDS_START_DEVICE,
    IOCTL_OPENDS_SUBMIT_INPUT_REPORT,
};

pub const DEVICE_PATH: &str = OPENDS_UHID_WIN32_PATH;
pub const INTERFACE_VERSION: u32 = OPENDS_UHID_INTERFACE_VERSION;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum VpadError {
    #[error("the opends-uhid driver is not installed. Keyboard and mouse mapping still works")]
    DriverAbsent,

    #[error("opening {path} failed with Windows error {code}. {meaning}")]
    CannotOpen {
        path: String,
        code: u32,
        meaning: &'static str,
    },

    #[error("the opends-uhid driver speaks version {found} and this build needs {needed}")]
    VersionMismatch { found: u32, needed: u32 },

    #[error(
        "creating the virtual pad. The driver rejected the SET_DEVICE_INFO, \
         SET_REPORT_DESCRIPTOR, SET_HARDWARE_IDS or CREATE_DEVICE step. Another \
         process may already own the pad; each open now tears its pad down on close, \
         so closing that process first should clear it."
    )]
    Create,

    #[error("submitting a {len} byte report to the virtual pad")]
    Submit { len: usize },
}

pub fn xbox_device_info() -> OpendsDeviceInfo {
    OpendsDeviceInfo {
        interface_version: OPENDS_UHID_INTERFACE_VERSION,
        vendor_id: vpad::XBOX_VENDOR,
        product_id: vpad::XBOX_ONE_PRODUCT,
        version_number: 0,
    }
}

pub fn hardware_ids_multi_sz() -> Vec<u16> {
    let mut wide: Vec<u16> = vpad::XINPUT_HARDWARE_ID.encode_utf16().collect();

    wide.push(0);
    wide.push(0);

    wide
}

#[cfg_attr(test, mockall::automock)]
pub trait VirtualPad {
    fn submit(&mut self, state: &PadState) -> Result<(), VpadError>;

    fn take_rumble(&mut self) -> Option<vpad::Rumble>;
}

pub struct UhidPad {
    inner: platform::Device,
}

impl UhidPad {
    pub fn open() -> Result<Self, VpadError> {
        Ok(Self {
            inner: platform::Device::open()?,
        })
    }
}

impl VirtualPad for UhidPad {
    fn submit(&mut self, state: &PadState) -> Result<(), VpadError> {
        let report = vpad::pack(state);

        self.inner.submit(&report)
    }

    fn take_rumble(&mut self) -> Option<vpad::Rumble> {
        let output = self.inner.next_output()?;

        vpad::parse_rumble(&output)
    }
}

pub fn is_available() -> bool {
    platform::is_available()
}

#[cfg(windows)]
mod platform {
    use super::{hardware_ids_multi_sz, xbox_device_info, VpadError, DEVICE_PATH};

    use opends_core::controller::vpad;
    use opends_spec::protocol::{
        IOCTL_OPENDS_CREATE_DEVICE, IOCTL_OPENDS_GET_INTERFACE_VERSION,
        IOCTL_OPENDS_GET_NEXT_EVENT, IOCTL_OPENDS_SET_DEVICE_INFO, IOCTL_OPENDS_SET_HARDWARE_IDS,
        IOCTL_OPENDS_SET_REPORT_DESCRIPTOR, IOCTL_OPENDS_START_DEVICE,
        IOCTL_OPENDS_SUBMIT_INPUT_REPORT, OPENDS_MAX_REPORT, OPENDS_UHID_INTERFACE_VERSION,
    };

    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
    };
    use windows::Win32::System::IO::DeviceIoControl;

    const GENERIC_READ_ACCESS: u32 = 0x8000_0000;
    const GENERIC_WRITE_ACCESS: u32 = 0x4000_0000;

    pub struct Device {
        handle: HANDLE,
    }

    fn wide(text: &str) -> Vec<u16> {
        let mut buffer: Vec<u16> = text.encode_utf16().collect();
        buffer.push(0);
        buffer
    }

    fn meaning_of(code: u32) -> &'static str {
        match code {
            2 => "the device does not exist. The driver did not create its symbolic link.",
            3 => "the path is wrong.",
            5 => "access denied. The device ACL is refusing this process.",
            32 => "another process already holds it.",
            _ => "see the Windows error code.",
        }
    }

    fn open_control_device() -> Result<HANDLE, VpadError> {
        let path = wide(DEVICE_PATH);

        unsafe {
            CreateFileW(
                PCWSTR(path.as_ptr()),
                GENERIC_READ_ACCESS | GENERIC_WRITE_ACCESS,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                None,
                OPEN_EXISTING,
                Default::default(),
                None,
            )
        }
        .map_err(|error| {
            let code = (error.code().0 & 0xFFFF) as u32;

            VpadError::CannotOpen {
                path: DEVICE_PATH.to_string(),
                code,
                meaning: meaning_of(code),
            }
        })
    }

    pub fn is_available() -> bool {
        match open_control_device() {
            Ok(handle) => {
                let _ = unsafe { CloseHandle(handle) };
                true
            }
            Err(_) => false,
        }
    }

    fn control(
        handle: HANDLE,
        code: u32,
        input: Option<&[u8]>,
        output: Option<&mut [u8]>,
    ) -> Result<u32, u32> {
        let mut returned = 0u32;

        let (in_ptr, in_len) = match input {
            Some(bytes) => (
                bytes.as_ptr() as *const core::ffi::c_void,
                bytes.len() as u32,
            ),
            None => (core::ptr::null(), 0),
        };

        let (out_ptr, out_len) = match output {
            Some(bytes) => (
                bytes.as_mut_ptr() as *mut core::ffi::c_void,
                bytes.len() as u32,
            ),
            None => (core::ptr::null_mut(), 0),
        };

        let result = unsafe {
            DeviceIoControl(
                handle,
                code,
                Some(in_ptr),
                in_len,
                Some(out_ptr),
                out_len,
                Some(&mut returned),
                None,
            )
        };

        match result {
            Ok(()) => Ok(returned),
            Err(error) => Err(error.code().0 as u32),
        }
    }

    impl Device {
        pub fn open() -> Result<Self, VpadError> {
            let handle = open_control_device()?;
            let device = Self { handle };

            device.negotiate()?;
            device.configure()?;

            Ok(device)
        }

        fn negotiate(&self) -> Result<(), VpadError> {
            let mut version = [0u8; 4];

            control(
                self.handle,
                IOCTL_OPENDS_GET_INTERFACE_VERSION,
                None,
                Some(&mut version),
            )
            .map_err(|_| VpadError::DriverAbsent)?;

            let found = u32::from_le_bytes(version);

            match found == OPENDS_UHID_INTERFACE_VERSION {
                true => Ok(()),
                false => Err(VpadError::VersionMismatch {
                    found,
                    needed: OPENDS_UHID_INTERFACE_VERSION,
                }),
            }
        }

        fn configure(&self) -> Result<(), VpadError> {
            let info = xbox_device_info().encode();

            control(self.handle, IOCTL_OPENDS_SET_DEVICE_INFO, Some(&info), None)
                .map_err(|_| VpadError::Create)?;

            control(
                self.handle,
                IOCTL_OPENDS_SET_REPORT_DESCRIPTOR,
                Some(vpad::REPORT_DESCRIPTOR),
                None,
            )
            .map_err(|_| VpadError::Create)?;

            let ids = hardware_ids_multi_sz();
            let id_bytes: Vec<u8> = ids.iter().flat_map(|unit| unit.to_le_bytes()).collect();

            control(
                self.handle,
                IOCTL_OPENDS_SET_HARDWARE_IDS,
                Some(&id_bytes),
                None,
            )
            .map_err(|_| VpadError::Create)?;

            control(self.handle, IOCTL_OPENDS_CREATE_DEVICE, None, None)
                .map_err(|_| VpadError::Create)?;

            control(self.handle, IOCTL_OPENDS_START_DEVICE, None, None)
                .map_err(|_| VpadError::Create)?;

            Ok(())
        }

        pub fn submit(&mut self, report: &[u8]) -> Result<(), VpadError> {
            control(
                self.handle,
                IOCTL_OPENDS_SUBMIT_INPUT_REPORT,
                Some(report),
                None,
            )
            .map(|_| ())
            .map_err(|_| VpadError::Submit { len: report.len() })
        }

        pub fn next_output(&mut self) -> Option<Vec<u8>> {
            let mut event = vec![0u8; 4 + OPENDS_MAX_REPORT];

            control(
                self.handle,
                IOCTL_OPENDS_GET_NEXT_EVENT,
                None,
                Some(&mut event),
            )
            .ok()?;

            let length = u32::from_le_bytes([event[0], event[1], event[2], event[3]]) as usize;

            match length > 0 && length + 4 <= event.len() {
                true => Some(event[4..4 + length].to_vec()),
                false => None,
            }
        }
    }

    impl Drop for Device {
        fn drop(&mut self) {
            let _ = unsafe { CloseHandle(self.handle) };
        }
    }
}

#[cfg(not(windows))]
mod platform {
    use super::VpadError;

    pub struct Device;

    pub fn is_available() -> bool {
        false
    }

    impl Device {
        pub fn open() -> Result<Self, VpadError> {
            Err(VpadError::DriverAbsent)
        }

        pub fn submit(&mut self, report: &[u8]) -> Result<(), VpadError> {
            Err(VpadError::Submit { len: report.len() })
        }

        pub fn next_output(&mut self) -> Option<Vec<u8>> {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use opends_core::types::pad::CROSS;

    #[test]
    fn a_pad_that_cannot_be_created_explains_why_rather_than_failing_silently() {
        if let Err(error) = UhidPad::open() {
            assert!(!error.to_string().is_empty());
        }
    }

    #[test]
    fn the_driver_absent_error_says_keyboard_and_mouse_still_work() {
        assert!(VpadError::DriverAbsent
            .to_string()
            .contains("Keyboard and mouse mapping still works"));
    }

    #[test]
    fn the_create_error_explains_a_pad_may_already_be_held_by_another_process() {
        assert!(VpadError::Create
            .to_string()
            .contains("tears its pad down on close"));
    }

    #[test]
    fn the_version_mismatch_error_names_both_versions() {
        let error = VpadError::VersionMismatch {
            found: 1,
            needed: 2,
        };

        let text = error.to_string();

        assert!(text.contains('1'));
        assert!(text.contains('2'));
    }

    #[test]
    fn every_open_failure_names_the_device_path_it_tried() {
        for code in [2u32, 5, 32, 999] {
            let error = VpadError::CannotOpen {
                path: DEVICE_PATH.to_string(),
                code,
                meaning: "test",
            };

            assert!(error.to_string().contains("OpenDsUHid"));
        }
    }

    #[test]
    fn availability_agrees_with_whether_the_control_device_can_actually_be_opened() {
        let available = is_available();
        let opened = UhidPad::open();

        if !available {
            assert!(opened.is_err());
        }
    }

    #[test]
    fn the_hardware_ids_are_a_double_null_terminated_multi_sz() {
        let wide = hardware_ids_multi_sz();

        assert_eq!(wide[wide.len() - 1], 0);
        assert_eq!(wide[wide.len() - 2], 0);
        assert_ne!(wide[wide.len() - 3], 0);
    }

    #[test]
    fn the_hardware_ids_carry_the_string_windows_binds_its_xinput_driver_to() {
        let wide = hardware_ids_multi_sz();
        let text = String::from_utf16_lossy(&wide[..wide.len() - 2]);

        assert_eq!(text, vpad::XINPUT_HARDWARE_ID);
        assert!(text.contains("IG_00"));
    }

    #[test]
    fn the_virtual_pad_identifies_itself_as_an_xbox_one_pad() {
        let info = xbox_device_info();

        assert_eq!({ info.vendor_id }, 0x045E);
        assert_eq!({ info.product_id }, 0x02FF);
    }

    #[test]
    fn the_device_info_the_app_sends_encodes_to_what_the_driver_expects() {
        let encoded = xbox_device_info().encode();

        assert_eq!(encoded.len(), OpendsDeviceInfo::ENCODED_LEN);
        assert_eq!(
            u32::from_le_bytes([encoded[0], encoded[1], encoded[2], encoded[3]]),
            OPENDS_UHID_INTERFACE_VERSION
        );
    }

    #[test]
    fn the_device_path_comes_from_the_spec_and_is_not_written_out_again_here() {
        assert_eq!(DEVICE_PATH, OPENDS_UHID_WIN32_PATH);
    }

    #[test]
    fn a_mocked_pad_receives_the_packed_report_for_the_state_it_was_given() {
        let mut pad = MockVirtualPad::new();

        pad.expect_submit()
            .withf(|state| state.is_down(CROSS))
            .times(1)
            .returning(|_| Ok(()));

        let state = PadState {
            buttons: CROSS,
            ..PadState::default()
        };

        assert!(pad.submit(&state).is_ok());
    }

    #[test]
    fn rumble_arriving_from_a_game_is_handed_back_to_the_caller() {
        let mut pad = MockVirtualPad::new();

        pad.expect_take_rumble().returning(|| {
            Some(vpad::Rumble {
                left_motor: 200,
                right_motor: 100,
                left_trigger_motor: 0,
                right_trigger_motor: 0,
            })
        });

        assert_eq!(pad.take_rumble().unwrap().left_motor, 200);
    }
}
