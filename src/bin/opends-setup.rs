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

use opends_app::driver::setup_driver;

fn main() {
    let arguments: Vec<String> = std::env::args().skip(1).collect();

    if setup_driver::probe_requested(&arguments) {
        opends_app::adapter::console_adapter::attach();
        println!("OpenDS-Setup started. Smart App Control allowed this build.");
        return;
    }

    let mode = setup_driver::mode_from_args(&arguments);
    let self_test = setup_driver::self_test_requested(&arguments);

    match opends_app::adapter::elevation_adapter::ensure_elevated(&arguments) {
        Ok(opends_app::adapter::elevation_adapter::Outcome::AlreadyElevated) => {}
        Ok(opends_app::adapter::elevation_adapter::Outcome::Relaunched) => return,
        Err(error) => {
            opends_app::adapter::console_adapter::attach();
            eprintln!("{error}");
            return;
        }
    }

    app::run(mode, self_test);
}

#[cfg(windows)]
mod app {
    use std::sync::mpsc::Receiver;

    use opends_app::adapter::driver_install_adapter::{
        human_size, Component, DriverInstaller, Selection, Step,
    };
    use opends_app::driver::setup_driver::{self, Mode};
    use opends_app::driver::setup_gui::{step_label, Message, Phase, StepState, Wizard};
    use opends_app::driver::theme;

    use egui::Color32;

    const WINDOW: [f32; 2] = [720.0, 700.0];

    struct Setup {
        wizard: Wizard,
        selection: Selection,
        install_dir_input: String,
        receiver: Option<Receiver<Message>>,
        self_test: bool,
        already_installed: bool,
        repair_choice_made: bool,
    }

    impl Setup {
        fn new(mode: Mode, self_test: bool) -> Self {
            let installer = opends_app::adapter::driver_install_win::WindowsDriverInstaller::new();
            let selection = Selection::default();
            let install_dir_input = selection.install_dir.display().to_string();

            Self {
                wizard: Wizard::new(mode),
                selection,
                install_dir_input,
                receiver: None,
                self_test,
                already_installed: installer.installed_at().is_some(),
                repair_choice_made: false,
            }
        }

        fn showing_repair_choice(&self) -> bool {
            opends_app::driver::setup_gui::showing_repair_choice(
                self.wizard.phase,
                self.wizard.mode,
                self.already_installed,
                self.repair_choice_made,
            )
        }

        fn start(&mut self) {
            self.wizard.begin();
            self.receiver = Some(opends_app::driver::setup_gui::spawn_worker(
                self.wizard.mode,
                self.selection.clone(),
            ));
        }

        fn pump(&mut self) {
            let mut finished = false;

            if let Some(receiver) = self.receiver.as_ref() {
                while let Ok(message) = receiver.try_recv() {
                    let last = matches!(message, Message::Finished(_));
                    self.wizard.apply(message);
                    finished |= last;
                }
            }

            if finished {
                self.receiver = None;
            }
        }
    }

    fn header(ui: &mut egui::Ui, mode: Mode) {
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(4.0, 48.0), egui::Sense::hover());

            ui.painter().rect_filled(
                rect,
                egui::CornerRadius::same(2),
                Color32::from(theme::ACCENT),
            );

            ui.add_space(14.0);

            ui.vertical(|ui| {
                ui.label(theme::heading(match mode {
                    Mode::Install => "Install OpenDS",
                    Mode::Uninstall => "Remove OpenDS",
                }));
                ui.label(theme::muted(
                    "The fastest, safest way to use your PlayStation controller on PC.",
                ));
            });
        });
    }

    fn component_rows(ui: &mut egui::Ui, setup: &mut Setup) {
        let live = setup.wizard.phase == Phase::Idle;

        for component in Component::ALL {
            let mut wanted = setup.selection.wants(*component);
            let required = component.required();

            egui::Frame::new()
                .fill(Color32::from(theme::ROW))
                .corner_radius(egui::CornerRadius::same(theme::RADIUS_ROW))
                .inner_margin(egui::Margin::symmetric(12, 10))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());

                    ui.horizontal(|ui| {
                        ui.add_enabled_ui(live && !required, |ui| {
                            if ui.checkbox(&mut wanted, "").changed() {
                                setup.selection.set(*component, wanted);
                            }
                        });

                        ui.add_space(2.0);

                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                ui.label(theme::body(component.label()));

                                if required {
                                    ui.label(
                                        egui::RichText::new("required")
                                            .size(10.5)
                                            .color(Color32::from(theme::ACCENT)),
                                    );
                                }
                            });

                            ui.label(theme::muted(component.description()));
                        });

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(theme::muted(&human_size(component.approximate_bytes())));
                        });
                    });
                });

            ui.add_space(6.0);
        }
    }

    fn step_rows(ui: &mut egui::Ui, wizard: &Wizard) {
        for (index, step) in Step::ORDER.iter().enumerate() {
            let state = wizard.states[index];

            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(state.glyph())
                        .size(14.0)
                        .color(Color32::from(state.colour())),
                );

                ui.add_space(8.0);

                let colour = match state {
                    StepState::Pending => theme::DIM,
                    _ => theme::TEXT,
                };

                ui.label(
                    egui::RichText::new(step_label(*step, wizard.mode))
                        .size(13.0)
                        .color(Color32::from(colour)),
                );
            });

            ui.add_space(3.0);
        }
    }

    fn repair_choice_panel(ui: &mut egui::Ui, setup: &mut Setup) {
        theme::card().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(theme::body("OpenDS is already installed"));
            ui.add_space(6.0);
            ui.label(theme::muted(
                "Repair reinstalls the driver and the app fresh. Uninstall removes \
                 everything. Neither one downloads anything new.",
            ));
            ui.add_space(14.0);

            ui.horizontal(|ui| {
                let repair = egui::Button::new(
                    egui::RichText::new("Repair / Reinstall")
                        .size(14.0)
                        .strong()
                        .color(Color32::WHITE),
                )
                .fill(Color32::from(theme::ACCENT))
                .corner_radius(egui::CornerRadius::same(theme::RADIUS_ROW));

                if ui.add(repair).clicked() {
                    setup.repair_choice_made = true;
                }

                ui.add_space(10.0);

                if ui.button("Uninstall instead").clicked() {
                    setup.wizard.mode = Mode::Uninstall;
                    setup.repair_choice_made = true;
                }
            });
        });
    }

    fn buttons(ui: &mut egui::Ui, ctx: &egui::Context, setup: &mut Setup) {
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(setup.wizard.close_label()).clicked() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }

                ui.add_space(10.0);

                let label = setup_driver::action_label(setup.wizard.mode);

                ui.add_enabled_ui(setup.wizard.action_enabled(), |ui| {
                    let button = egui::Button::new(
                        egui::RichText::new(label)
                            .size(14.0)
                            .strong()
                            .color(Color32::WHITE),
                    )
                    .fill(Color32::from(theme::ACCENT))
                    .corner_radius(egui::CornerRadius::same(theme::RADIUS_ROW));

                    if ui.add(button).clicked() {
                        setup.start();
                    }
                });
            });
        });
    }

    impl eframe::App for Setup {
        fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
            if self.self_test && self.wizard.phase == Phase::Idle {
                self.self_test = false;
                self.start();
            }

            self.pump();

            if self.wizard.phase == Phase::Running {
                ctx.request_repaint_after(std::time::Duration::from_millis(60));
            }

            if !self.showing_repair_choice() {
                egui::TopBottomPanel::bottom("actions")
                    .frame(
                        egui::Frame::new()
                            .fill(Color32::from(theme::BACKGROUND))
                            .inner_margin(egui::Margin::symmetric(22, 16)),
                    )
                    .show(ctx, |ui| buttons(ui, ctx, self));
            }

            egui::CentralPanel::default()
                .frame(
                    egui::Frame::new()
                        .fill(Color32::from(theme::BACKGROUND))
                        .inner_margin(egui::Margin::symmetric(22, 20)),
                )
                .show(ctx, |ui| {
                    header(ui, self.wizard.mode);

                    ui.add_space(18.0);

                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            if self.showing_repair_choice() {
                                repair_choice_panel(ui, self);
                                return;
                            }

                            let choosing = self.wizard.phase == Phase::Idle
                                && self.wizard.mode == Mode::Install;

                            match choosing {
                                true => {
                                    theme::card().show(ui, |ui| {
                                        ui.set_width(ui.available_width());
                                        ui.label(theme::body("What gets installed"));
                                        ui.add_space(10.0);

                                        component_rows(ui, self);

                                        ui.add_space(4.0);
                                        ui.separator();
                                        ui.add_space(8.0);

                                        ui.horizontal(|ui| {
                                            ui.label(theme::muted("Location"));

                                            if ui
                                                .add(
                                                    egui::TextEdit::singleline(
                                                        &mut self.install_dir_input,
                                                    )
                                                    .desired_width(280.0)
                                                    .font(egui::TextStyle::Monospace),
                                                )
                                                .changed()
                                            {
                                                self.selection.install_dir =
                                                    std::path::PathBuf::from(
                                                        &self.install_dir_input,
                                                    );
                                            }

                                            ui.with_layout(
                                                egui::Layout::right_to_left(egui::Align::Center),
                                                |ui| {
                                                    ui.label(theme::muted(&format!(
                                                        "about {}",
                                                        human_size(self.selection.total_bytes())
                                                    )));
                                                },
                                            );
                                        });
                                    });
                                }
                                false => {
                                    theme::card().show(ui, |ui| {
                                        ui.set_width(ui.available_width());
                                        ui.label(theme::body("Progress"));
                                        ui.add_space(10.0);

                                        step_rows(ui, &self.wizard);

                                        ui.add_space(10.0);

                                        ui.add(
                                            egui::ProgressBar::new(self.wizard.progress)
                                                .desired_height(8.0)
                                                .corner_radius(egui::CornerRadius::same(4))
                                                .fill(Color32::from(match self.wizard.phase {
                                                    Phase::Failed => theme::FAILURE,
                                                    Phase::Succeeded => theme::SUCCESS,
                                                    _ => theme::ACCENT,
                                                })),
                                        );

                                        ui.add_space(8.0);
                                        ui.label(theme::muted(&self.wizard.status_line()));
                                    });

                                    if !self.wizard.outcome.is_empty() {
                                        ui.add_space(14.0);

                                        let colour = match self.wizard.phase {
                                            Phase::Failed => theme::FAILURE,
                                            _ => theme::SUCCESS,
                                        };

                                        egui::Frame::new()
                                            .fill(Color32::from(theme::ROW))
                                            .stroke(egui::Stroke::new(
                                                1.0f32,
                                                Color32::from(colour),
                                            ))
                                            .corner_radius(egui::CornerRadius::same(
                                                theme::RADIUS_ROW,
                                            ))
                                            .inner_margin(egui::Margin::symmetric(14, 12))
                                            .show(ui, |ui| {
                                                ui.set_width(ui.available_width());
                                                ui.label(
                                                    egui::RichText::new(&self.wizard.outcome)
                                                        .size(13.0)
                                                        .color(Color32::from(theme::TEXT)),
                                                );
                                            });
                                    }
                                }
                            }
                        });
                });
        }
    }

    pub fn run(mode: Mode, self_test: bool) {
        let icon = eframe::icon_data::from_png_bytes(include_bytes!("../../assets/opends-64.png"))
            .ok()
            .map(std::sync::Arc::new);

        let mut viewport = egui::ViewportBuilder::default()
            .with_inner_size(WINDOW)
            .with_min_inner_size(WINDOW)
            .with_resizable(false)
            .with_title(match mode {
                Mode::Install => "OpenDS Setup",
                Mode::Uninstall => "OpenDS Uninstall",
            });

        if let Some(icon) = icon {
            viewport = viewport.with_icon(icon);
        }

        let options = eframe::NativeOptions {
            viewport,
            ..Default::default()
        };

        let _ = eframe::run_native(
            "OpenDS Setup",
            options,
            Box::new(move |cc| {
                theme::apply(&cc.egui_ctx);

                Ok(Box::new(Setup::new(mode, self_test)))
            }),
        );
    }
}

#[cfg(not(windows))]
mod app {
    use opends_app::driver::setup_driver::{self, Mode};

    pub fn run(mode: Mode, self_test: bool) {
        let _ = self_test;

        println!("{}", setup_driver::welcome_text(mode));
        println!();
        println!("The OpenDS installer only runs on Windows.");
    }
}
