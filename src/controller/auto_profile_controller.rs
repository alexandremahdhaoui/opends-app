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

pub struct AutoProfileSwitcher {
    rules: Vec<(String, String)>,
    default_profile: Option<String>,
    last_loaded: Option<String>,
}

impl AutoProfileSwitcher {
    pub fn new(rules: Vec<(String, String)>, default_profile: Option<String>) -> Self {
        Self {
            rules,
            default_profile,
            last_loaded: None,
        }
    }

    pub fn profile_to_load(&mut self, foreground_process: Option<&str>) -> Option<String> {
        let target = foreground_process
            .and_then(|name| self.matching_profile(name))
            .or_else(|| self.default_profile.clone());

        if target == self.last_loaded {
            return None;
        }

        self.last_loaded = target.clone();
        target
    }

    fn matching_profile(&self, process_name: &str) -> Option<String> {
        self.rules
            .iter()
            .find(|(rule_process, _)| rule_process.eq_ignore_ascii_case(process_name))
            .map(|(_, profile)| profile.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules() -> Vec<(String, String)> {
        vec![
            ("forza.exe".to_string(), "forza.json".to_string()),
            ("cs2.exe".to_string(), "shooter.json".to_string()),
        ]
    }

    #[test]
    fn a_matching_process_loads_its_configured_profile() {
        let mut switcher = AutoProfileSwitcher::new(rules(), None);

        assert_eq!(
            switcher.profile_to_load(Some("forza.exe")),
            Some("forza.json".to_string())
        );
    }

    #[test]
    fn the_match_is_case_insensitive() {
        let mut switcher = AutoProfileSwitcher::new(rules(), None);

        assert_eq!(
            switcher.profile_to_load(Some("FORZA.EXE")),
            Some("forza.json".to_string())
        );
    }

    #[test]
    fn an_unmatched_process_with_no_default_loads_nothing() {
        let mut switcher = AutoProfileSwitcher::new(rules(), None);

        assert_eq!(switcher.profile_to_load(Some("notepad.exe")), None);
    }

    #[test]
    fn an_unmatched_process_falls_back_to_the_default_profile() {
        let mut switcher = AutoProfileSwitcher::new(rules(), Some("default.json".to_string()));

        assert_eq!(
            switcher.profile_to_load(Some("notepad.exe")),
            Some("default.json".to_string())
        );
    }

    #[test]
    fn no_foreground_process_falls_back_to_the_default_profile() {
        let mut switcher = AutoProfileSwitcher::new(rules(), Some("default.json".to_string()));

        assert_eq!(
            switcher.profile_to_load(None),
            Some("default.json".to_string())
        );
    }

    #[test]
    fn staying_on_the_same_process_never_reloads_the_same_profile_twice() {
        let mut switcher = AutoProfileSwitcher::new(rules(), None);

        assert_eq!(
            switcher.profile_to_load(Some("forza.exe")),
            Some("forza.json".to_string())
        );
        assert_eq!(switcher.profile_to_load(Some("forza.exe")), None);
    }

    #[test]
    fn switching_between_two_matched_games_reloads_each_time() {
        let mut switcher = AutoProfileSwitcher::new(rules(), None);

        assert_eq!(
            switcher.profile_to_load(Some("forza.exe")),
            Some("forza.json".to_string())
        );
        assert_eq!(
            switcher.profile_to_load(Some("cs2.exe")),
            Some("shooter.json".to_string())
        );
    }

    #[test]
    fn leaving_a_matched_game_for_an_unmatched_one_falls_back_to_default_exactly_once() {
        let mut switcher = AutoProfileSwitcher::new(rules(), Some("default.json".to_string()));

        assert_eq!(
            switcher.profile_to_load(Some("forza.exe")),
            Some("forza.json".to_string())
        );
        assert_eq!(
            switcher.profile_to_load(Some("notepad.exe")),
            Some("default.json".to_string())
        );
        assert_eq!(switcher.profile_to_load(Some("explorer.exe")), None);
    }

    #[test]
    fn an_empty_rule_set_still_falls_back_to_the_default() {
        let mut switcher = AutoProfileSwitcher::new(Vec::new(), Some("default.json".to_string()));

        assert_eq!(
            switcher.profile_to_load(Some("anything.exe")),
            Some("default.json".to_string())
        );
    }
}
