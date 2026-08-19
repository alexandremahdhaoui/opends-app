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

use opends_core::controller::output::TriggerEffect;
use opends_core::types::pad::{Battery, DeviceKind, PadState, Transport};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TriggerPreset {
    #[default]
    Off,
    Rigid,
    Pulse,
    Weapon,
}

impl TriggerPreset {
    pub const ALL: &'static [TriggerPreset] = &[
        TriggerPreset::Off,
        TriggerPreset::Rigid,
        TriggerPreset::Pulse,
        TriggerPreset::Weapon,
    ];

    pub fn label(self) -> &'static str {
        match self {
            TriggerPreset::Off => "Off",
            TriggerPreset::Rigid => "Rigid",
            TriggerPreset::Pulse => "Pulse",
            TriggerPreset::Weapon => "Weapon",
        }
    }

    pub fn to_effect(self) -> TriggerEffect {
        match self {
            TriggerPreset::Off => TriggerEffect::Off,
            TriggerPreset::Rigid => TriggerEffect::Rigid { force: 200 },
            TriggerPreset::Pulse => TriggerEffect::Pulse {
                start: 30,
                force: 200,
            },
            TriggerPreset::Weapon => TriggerEffect::Weapon {
                start: 40,
                end: 200,
                force: 255,
            },
        }
    }

    pub fn from_effect(effect: TriggerEffect) -> Self {
        match effect {
            TriggerEffect::Off => TriggerPreset::Off,
            TriggerEffect::Rigid { .. } => TriggerPreset::Rigid,
            TriggerEffect::Pulse { .. } => TriggerPreset::Pulse,
            TriggerEffect::Weapon { .. } => TriggerPreset::Weapon,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct GuiStatus {
    pub pad_name: Option<&'static str>,
    pub transport: Option<Transport>,
    pub battery: Option<Battery>,
    pub vpad_active: bool,
    pub state: Option<PadState>,
}

impl GuiStatus {
    pub fn from_pad(
        kind: DeviceKind,
        transport: Option<Transport>,
        battery: Option<Battery>,
    ) -> Self {
        Self {
            pad_name: Some(kind.display_name()),
            transport,
            battery,
            vpad_active: false,
            state: None,
        }
    }

    pub fn with_state(mut self, state: PadState) -> Self {
        self.state = Some(state);
        self
    }
}

pub fn live_view(status: &GuiStatus) -> String {
    let Some(state) = &status.state else {
        return "No pad connected. Plug one in to see live button, stick, and sensor \
                readings here."
            .to_string();
    };

    let buttons = state.held_names();
    let buttons = match buttons.is_empty() {
        true => "none".to_string(),
        false => buttons.join(", "),
    };

    let (lx, ly) = state.left_stick.normalised();
    let (rx, ry) = state.right_stick.normalised();

    let finger = |touch: &opends_core::types::pad::Touch| match touch.active {
        true => format!("{},{}", touch.x, touch.y),
        false => "up".to_string(),
    };

    format!(
        "buttons held: {buttons}\n\
         left stick: {lx:.2}, {ly:.2}\n\
         right stick: {rx:.2}, {ry:.2}\n\
         left trigger: {}   right trigger: {}\n\
         touch 1: {}   touch 2: {}\n\
         gyro: {}, {}, {}\n\
         accel: {}, {}, {}",
        state.left_trigger,
        state.right_trigger,
        finger(&state.touch.first),
        finger(&state.touch.second),
        state.motion.gyro_pitch,
        state.motion.gyro_yaw,
        state.motion.gyro_roll,
        state.motion.accel_x,
        state.motion.accel_y,
        state.motion.accel_z,
    )
}

pub fn status_line(status: &GuiStatus) -> String {
    match status.pad_name {
        None => "No pad connected. Plug one in over USB or pair it over Bluetooth.".to_string(),
        Some(name) => {
            let transport = match status.transport {
                Some(transport) => format!("{transport:?}"),
                None => "connecting".to_string(),
            };

            let battery = match &status.battery {
                Some(battery) => format!(
                    ", {}%{}",
                    battery.percent,
                    if battery.charging { " charging" } else { "" }
                ),
                None => String::new(),
            };

            format!("{name} connected over {transport}{battery}")
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct MappingStatus {
    pub profile_name: Option<String>,
    pub binding_count: usize,
    pub error: Option<String>,
    pub enabled: bool,
}

pub fn mapping_summary(status: &MappingStatus) -> String {
    if let Some(error) = &status.error {
        return format!("could not load profile: {error}");
    }

    match &status.profile_name {
        None => "no profile loaded. Buttons only work as a pad, not as keys.".to_string(),
        Some(name) => {
            let state = match status.enabled {
                true => "mapping is ON",
                false => "profile loaded, mapping is OFF",
            };

            format!("{name}, {} binding(s), {state}", status.binding_count)
        }
    }
}

pub fn parse_key_code(input: &str) -> Option<u16> {
    let trimmed = input.trim();

    if trimmed.is_empty() {
        return None;
    }

    match trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        Some(hex) => u16::from_str_radix(hex, 16).ok(),
        None => trimmed.parse().ok(),
    }
}

pub fn tray_tooltip(status: &GuiStatus) -> String {
    let pad = match status.pad_name {
        None => "no pad".to_string(),
        Some(name) => match &status.battery {
            Some(battery) => format!("{name} {}%", battery.percent),
            None => name.to_string(),
        },
    };

    let vpad = match status.vpad_active {
        true => "virtual pad on",
        false => "virtual pad off",
    };

    format!("OpenDS: {pad}, {vpad}")
}

#[cfg(windows)]
pub use platform::run;

#[cfg(windows)]
mod platform {
    use std::sync::mpsc::TryRecvError;
    use std::sync::{Arc, Mutex};
    use std::thread::sleep;
    use std::time::Duration;

    use egui::Color32;

    use opends_core::controller::mapping;
    use opends_core::controller::mapping::{Binding, MouseButton, Profile, TimedStep};
    use opends_core::controller::output::{Colour, PadOutput, Rumble};

    use crate::adapter::driver_install_adapter::DriverInstaller;
    use crate::adapter::driver_install_win::WindowsDriverInstaller;
    use crate::adapter::foreground_process_adapter::{ForegroundProcess, Win32ForegroundProcess};
    use crate::adapter::hid_adapter::{self, SetupApiEnumerator};
    use crate::adapter::kbm_adapter::SendInputKbm;
    use crate::adapter::profile_adapter::{self, FileProfiles, Profiles};
    use crate::adapter::tray_adapter::{self, TrayEvent};
    use crate::adapter::vpad_adapter::{UhidPad, VirtualPad};
    use crate::controller::auto_profile_controller::AutoProfileSwitcher;
    use crate::controller::map_controller::MapController;
    use crate::controller::pad_controller::PadController;
    use crate::controller::vpad_recovery_controller::create_pad_with_recovery;
    use crate::driver::theme;

    use super::{
        live_view, mapping_summary, status_line, tray_tooltip, GuiStatus, MappingStatus,
        TriggerPreset,
    };

    const WINDOW: [f32; 2] = [440.0, 680.0];

    #[derive(Default)]
    struct MappingSlot {
        profile: Option<Profile>,
        enabled: bool,
        version: u64,
    }

    #[derive(Default, Clone)]
    struct AutoProfileConfig {
        rules: Vec<(String, String)>,
        default_profile: Option<String>,
        version: u64,
    }

    fn open_virtual_pad() -> Option<UhidPad> {
        let installer = WindowsDriverInstaller::new();

        create_pad_with_recovery(UhidPad::open, || installer.remove_stray_pads())
            .inspect_err(|error| eprintln!("virtual pad not available: {error}"))
            .ok()
    }

    fn poll_loop(
        shared: Arc<Mutex<GuiStatus>>,
        mapping: Arc<Mutex<MappingSlot>>,
        auto_profile: Arc<Mutex<AutoProfileConfig>>,
    ) {
        let enumerator = SetupApiEnumerator::new();
        let mut controller = PadController::new();
        let mut attached_paths: Vec<String> = Vec::new();
        let mut virtual_pad = open_virtual_pad();
        let mut kbm = SendInputKbm::new();
        let mut mapper: Option<MapController> = None;
        let mut mapper_version = 0u64;
        let mut wanted = PadOutput {
            lightbar: Colour::new(0, 40, 120),
            ..PadOutput::default()
        };
        let mut output_dirty = true;
        let foreground_process = Win32ForegroundProcess::new();
        let mut auto_switcher = AutoProfileSwitcher::new(Vec::new(), None);
        let mut auto_profile_version = 0u64;

        loop {
            let (auto_rules, auto_default, auto_version) = auto_profile
                .lock()
                .map(|config| {
                    (
                        config.rules.clone(),
                        config.default_profile.clone(),
                        config.version,
                    )
                })
                .unwrap_or_default();

            if auto_version != auto_profile_version {
                auto_profile_version = auto_version;
                auto_switcher = AutoProfileSwitcher::new(auto_rules, auto_default);
            }

            if let Some(path) =
                auto_switcher.profile_to_load(foreground_process.current_process_name().as_deref())
            {
                if let Ok(profile) = FileProfiles::new().load(&path) {
                    if let Ok(mut guard) = mapping.lock() {
                        guard.profile = Some(profile);
                        guard.version += 1;
                    }
                }
            }

            for info in hid_adapter::sony_gamepads(&enumerator) {
                if attached_paths.contains(&info.path) {
                    continue;
                }

                let Some(kind) = info.kind() else { continue };

                if let Ok(device) = hid_adapter::open(&info, false) {
                    attached_paths.push(info.path.clone());
                    controller.attach(Box::new(device) as Box<_>, kind);
                    output_dirty = true;
                }
            }

            let (mapping_enabled, mapping_version, mapping_profile) = mapping
                .lock()
                .map(|guard| (guard.enabled, guard.version, guard.profile.clone()))
                .unwrap_or((false, 0, None));

            if mapping_version != mapper_version {
                mapper_version = mapping_version;
                mapper = mapping_profile.map(MapController::new);
                wanted.left_trigger = mapper
                    .as_ref()
                    .map(|mapper| mapper.profile().left_trigger)
                    .unwrap_or_default();
                wanted.right_trigger = mapper
                    .as_ref()
                    .map(|mapper| mapper.profile().right_trigger)
                    .unwrap_or_default();
                output_dirty = true;
            }

            if output_dirty && controller.send_output_to_all(&wanted) > 0 {
                output_dirty = false;
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

            let mut latest = GuiStatus::default();

            for update in controller.poll() {
                latest =
                    GuiStatus::from_pad(update.kind, Some(update.transport), update.state.battery)
                        .with_state(update.state);

                if mapping_enabled {
                    if let Some(mapper) = mapper.as_mut() {
                        mapper.apply(&update, &mut kbm);
                    }
                }

                if let Some(pad) = virtual_pad.as_mut() {
                    let shaped = match mapper.as_ref() {
                        Some(mapper) => mapping::shape_sticks(mapper.profile(), &update.state),
                        None => update.state,
                    };

                    let _ = pad.submit(&shaped);
                }
            }

            if latest.pad_name.is_none() {
                if let Some(session) = controller.sessions().first() {
                    latest = GuiStatus::from_pad(
                        session.kind(),
                        session.transport(),
                        session.state().battery,
                    )
                    .with_state(*session.state());
                }
            }

            latest.vpad_active = virtual_pad.is_some();

            if let Ok(mut guard) = shared.lock() {
                *guard = latest;
            }

            sleep(Duration::from_millis(16));
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    enum Tab {
        #[default]
        Status,
        Test,
        Bindings,
        AutoProfile,
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum BindingKind {
        Unbound,
        Key,
        Mouse,
    }

    impl BindingKind {
        fn label(self) -> &'static str {
            match self {
                BindingKind::Unbound => "Unbound",
                BindingKind::Key => "Key",
                BindingKind::Mouse => "Mouse",
            }
        }
    }

    const MOUSE_BUTTONS: &[MouseButton] = &[
        MouseButton::Left,
        MouseButton::Right,
        MouseButton::Middle,
        MouseButton::Fourth,
        MouseButton::Fifth,
    ];

    fn mouse_button_label(button: MouseButton) -> &'static str {
        match button {
            MouseButton::Left => "Left",
            MouseButton::Right => "Right",
            MouseButton::Middle => "Middle",
            MouseButton::Fourth => "Fourth",
            MouseButton::Fifth => "Fifth",
        }
    }

    struct BindingRow {
        button_mask: u32,
        button_name: &'static str,
        kind: BindingKind,
        key_code_input: String,
        mouse_button: MouseButton,
        combo_enabled: bool,
        combo_kind: BindingKind,
        combo_key_code_input: String,
        combo_mouse_button: MouseButton,
        combo_delay_ms_input: String,
        turbo: bool,
    }

    impl BindingRow {
        fn new(button_mask: u32, button_name: &'static str) -> Self {
            Self {
                button_mask,
                button_name,
                kind: BindingKind::Unbound,
                key_code_input: String::new(),
                mouse_button: MouseButton::Left,
                combo_enabled: false,
                combo_kind: BindingKind::Key,
                combo_key_code_input: String::new(),
                combo_mouse_button: MouseButton::Left,
                combo_delay_ms_input: "0".to_string(),
                turbo: false,
            }
        }

        fn from_binding(button_mask: u32, button_name: &'static str, binding: &Binding) -> Self {
            let mut row = Self::new(button_mask, button_name);

            match binding {
                Binding::Key { code } => {
                    row.kind = BindingKind::Key;
                    row.key_code_input = format!("0x{code:X}");
                }
                Binding::Mouse { button } => {
                    row.kind = BindingKind::Mouse;
                    row.mouse_button = *button;
                }
                Binding::Macro { steps } => {
                    if let [first, second] = steps.as_slice() {
                        row.apply_step(first, false);
                        row.combo_enabled = true;
                        row.apply_step(second, true);
                        row.combo_delay_ms_input = "0".to_string();
                    }
                }
                Binding::TimedMacro { steps } => {
                    if let [first, second] = steps.as_slice() {
                        row.apply_step(&first.binding, false);
                        row.combo_enabled = true;
                        row.apply_step(&second.binding, true);
                        row.combo_delay_ms_input = second.delay_ms.to_string();
                    }
                }
                Binding::Unbound => {}
            }

            row
        }

        fn apply_step(&mut self, step: &Binding, combo: bool) {
            let (kind, key_input, mouse) = match step {
                Binding::Key { code } => (BindingKind::Key, format!("0x{code:X}"), None),
                Binding::Mouse { button } => (BindingKind::Mouse, String::new(), Some(*button)),
                _ => return,
            };

            if combo {
                self.combo_kind = kind;
                self.combo_key_code_input = key_input;
                if let Some(button) = mouse {
                    self.combo_mouse_button = button;
                }
            } else {
                self.kind = kind;
                self.key_code_input = key_input;
                if let Some(button) = mouse {
                    self.mouse_button = button;
                }
            }
        }

        fn single_binding(kind: BindingKind, key_input: &str, mouse: MouseButton) -> Binding {
            match kind {
                BindingKind::Unbound => Binding::Unbound,
                BindingKind::Key => Binding::Key {
                    code: super::parse_key_code(key_input).unwrap_or(0),
                },
                BindingKind::Mouse => Binding::Mouse { button: mouse },
            }
        }

        fn to_binding(&self) -> Binding {
            let primary = Self::single_binding(self.kind, &self.key_code_input, self.mouse_button);

            if !self.combo_enabled || self.kind == BindingKind::Unbound {
                return primary;
            }

            let secondary = Self::single_binding(
                self.combo_kind,
                &self.combo_key_code_input,
                self.combo_mouse_button,
            );

            let delay_ms = super::parse_key_code(&self.combo_delay_ms_input)
                .map(u32::from)
                .unwrap_or(0);

            if delay_ms == 0 {
                return Binding::Macro {
                    steps: vec![primary, secondary],
                };
            }

            Binding::TimedMacro {
                steps: vec![
                    TimedStep {
                        binding: primary,
                        delay_ms: 0,
                    },
                    TimedStep {
                        binding: secondary,
                        delay_ms,
                    },
                ],
            }
        }
    }

    struct App {
        shared: Arc<Mutex<GuiStatus>>,
        mapping: Arc<Mutex<MappingSlot>>,
        mapping_status: MappingStatus,
        current_profile: Option<Profile>,
        profile_path_input: String,
        binding_rows: Vec<BindingRow>,
        turbo_interval_ms_input: String,
        tab: Tab,
        auto_profile: Arc<Mutex<AutoProfileConfig>>,
        auto_profile_rows: Vec<(String, String)>,
        auto_profile_default_input: String,
        tray: tray_adapter::TrayHandle,
        tray_events: std::sync::mpsc::Receiver<TrayEvent>,
    }

    impl App {
        fn load_profile(&mut self, profile: Result<Profile, String>) {
            match profile {
                Ok(profile) => {
                    self.mapping_status = MappingStatus {
                        profile_name: Some(profile.name.clone()),
                        binding_count: profile.bindings.len(),
                        error: None,
                        enabled: self.mapping_status.enabled,
                    };

                    self.binding_rows = opends_core::types::pad::ALL_BUTTONS
                        .iter()
                        .map(|(mask, name)| {
                            let mut row = match profile.bindings.get(*name) {
                                Some(binding) => BindingRow::from_binding(*mask, name, binding),
                                None => BindingRow::new(*mask, name),
                            };
                            row.turbo = profile.turbo_buttons.contains(*name);
                            row
                        })
                        .collect();

                    self.turbo_interval_ms_input = profile.turbo_interval_ms.to_string();

                    self.current_profile = Some(profile);
                    self.push_profile_update();
                }
                Err(error) => {
                    self.mapping_status.error = Some(error);
                }
            }
        }

        fn sync_bindings_into_profile(&mut self) {
            if let Some(profile) = self.current_profile.as_mut() {
                profile.bindings.clear();
                profile.turbo_buttons.clear();

                for row in &self.binding_rows {
                    let binding = row.to_binding();

                    if binding != Binding::Unbound {
                        profile
                            .bindings
                            .insert(row.button_name.to_string(), binding);
                    }

                    if row.turbo && row.kind != BindingKind::Unbound {
                        profile.turbo_buttons.insert(row.button_name.to_string());
                    }
                }

                self.mapping_status.binding_count = profile.bindings.len();
            }

            self.push_profile_update();
        }

        fn set_turbo_interval_ms(&mut self, interval_ms: u32) {
            if let Some(profile) = self.current_profile.as_mut() {
                profile.turbo_interval_ms = interval_ms.max(2);
            }

            self.push_profile_update();
        }

        fn set_shift_button(&mut self, button: Option<u32>) {
            if let Some(profile) = self.current_profile.as_mut() {
                profile.shift_button = button;
            }

            self.push_profile_update();
        }

        fn push_profile_update(&mut self) {
            if let Ok(mut guard) = self.mapping.lock() {
                guard.profile = self.current_profile.clone();
                guard.version += 1;
            }
        }

        fn set_gyro_sensitivity(&mut self, sensitivity: Option<f32>) {
            if let Some(profile) = self.current_profile.as_mut() {
                profile.gyro_mouse_sensitivity = sensitivity;
            }

            self.push_profile_update();
        }

        fn set_gyro_toggle_button(&mut self, button: Option<u32>) {
            if let Some(profile) = self.current_profile.as_mut() {
                profile.gyro_toggle_button = button;
            }

            self.push_profile_update();
        }

        fn push_auto_profile_update(&mut self) {
            let rules: Vec<(String, String)> = self
                .auto_profile_rows
                .iter()
                .filter(|(process, path)| !process.trim().is_empty() && !path.trim().is_empty())
                .cloned()
                .collect();

            let default_profile = match self.auto_profile_default_input.trim().is_empty() {
                true => None,
                false => Some(self.auto_profile_default_input.trim().to_string()),
            };

            let _ = crate::adapter::auto_profile_adapter::save(
                &crate::adapter::auto_profile_adapter::config_path(),
                &crate::adapter::auto_profile_adapter::AutoProfileFile {
                    rules: rules.clone(),
                    default_profile: default_profile.clone(),
                },
            );

            if let Ok(mut guard) = self.auto_profile.lock() {
                guard.rules = rules;
                guard.default_profile = default_profile;
                guard.version += 1;
            }
        }

        fn set_touch_sensitivity(&mut self, sensitivity: Option<f32>) {
            if let Some(profile) = self.current_profile.as_mut() {
                profile.touch_mouse_sensitivity = sensitivity;
            }

            self.push_profile_update();
        }

        fn set_stick_dead_zone(&mut self, dead_zone: Option<f32>) {
            if let Some(profile) = self.current_profile.as_mut() {
                profile.stick_dead_zone = dead_zone;
            }

            self.push_profile_update();
        }

        fn set_trigger_preset(&mut self, preset: TriggerPreset) {
            if let Some(profile) = self.current_profile.as_mut() {
                profile.left_trigger = preset.to_effect();
                profile.right_trigger = preset.to_effect();
            }

            self.push_profile_update();
        }

        fn set_enabled(&mut self, enabled: bool) {
            self.mapping_status.enabled = enabled;

            if let Ok(mut guard) = self.mapping.lock() {
                guard.enabled = enabled;
            }
        }

        fn status_tab(&mut self, ui: &mut egui::Ui, status: &GuiStatus) {
            ui.label(theme::body(&status_line(status)));
            ui.add_space(6.0);
            ui.label(theme::muted(match status.vpad_active {
                true => "Virtual Xbox pad is active. XInput games can see it.",
                false => "No virtual pad. Install the driver with OpenDS-Setup.exe.",
            }));
            ui.add_space(16.0);
            ui.separator();
            ui.add_space(10.0);

            ui.label(theme::body("Mapping"));
            ui.add_space(4.0);
            ui.label(theme::muted(
                "A profile is a JSON file that says which key or mouse button each pad \
                 button presses. Default gives you a working one with no file needed: \
                 Cross is Space, Circle is Escape, the D-pad is the arrow keys.",
            ));
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.profile_path_input)
                        .hint_text("only needed for a custom profile.json")
                        .desired_width(220.0),
                );

                if ui.button("Load").clicked() {
                    let path = self.profile_path_input.clone();
                    let result = FileProfiles::new()
                        .load(&path)
                        .map_err(|error| error.to_string());

                    self.load_profile(result);
                }

                if ui.button("Default").clicked() {
                    self.load_profile(Ok(profile_adapter::default_profile()));
                }
            });

            ui.add_space(6.0);

            ui.add_enabled_ui(self.mapping_status.profile_name.is_some(), |ui| {
                let mut enabled = self.mapping_status.enabled;

                if ui
                    .checkbox(&mut enabled, "Map buttons to keys and mouse")
                    .changed()
                {
                    self.set_enabled(enabled);
                }
            });

            if self.current_profile.is_some() {
                ui.add_space(8.0);

                let mut gyro_enabled = self
                    .current_profile
                    .as_ref()
                    .and_then(|profile| profile.gyro_mouse_sensitivity)
                    .is_some();
                let mut gyro_value = self
                    .current_profile
                    .as_ref()
                    .and_then(|profile| profile.gyro_mouse_sensitivity)
                    .unwrap_or(0.15);
                let mut gyro_changed = false;

                ui.horizontal(|ui| {
                    if ui.checkbox(&mut gyro_enabled, "Gyro to mouse").changed() {
                        gyro_changed = true;
                    }

                    ui.add_enabled_ui(gyro_enabled, |ui| {
                        if ui
                            .add(egui::Slider::new(&mut gyro_value, 0.02..=0.5).text("sensitivity"))
                            .changed()
                        {
                            gyro_changed = true;
                        }
                    });
                });

                if gyro_changed {
                    self.set_gyro_sensitivity(match gyro_enabled {
                        true => Some(gyro_value),
                        false => None,
                    });
                }

                if gyro_enabled {
                    let current_toggle = self
                        .current_profile
                        .as_ref()
                        .and_then(|profile| profile.gyro_toggle_button);
                    let mut selected_toggle = current_toggle;

                    ui.horizontal(|ui| {
                        ui.label("  Only while holding");

                        egui::ComboBox::from_id_salt("gyro_toggle_button")
                            .selected_text(
                                current_toggle
                                    .and_then(opends_core::types::pad::button_name)
                                    .unwrap_or("Always on"),
                            )
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut selected_toggle, None, "Always on");

                                for (mask, name) in opends_core::types::pad::ALL_BUTTONS {
                                    ui.selectable_value(&mut selected_toggle, Some(*mask), *name);
                                }
                            });
                    });

                    if selected_toggle != current_toggle {
                        self.set_gyro_toggle_button(selected_toggle);
                    }
                }

                ui.add_space(8.0);

                let mut touch_enabled = self
                    .current_profile
                    .as_ref()
                    .and_then(|profile| profile.touch_mouse_sensitivity)
                    .is_some();
                let mut touch_value = self
                    .current_profile
                    .as_ref()
                    .and_then(|profile| profile.touch_mouse_sensitivity)
                    .unwrap_or(1.0);
                let mut touch_changed = false;

                ui.horizontal(|ui| {
                    if ui
                        .checkbox(&mut touch_enabled, "Touchpad to mouse")
                        .changed()
                    {
                        touch_changed = true;
                    }

                    ui.add_enabled_ui(touch_enabled, |ui| {
                        if ui
                            .add(egui::Slider::new(&mut touch_value, 0.2..=3.0).text("sensitivity"))
                            .changed()
                        {
                            touch_changed = true;
                        }
                    });
                });

                if touch_changed {
                    self.set_touch_sensitivity(match touch_enabled {
                        true => Some(touch_value),
                        false => None,
                    });
                }

                ui.add_space(8.0);

                let mut dead_zone_enabled = self
                    .current_profile
                    .as_ref()
                    .and_then(|profile| profile.stick_dead_zone)
                    .is_some();
                let mut dead_zone_value = self
                    .current_profile
                    .as_ref()
                    .and_then(|profile| profile.stick_dead_zone)
                    .unwrap_or(0.1);
                let mut dead_zone_changed = false;

                ui.horizontal(|ui| {
                    if ui
                        .checkbox(&mut dead_zone_enabled, "Stick dead zone")
                        .changed()
                    {
                        dead_zone_changed = true;
                    }

                    ui.add_enabled_ui(dead_zone_enabled, |ui| {
                        if ui
                            .add(egui::Slider::new(&mut dead_zone_value, 0.02..=0.4).text("size"))
                            .changed()
                        {
                            dead_zone_changed = true;
                        }
                    });
                });

                if dead_zone_changed {
                    self.set_stick_dead_zone(match dead_zone_enabled {
                        true => Some(dead_zone_value),
                        false => None,
                    });
                }

                ui.add_space(8.0);

                let mut selected = self
                    .current_profile
                    .as_ref()
                    .map(|profile| TriggerPreset::from_effect(profile.left_trigger))
                    .unwrap_or_default();

                ui.horizontal(|ui| {
                    ui.label("Adaptive triggers (both sides)");

                    egui::ComboBox::from_id_salt("trigger_preset")
                        .selected_text(selected.label())
                        .show_ui(ui, |ui| {
                            for preset in TriggerPreset::ALL {
                                ui.selectable_value(&mut selected, *preset, preset.label());
                            }
                        });
                });

                if selected
                    != self
                        .current_profile
                        .as_ref()
                        .map(|profile| TriggerPreset::from_effect(profile.left_trigger))
                        .unwrap_or_default()
                {
                    self.set_trigger_preset(selected);
                }
            }

            ui.add_space(6.0);
            ui.label(theme::muted(&mapping_summary(&self.mapping_status)));

            ui.add_space(16.0);
            ui.separator();
            ui.add_space(10.0);
            ui.label(theme::muted(
                "Closing this window keeps OpenDS running in the tray.",
            ));
            ui.add_space(8.0);
        }

        fn bindings_tab(&mut self, ui: &mut egui::Ui) {
            if self.current_profile.is_none() {
                ui.label(theme::muted(
                    "Load or create a profile on the Status tab first.",
                ));
                return;
            }

            ui.label(theme::muted(
                "What each button does. Check \"Also\" to fire a second action on the same \
                 press, a simple two-step macro. Leave the delay at 0 to fire both at once, \
                 or set it to fire the second action a moment after the first. Check \"Turbo\" \
                 to make a button auto-repeat while held, at the rate below.",
            ));

            ui.horizontal(|ui| {
                ui.label("Turbo rate:");

                if ui
                    .add(
                        egui::TextEdit::singleline(&mut self.turbo_interval_ms_input)
                            .desired_width(50.0),
                    )
                    .changed()
                {
                    if let Ok(interval_ms) = self.turbo_interval_ms_input.parse::<u32>() {
                        self.set_turbo_interval_ms(interval_ms);
                    }
                }

                ui.label("ms per repeat");
            });

            ui.horizontal(|ui| {
                let current_shift = self
                    .current_profile
                    .as_ref()
                    .and_then(|profile| profile.shift_button);
                let mut selected_shift = current_shift;

                ui.label("Shift button:");

                egui::ComboBox::from_id_salt("shift_button")
                    .selected_text(
                        current_shift
                            .and_then(opends_core::types::pad::button_name)
                            .unwrap_or("None"),
                    )
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut selected_shift, None, "None");

                        for (mask, name) in opends_core::types::pad::ALL_BUTTONS {
                            ui.selectable_value(&mut selected_shift, Some(*mask), *name);
                        }
                    });

                if selected_shift != current_shift {
                    self.set_shift_button(selected_shift);
                }
            });

            if self
                .current_profile
                .as_ref()
                .and_then(|profile| profile.shift_button)
                .is_some()
            {
                ui.label(theme::muted(
                    "Holding the shift button gives every bound button a second, \
                     alternate action. Edit shift_bindings in the profile JSON by hand \
                     for now, there is no row editor for it yet.",
                ));
            }

            ui.add_space(10.0);

            let mut changed = false;

            for row in self.binding_rows.iter_mut() {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(row.button_name).strong().size(13.0));

                    egui::ComboBox::from_id_salt(("binding_kind", row.button_mask))
                        .selected_text(row.kind.label())
                        .show_ui(ui, |ui| {
                            for kind in [BindingKind::Unbound, BindingKind::Key, BindingKind::Mouse]
                            {
                                if ui
                                    .selectable_value(&mut row.kind, kind, kind.label())
                                    .changed()
                                {
                                    changed = true;
                                }
                            }
                        });

                    match row.kind {
                        BindingKind::Key => {
                            if ui
                                .add(
                                    egui::TextEdit::singleline(&mut row.key_code_input)
                                        .hint_text("0x20")
                                        .desired_width(60.0),
                                )
                                .changed()
                            {
                                changed = true;
                            }
                        }
                        BindingKind::Mouse => {
                            egui::ComboBox::from_id_salt(("binding_mouse", row.button_mask))
                                .selected_text(mouse_button_label(row.mouse_button))
                                .show_ui(ui, |ui| {
                                    for button in MOUSE_BUTTONS {
                                        if ui
                                            .selectable_value(
                                                &mut row.mouse_button,
                                                *button,
                                                mouse_button_label(*button),
                                            )
                                            .changed()
                                        {
                                            changed = true;
                                        }
                                    }
                                });
                        }
                        BindingKind::Unbound => {}
                    }

                    if row.kind != BindingKind::Unbound
                        && ui.checkbox(&mut row.combo_enabled, "Also").changed()
                    {
                        changed = true;
                    }
                });

                if row.kind != BindingKind::Unbound {
                    ui.horizontal(|ui| {
                        ui.add_space(24.0);

                        if ui.checkbox(&mut row.turbo, "Turbo").changed() {
                            changed = true;
                        }
                    });
                }

                if row.kind != BindingKind::Unbound && row.combo_enabled {
                    ui.horizontal(|ui| {
                        ui.add_space(24.0);

                        egui::ComboBox::from_id_salt(("combo_kind", row.button_mask))
                            .selected_text(row.combo_kind.label())
                            .show_ui(ui, |ui| {
                                for kind in [BindingKind::Key, BindingKind::Mouse] {
                                    if ui
                                        .selectable_value(&mut row.combo_kind, kind, kind.label())
                                        .changed()
                                    {
                                        changed = true;
                                    }
                                }
                            });

                        match row.combo_kind {
                            BindingKind::Key => {
                                if ui
                                    .add(
                                        egui::TextEdit::singleline(&mut row.combo_key_code_input)
                                            .hint_text("0x1B")
                                            .desired_width(60.0),
                                    )
                                    .changed()
                                {
                                    changed = true;
                                }
                            }
                            BindingKind::Mouse => {
                                egui::ComboBox::from_id_salt(("combo_mouse", row.button_mask))
                                    .selected_text(mouse_button_label(row.combo_mouse_button))
                                    .show_ui(ui, |ui| {
                                        for button in MOUSE_BUTTONS {
                                            if ui
                                                .selectable_value(
                                                    &mut row.combo_mouse_button,
                                                    *button,
                                                    mouse_button_label(*button),
                                                )
                                                .changed()
                                            {
                                                changed = true;
                                            }
                                        }
                                    });
                            }
                            BindingKind::Unbound => {}
                        }

                        ui.label("after");

                        if ui
                            .add(
                                egui::TextEdit::singleline(&mut row.combo_delay_ms_input)
                                    .hint_text("0")
                                    .desired_width(40.0),
                            )
                            .changed()
                        {
                            changed = true;
                        }

                        ui.label("ms");
                    });
                }

                ui.add_space(2.0);
            }

            if changed {
                self.sync_bindings_into_profile();
            }
        }

        fn auto_profile_tab(&mut self, ui: &mut egui::Ui) {
            ui.label(theme::muted(
                "Switch profiles automatically by which game or app has focus. \
                 Type the exe name exactly as it shows in Task Manager, for example \
                 forzahorizon6.exe, and the full path to the profile to load for it.",
            ));
            ui.add_space(10.0);

            let mut changed = false;
            let mut remove_at = None;

            for (index, (process, path)) in self.auto_profile_rows.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    if ui
                        .add(
                            egui::TextEdit::singleline(process)
                                .hint_text("game.exe")
                                .desired_width(140.0),
                        )
                        .changed()
                    {
                        changed = true;
                    }

                    if ui
                        .add(
                            egui::TextEdit::singleline(path)
                                .hint_text("path to that game's profile.json")
                                .desired_width(220.0),
                        )
                        .changed()
                    {
                        changed = true;
                    }

                    if ui.button("Remove").clicked() {
                        remove_at = Some(index);
                    }
                });
            }

            if let Some(index) = remove_at {
                self.auto_profile_rows.remove(index);
                changed = true;
            }

            ui.add_space(6.0);

            if ui.button("Add rule").clicked() {
                self.auto_profile_rows.push((String::new(), String::new()));
            }

            ui.add_space(12.0);
            ui.separator();
            ui.add_space(10.0);

            ui.horizontal(|ui| {
                ui.label("Default profile");

                if ui
                    .add(
                        egui::TextEdit::singleline(&mut self.auto_profile_default_input)
                            .hint_text("used when the foreground app matches no rule")
                            .desired_width(260.0),
                    )
                    .changed()
                {
                    changed = true;
                }
            });

            if changed {
                self.push_auto_profile_update();
            }

            ui.add_space(16.0);

            let active_rules = self
                .auto_profile_rows
                .iter()
                .filter(|(process, path)| !process.trim().is_empty() && !path.trim().is_empty())
                .count();

            ui.label(theme::muted(&format!(
                "{active_rules} active rule(s). Switching happens live while OpenDS is running, \
                 no restart needed."
            )));
        }
    }

    fn test_tab(ui: &mut egui::Ui, status: &GuiStatus) {
        ui.label(theme::muted(
            "Press buttons, move the sticks, drag the touchpad, tilt the pad. \
             This updates live so you can check it works before starting a game.",
        ));
        ui.add_space(10.0);

        egui::Frame::new()
            .fill(Color32::from(theme::ROW))
            .corner_radius(egui::CornerRadius::same(theme::RADIUS_ROW))
            .inner_margin(egui::Margin::symmetric(12, 10))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(
                    egui::RichText::new(live_view(status))
                        .monospace()
                        .size(13.0)
                        .color(Color32::from(theme::TEXT)),
                );
            });
    }

    impl eframe::App for App {
        fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
            ctx.request_repaint_after(Duration::from_millis(250));

            loop {
                match self.tray_events.try_recv() {
                    Ok(TrayEvent::Show) => {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true))
                    }
                    Ok(TrayEvent::Quit) => std::process::exit(0),
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => break,
                }
            }

            if ctx.input(|input| input.viewport().close_requested()) {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            }

            let status = self
                .shared
                .lock()
                .map(|guard| guard.clone())
                .unwrap_or_default();
            self.tray.set_tooltip(&tray_tooltip(&status));

            egui::CentralPanel::default()
                .frame(
                    egui::Frame::new()
                        .fill(Color32::from(theme::BACKGROUND))
                        .inner_margin(egui::Margin::symmetric(20, 20)),
                )
                .show(ctx, |ui| {
                    ui.label(theme::heading("OpenDS"));
                    ui.add_space(10.0);

                    ui.horizontal(|ui| {
                        if ui
                            .selectable_label(self.tab == Tab::Status, "Status")
                            .clicked()
                        {
                            self.tab = Tab::Status;
                        }

                        if ui
                            .selectable_label(self.tab == Tab::Test, "Test the pad")
                            .clicked()
                        {
                            self.tab = Tab::Test;
                        }

                        if ui
                            .selectable_label(self.tab == Tab::Bindings, "Bindings")
                            .clicked()
                        {
                            self.tab = Tab::Bindings;
                        }

                        if ui
                            .selectable_label(self.tab == Tab::AutoProfile, "Auto profile")
                            .clicked()
                        {
                            self.tab = Tab::AutoProfile;
                        }
                    });

                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(10.0);

                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| match self.tab {
                            Tab::Status => self.status_tab(ui, &status),
                            Tab::Test => test_tab(ui, &status),
                            Tab::Bindings => self.bindings_tab(ui),
                            Tab::AutoProfile => self.auto_profile_tab(ui),
                        });
                });
        }
    }

    pub fn run() {
        let shared = Arc::new(Mutex::new(GuiStatus::default()));
        let worker_shared = shared.clone();
        let mapping = Arc::new(Mutex::new(MappingSlot::default()));
        let worker_mapping = mapping.clone();
        let auto_profile = Arc::new(Mutex::new(AutoProfileConfig::default()));
        let worker_auto_profile = auto_profile.clone();

        std::thread::spawn(move || poll_loop(worker_shared, worker_mapping, worker_auto_profile));

        let (tray_events, tray) = tray_adapter::spawn("OpenDS: starting");

        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_inner_size(WINDOW)
                .with_min_inner_size(WINDOW)
                .with_title("OpenDS"),
            ..Default::default()
        };

        let _ = eframe::run_native(
            "OpenDS",
            options,
            Box::new(move |cc| {
                theme::apply(&cc.egui_ctx);

                let persisted = crate::adapter::auto_profile_adapter::load(
                    &crate::adapter::auto_profile_adapter::config_path(),
                );

                let mut app = App {
                    shared,
                    mapping,
                    mapping_status: MappingStatus::default(),
                    current_profile: None,
                    profile_path_input: String::new(),
                    binding_rows: Vec::new(),
                    turbo_interval_ms_input: "100".to_string(),
                    tab: Tab::default(),
                    auto_profile,
                    auto_profile_rows: persisted.rules,
                    auto_profile_default_input: persisted.default_profile.unwrap_or_default(),
                    tray,
                    tray_events,
                };

                app.load_profile(Ok(profile_adapter::default_profile()));
                app.push_auto_profile_update();

                Ok(Box::new(app))
            }),
        );
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn a_plain_key_binding_round_trips_through_the_row() {
            let binding = Binding::Key { code: 0x1B };
            let row = BindingRow::from_binding(1, "Circle", &binding);

            assert_eq!(row.to_binding(), binding);
        }

        #[test]
        fn an_instant_macro_round_trips_through_the_row() {
            let binding = Binding::Macro {
                steps: vec![
                    Binding::Key { code: 0x1B },
                    Binding::Mouse {
                        button: MouseButton::Left,
                    },
                ],
            };
            let row = BindingRow::from_binding(1, "Circle", &binding);

            assert_eq!(row.to_binding(), binding);
        }

        #[test]
        fn a_timed_macro_round_trips_through_the_row_without_being_silently_dropped() {
            let binding = Binding::TimedMacro {
                steps: vec![
                    TimedStep {
                        binding: Binding::Key { code: 0x11 },
                        delay_ms: 0,
                    },
                    TimedStep {
                        binding: Binding::Mouse {
                            button: MouseButton::Left,
                        },
                        delay_ms: 150,
                    },
                ],
            };
            let row = BindingRow::from_binding(1, "Triangle", &binding);

            assert_eq!(row.to_binding(), binding);
        }

        #[test]
        fn a_zero_delay_combo_builds_an_instant_macro_not_a_timed_one() {
            let mut row = BindingRow::new(1, "Circle");
            row.kind = BindingKind::Key;
            row.key_code_input = "0x1B".to_string();
            row.combo_enabled = true;
            row.combo_kind = BindingKind::Key;
            row.combo_key_code_input = "0x20".to_string();
            row.combo_delay_ms_input = "0".to_string();

            assert!(matches!(row.to_binding(), Binding::Macro { .. }));
        }
    }
}

#[cfg(not(windows))]
pub fn run() {
    println!("The OpenDS GUI only runs on Windows. Use --watch-pad or --map here.");
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dualsense() -> DeviceKind {
        DeviceKind::DualSense
    }

    #[test]
    fn with_no_pad_the_status_line_says_so() {
        assert!(status_line(&GuiStatus::default()).contains("No pad connected"));
    }

    #[test]
    fn a_connected_pad_names_itself_and_its_transport() {
        let status = GuiStatus::from_pad(dualsense(), Some(Transport::Usb), None);

        let line = status_line(&status);

        assert!(line.contains("DualSense"));
        assert!(line.contains("Usb"));
    }

    #[test]
    fn a_reported_battery_shows_percent_and_charging_state() {
        let status = GuiStatus::from_pad(
            dualsense(),
            Some(Transport::BluetoothFull),
            Some(Battery {
                percent: 61,
                charging: true,
                full: false,
            }),
        );

        let line = status_line(&status);

        assert!(line.contains("61%"));
        assert!(line.contains("charging"));
    }

    #[test]
    fn no_pad_gives_no_battery_reading() {
        let line = status_line(&GuiStatus::from_pad(
            dualsense(),
            Some(Transport::Usb),
            None,
        ));

        assert!(!line.contains('%'));
    }

    #[test]
    fn the_tooltip_names_the_pad_and_whether_the_virtual_pad_is_on() {
        let status = GuiStatus {
            vpad_active: true,
            ..GuiStatus::from_pad(dualsense(), Some(Transport::Usb), None)
        };

        let tooltip = tray_tooltip(&status);

        assert!(tooltip.contains("DualSense"));
        assert!(tooltip.contains("virtual pad on"));
    }

    #[test]
    fn the_tooltip_says_no_pad_when_nothing_is_connected() {
        assert!(tray_tooltip(&GuiStatus::default()).contains("no pad"));
        assert!(tray_tooltip(&GuiStatus::default()).contains("virtual pad off"));
    }

    #[test]
    fn the_tooltip_includes_battery_when_known() {
        let status = GuiStatus::from_pad(
            dualsense(),
            Some(Transport::Usb),
            Some(Battery {
                percent: 88,
                charging: false,
                full: false,
            }),
        );

        assert!(tray_tooltip(&status).contains("88%"));
    }

    #[test]
    fn with_no_profile_the_mapping_summary_says_buttons_only() {
        assert!(mapping_summary(&MappingStatus::default()).contains("no profile loaded"));
    }

    #[test]
    fn a_load_failure_shows_the_reason_instead_of_a_stale_summary() {
        let status = MappingStatus {
            error: Some("parsing profile foo.json: bad json".to_string()),
            ..MappingStatus::default()
        };

        assert!(mapping_summary(&status).contains("bad json"));
    }

    #[test]
    fn a_loaded_but_disabled_profile_says_off() {
        let status = MappingStatus {
            profile_name: Some("forza".to_string()),
            binding_count: 6,
            enabled: false,
            error: None,
        };

        let summary = mapping_summary(&status);

        assert!(summary.contains("forza"));
        assert!(summary.contains('6'));
        assert!(summary.contains("OFF"));
    }

    #[test]
    fn an_enabled_profile_says_on() {
        let status = MappingStatus {
            profile_name: Some("forza".to_string()),
            binding_count: 6,
            enabled: true,
            error: None,
        };

        assert!(mapping_summary(&status).contains("ON"));
    }

    #[test]
    fn with_no_pad_the_live_view_says_so_instead_of_showing_stale_numbers() {
        assert!(live_view(&GuiStatus::default()).contains("No pad connected"));
    }

    #[test]
    fn a_held_button_shows_up_by_name() {
        use opends_core::types::pad::{PadState, CIRCLE};

        let status =
            GuiStatus::from_pad(dualsense(), Some(Transport::Usb), None).with_state(PadState {
                buttons: CIRCLE,
                ..PadState::default()
            });

        assert!(live_view(&status).contains("Circle"));
    }

    #[test]
    fn no_buttons_held_says_none_rather_than_an_empty_line() {
        let status = GuiStatus::from_pad(dualsense(), Some(Transport::Usb), None)
            .with_state(PadState::default());

        assert!(live_view(&status).contains("buttons held: none"));
    }

    #[test]
    fn an_inactive_finger_shows_up_rather_than_a_stale_position() {
        let status = GuiStatus::from_pad(dualsense(), Some(Transport::Usb), None)
            .with_state(PadState::default());

        assert!(live_view(&status).contains("touch 1: up"));
    }

    #[test]
    fn an_active_finger_shows_its_position() {
        use opends_core::types::pad::{PadState, Touch, TouchPad};

        let status =
            GuiStatus::from_pad(dualsense(), Some(Transport::Usb), None).with_state(PadState {
                touch: TouchPad {
                    first: Touch {
                        active: true,
                        id: 0,
                        x: 400,
                        y: 200,
                    },
                    ..TouchPad::default()
                },
                ..PadState::default()
            });

        assert!(live_view(&status).contains("touch 1: 400,200"));
    }

    #[test]
    fn off_is_the_default_preset_and_encodes_to_the_off_effect() {
        assert_eq!(TriggerPreset::default(), TriggerPreset::Off);
        assert_eq!(TriggerPreset::Off.to_effect(), TriggerEffect::Off);
    }

    #[test]
    fn every_preset_round_trips_through_from_effect() {
        for preset in TriggerPreset::ALL {
            assert_eq!(TriggerPreset::from_effect(preset.to_effect()), *preset);
        }
    }

    #[test]
    fn every_preset_has_its_own_label() {
        for (index, preset) in TriggerPreset::ALL.iter().enumerate() {
            for other in TriggerPreset::ALL.iter().skip(index + 1) {
                assert_ne!(preset.label(), other.label());
            }
        }
    }

    #[test]
    fn an_effect_this_gui_never_offers_still_maps_to_some_preset_without_panicking() {
        let odd = TriggerEffect::Pulse { start: 5, force: 5 };

        assert_eq!(TriggerPreset::from_effect(odd), TriggerPreset::Pulse);
    }

    #[test]
    fn a_hex_key_code_parses() {
        assert_eq!(parse_key_code("0x20"), Some(0x20));
        assert_eq!(parse_key_code("0X1B"), Some(0x1B));
    }

    #[test]
    fn a_decimal_key_code_parses() {
        assert_eq!(parse_key_code("32"), Some(32));
    }

    #[test]
    fn whitespace_around_the_code_is_ignored() {
        assert_eq!(parse_key_code("  0x20  "), Some(0x20));
    }

    #[test]
    fn an_empty_code_parses_to_nothing_rather_than_zero() {
        assert_eq!(parse_key_code(""), None);
        assert_eq!(parse_key_code("   "), None);
    }

    #[test]
    fn garbage_input_parses_to_nothing_rather_than_panicking() {
        assert_eq!(parse_key_code("not a key"), None);
        assert_eq!(parse_key_code("0xZZ"), None);
    }
}
