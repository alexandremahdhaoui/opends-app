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

use std::sync::mpsc::{channel, Receiver};

use crate::adapter::driver_install_adapter::{Selection, Step};
use crate::driver::setup_driver::{self, Mode};
use crate::driver::theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StepState {
    Pending,
    Running,
    Done,
    Failed,
}

impl StepState {
    pub fn glyph(self) -> &'static str {
        match self {
            StepState::Pending => "•",
            StepState::Running => "▶",
            StepState::Done => "✔",
            StepState::Failed => "✖",
        }
    }

    pub fn colour(self) -> theme::Rgb {
        match self {
            StepState::Pending => theme::DIM,
            StepState::Running => theme::ACCENT,
            StepState::Done => theme::SUCCESS,
            StepState::Failed => theme::FAILURE,
        }
    }
}

pub enum Message {
    Progress(Step, String),
    Finished(Result<(), String>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Running,
    Succeeded,
    Failed,
}

pub struct Wizard {
    pub mode: Mode,
    pub phase: Phase,
    pub states: Vec<StepState>,
    pub detail: String,
    pub outcome: String,
    pub progress: f32,
}

impl Wizard {
    pub fn new(mode: Mode) -> Self {
        Self {
            mode,
            phase: Phase::Idle,
            states: vec![StepState::Pending; Step::ORDER.len()],
            detail: String::new(),
            outcome: String::new(),
            progress: 0.0,
        }
    }

    pub fn begin(&mut self) {
        self.phase = Phase::Running;
        self.states = vec![StepState::Pending; Step::ORDER.len()];
        self.detail.clear();
        self.outcome.clear();
        self.progress = 0.0;
    }

    pub fn apply(&mut self, message: Message) {
        match message {
            Message::Progress(step, detail) => {
                let at = Step::ORDER.iter().position(|known| *known == step);

                if let Some(at) = at {
                    for earlier in 0..at {
                        if self.states[earlier] == StepState::Running {
                            self.states[earlier] = StepState::Done;
                        }
                    }

                    self.states[at] = match step {
                        Step::Done => StepState::Done,
                        _ => StepState::Running,
                    };
                }

                self.detail = detail;
                self.progress = step.percent() as f32 / 100.0;
            }
            Message::Finished(result) => {
                let failed = result.is_err();

                for state in self.states.iter_mut() {
                    if *state == StepState::Running {
                        *state = match failed {
                            true => StepState::Failed,
                            false => StepState::Done,
                        };
                    }
                }

                self.phase = match failed {
                    true => Phase::Failed,
                    false => Phase::Succeeded,
                };

                if !failed {
                    self.progress = 1.0;

                    for state in self.states.iter_mut() {
                        *state = StepState::Done;
                    }
                }

                self.outcome = match result {
                    Ok(()) => setup_driver::outcome_text(self.mode, &Ok(())),
                    Err(text) => text,
                };
            }
        }
    }

    pub fn action_enabled(&self) -> bool {
        self.phase == Phase::Idle
    }

    pub fn close_label(&self) -> &'static str {
        match self.phase {
            Phase::Idle => "Cancel",
            Phase::Running => "Cancel",
            _ => "Close",
        }
    }

    pub fn status_line(&self) -> String {
        match self.phase {
            Phase::Idle => "Ready when you are.".to_string(),
            Phase::Running => self.detail.clone(),
            Phase::Succeeded => "Finished.".to_string(),
            Phase::Failed => "Stopped.".to_string(),
        }
    }
}

pub fn showing_repair_choice(
    phase: Phase,
    mode: Mode,
    already_installed: bool,
    repair_choice_made: bool,
) -> bool {
    phase == Phase::Idle && mode == Mode::Install && already_installed && !repair_choice_made
}

pub fn spawn_worker(mode: Mode, selection: Selection) -> Receiver<Message> {
    let (sender, receiver) = channel();

    std::thread::spawn(move || {
        use crate::adapter::setup_log_adapter::log;

        log(&format!("---- {mode:?} started ----"));

        let installer = crate::adapter::driver_install_win::WindowsDriverInstaller::new();

        if let Some(problem) = setup_driver::precheck(mode, &installer) {
            log(&format!("precheck failed: {problem}"));
            let _ = sender.send(Message::Finished(Err(problem)));
            return;
        }

        let progress = sender.clone();
        let mut report = |step: Step, detail: &str| {
            log(&format!("{step:?}: {detail}"));
            let _ = progress.send(Message::Progress(step, detail.to_string()));
        };

        let result = setup_driver::run_with(mode, &installer, &selection, &mut report);

        match &result {
            Ok(()) => log("FINISHED: ok"),
            Err(error) => log(&format!("FINISHED: error: {error}")),
        }

        let _ = sender.send(Message::Finished(
            result.map_err(|error| setup_driver::outcome_text(mode, &Err(error))),
        ));
    });

    receiver
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wizard() -> Wizard {
        Wizard::new(Mode::Install)
    }

    #[test]
    fn a_fresh_install_over_an_existing_one_shows_the_repair_choice() {
        assert!(showing_repair_choice(
            Phase::Idle,
            Mode::Install,
            true,
            false
        ));
    }

    #[test]
    fn a_fresh_machine_never_shows_a_repair_choice() {
        assert!(!showing_repair_choice(
            Phase::Idle,
            Mode::Install,
            false,
            false
        ));
    }

    #[test]
    fn the_repair_choice_disappears_once_it_has_been_made() {
        assert!(!showing_repair_choice(
            Phase::Idle,
            Mode::Install,
            true,
            true
        ));
    }

    #[test]
    fn the_repair_choice_never_shows_for_uninstall_mode() {
        assert!(!showing_repair_choice(
            Phase::Idle,
            Mode::Uninstall,
            true,
            false
        ));
    }

    #[test]
    fn the_repair_choice_never_shows_once_a_run_has_started() {
        assert!(!showing_repair_choice(
            Phase::Running,
            Mode::Install,
            true,
            false
        ));
        assert!(!showing_repair_choice(
            Phase::Succeeded,
            Mode::Install,
            true,
            false
        ));
        assert!(!showing_repair_choice(
            Phase::Failed,
            Mode::Install,
            true,
            false
        ));
    }

    #[test]
    fn a_fresh_wizard_shows_every_step_as_pending() {
        let wizard = wizard();

        assert_eq!(wizard.phase, Phase::Idle);
        assert!(wizard.states.iter().all(|s| *s == StepState::Pending));
        assert_eq!(wizard.progress, 0.0);
    }

    #[test]
    fn the_action_button_is_only_live_before_a_run_starts() {
        let mut wizard = wizard();

        assert!(wizard.action_enabled());

        wizard.begin();

        assert!(!wizard.action_enabled());
    }

    #[test]
    fn reaching_a_later_step_marks_the_earlier_ones_done() {
        let mut wizard = wizard();
        wizard.begin();

        wizard.apply(Message::Progress(Step::CheckPackage, "a".into()));
        wizard.apply(Message::Progress(Step::InstallDriver, "b".into()));

        assert_eq!(wizard.states[0], StepState::Done);
        assert_eq!(wizard.states[3], StepState::Running);
    }

    #[test]
    fn a_failure_marks_the_step_that_was_running_and_leaves_the_rest_pending() {
        let mut wizard = wizard();
        wizard.begin();

        wizard.apply(Message::Progress(Step::CreateDevice, "trying".into()));
        wizard.apply(Message::Finished(Err("it broke".into())));

        assert_eq!(wizard.phase, Phase::Failed);
        assert_eq!(wizard.states[2], StepState::Failed);
        assert_eq!(wizard.states[5], StepState::Pending);
        assert_eq!(wizard.outcome, "it broke");
    }

    #[test]
    fn success_marks_everything_done_and_fills_the_bar() {
        let mut wizard = wizard();
        wizard.begin();

        wizard.apply(Message::Progress(Step::CheckPackage, "a".into()));
        wizard.apply(Message::Finished(Ok(())));

        assert_eq!(wizard.phase, Phase::Succeeded);
        assert!(wizard.states.iter().all(|s| *s == StepState::Done));
        assert_eq!(wizard.progress, 1.0);
    }

    #[test]
    fn the_close_button_says_cancel_before_and_close_after() {
        let mut wizard = wizard();

        assert_eq!(wizard.close_label(), "Cancel");

        wizard.begin();
        wizard.apply(Message::Finished(Ok(())));

        assert_eq!(wizard.close_label(), "Close");
    }

    #[test]
    fn the_status_line_shows_the_live_detail_while_running() {
        let mut wizard = wizard();
        wizard.begin();

        wizard.apply(Message::Progress(
            Step::TrustCertificate,
            "adding cert".into(),
        ));

        assert_eq!(wizard.status_line(), "adding cert");
    }

    #[test]
    fn progress_never_moves_backwards_across_the_whole_run() {
        let mut wizard = wizard();
        wizard.begin();

        let mut last = 0.0;

        for step in Step::ORDER {
            wizard.apply(Message::Progress(*step, String::new()));

            assert!(wizard.progress >= last, "{:?} went backwards", step);
            last = wizard.progress;
        }
    }

    #[test]
    fn every_step_state_has_a_glyph_and_a_colour_that_differ_from_each_other() {
        let states = [
            StepState::Pending,
            StepState::Running,
            StepState::Done,
            StepState::Failed,
        ];

        for (index, state) in states.iter().enumerate() {
            for other in states.iter().skip(index + 1) {
                assert_ne!(state.glyph(), other.glyph());
                assert_ne!(state.colour(), other.colour());
            }
        }
    }

    #[test]
    fn an_uninstall_wizard_reports_its_own_outcome_text() {
        let mut wizard = Wizard::new(Mode::Uninstall);
        wizard.begin();
        wizard.apply(Message::Finished(Ok(())));

        assert!(wizard.outcome.contains("Removed"));
    }

    #[test]
    fn the_status_line_differs_in_every_phase() {
        let mut wizard = wizard();
        let idle = wizard.status_line();

        wizard.begin();
        wizard.apply(Message::Progress(Step::CheckPackage, "working".into()));
        let running = wizard.status_line();

        wizard.apply(Message::Finished(Ok(())));
        let done = wizard.status_line();

        assert_ne!(idle, running);
        assert_ne!(running, done);
        assert_ne!(idle, done);
    }

    #[test]
    fn a_failed_run_says_stopped_rather_than_finished() {
        let mut wizard = wizard();
        wizard.begin();
        wizard.apply(Message::Finished(Err("nope".into())));

        assert_eq!(wizard.status_line(), "Stopped.");
        assert_eq!(wizard.close_label(), "Close");
    }

    #[test]
    fn a_progress_message_for_an_unknown_step_does_not_panic() {
        let mut wizard = wizard();
        wizard.begin();

        wizard.apply(Message::Progress(Step::Done, "finishing".into()));

        assert_eq!(wizard.states[Step::ORDER.len() - 1], StepState::Done);
    }

    #[test]
    fn the_action_stays_disabled_after_a_failure_until_a_fresh_begin() {
        let mut wizard = wizard();
        wizard.begin();
        wizard.apply(Message::Finished(Err("nope".into())));

        assert!(!wizard.action_enabled());
    }

    #[test]
    fn a_rerun_clears_the_previous_outcome() {
        let mut wizard = wizard();
        wizard.begin();
        wizard.apply(Message::Finished(Err("old failure".into())));

        wizard.begin();

        assert!(wizard.outcome.is_empty());
        assert_eq!(wizard.phase, Phase::Running);
    }
}
