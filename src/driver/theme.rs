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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

pub const BACKGROUND: Rgb = Rgb(15, 17, 21);
pub const CARD: Rgb = Rgb(23, 26, 33);
pub const ROW: Rgb = Rgb(30, 34, 43);
pub const ROW_ACTIVE: Rgb = Rgb(38, 44, 58);
pub const LINE: Rgb = Rgb(46, 52, 66);

pub const TEXT: Rgb = Rgb(230, 232, 238);
pub const MUTED: Rgb = Rgb(138, 144, 160);
pub const DIM: Rgb = Rgb(92, 98, 114);

pub const ACCENT: Rgb = Rgb(76, 141, 255);
pub const ACCENT_DIM: Rgb = Rgb(46, 84, 153);
pub const SUCCESS: Rgb = Rgb(74, 222, 128);
pub const FAILURE: Rgb = Rgb(248, 113, 113);
pub const WARNING: Rgb = Rgb(240, 180, 60);

pub const RADIUS_CARD: u8 = 10;
pub const RADIUS_ROW: u8 = 6;

pub fn luminance(colour: Rgb) -> f32 {
    let channel = |value: u8| {
        let unit = f32::from(value) / 255.0;

        match unit <= 0.04045 {
            true => unit / 12.92,
            false => ((unit + 0.055) / 1.055).powf(2.4),
        }
    };

    0.2126 * channel(colour.0) + 0.7152 * channel(colour.1) + 0.0722 * channel(colour.2)
}

pub fn contrast(front: Rgb, back: Rgb) -> f32 {
    let a = luminance(front);
    let b = luminance(back);
    let (lighter, darker) = match a > b {
        true => (a, b),
        false => (b, a),
    };

    (lighter + 0.05) / (darker + 0.05)
}

#[cfg(windows)]
mod paint {
    use super::*;

    impl From<Rgb> for egui::Color32 {
        fn from(colour: Rgb) -> Self {
            egui::Color32::from_rgb(colour.0, colour.1, colour.2)
        }
    }

    pub fn apply(ctx: &egui::Context) {
        let mut visuals = egui::Visuals::dark();
        let line = egui::Stroke::new(1.0f32, egui::Color32::from(LINE));

        visuals.panel_fill = BACKGROUND.into();
        visuals.window_fill = BACKGROUND.into();
        visuals.extreme_bg_color = ROW.into();
        visuals.override_text_color = Some(TEXT.into());
        visuals.selection.bg_fill = ACCENT_DIM.into();
        visuals.selection.stroke = egui::Stroke::new(1.0f32, egui::Color32::from(ACCENT));

        visuals.widgets.noninteractive.bg_fill = CARD.into();
        visuals.widgets.noninteractive.bg_stroke = line;
        visuals.widgets.inactive.bg_fill = ROW.into();
        visuals.widgets.inactive.bg_stroke = line;
        visuals.widgets.hovered.bg_fill = ROW_ACTIVE.into();
        visuals.widgets.hovered.bg_stroke =
            egui::Stroke::new(1.0f32, egui::Color32::from(ACCENT_DIM));
        visuals.widgets.active.bg_fill = ACCENT_DIM.into();
        visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0f32, egui::Color32::from(ACCENT));

        for widget in [
            &mut visuals.widgets.noninteractive,
            &mut visuals.widgets.inactive,
            &mut visuals.widgets.hovered,
            &mut visuals.widgets.active,
        ] {
            widget.corner_radius = egui::CornerRadius::same(RADIUS_ROW);
        }

        ctx.set_visuals(visuals);

        let mut style = (*ctx.style()).clone();
        style.spacing.item_spacing = egui::vec2(10.0, 10.0);
        style.spacing.button_padding = egui::vec2(16.0, 9.0);
        ctx.set_style(style);
    }

    pub fn heading(text: &str) -> egui::RichText {
        egui::RichText::new(text)
            .size(20.0)
            .strong()
            .color(egui::Color32::from(TEXT))
    }

    pub fn body(text: &str) -> egui::RichText {
        egui::RichText::new(text)
            .size(13.5)
            .color(egui::Color32::from(TEXT))
    }

    pub fn muted(text: &str) -> egui::RichText {
        egui::RichText::new(text)
            .size(12.5)
            .color(egui::Color32::from(MUTED))
    }

    pub fn card() -> egui::Frame {
        egui::Frame::new()
            .fill(egui::Color32::from(CARD))
            .stroke(egui::Stroke::new(1.0f32, egui::Color32::from(LINE)))
            .corner_radius(egui::CornerRadius::same(RADIUS_CARD))
            .inner_margin(egui::Margin::symmetric(16, 14))
    }
}

#[cfg(windows)]
pub use paint::{apply, body, card, heading, muted};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn body_text_on_a_card_clears_the_accessibility_floor() {
        assert!(
            contrast(TEXT, CARD) >= 4.5,
            "contrast {}",
            contrast(TEXT, CARD)
        );
    }

    #[test]
    fn muted_text_stays_readable_rather_than_decorative() {
        assert!(
            contrast(MUTED, CARD) >= 3.0,
            "contrast {}",
            contrast(MUTED, CARD)
        );
    }

    #[test]
    fn success_and_failure_are_distinguishable_from_the_card_and_each_other() {
        assert!(contrast(SUCCESS, CARD) >= 3.0);
        assert!(contrast(FAILURE, CARD) >= 3.0);
        assert_ne!(luminance(SUCCESS), luminance(FAILURE));
    }

    #[test]
    fn the_accent_reads_against_the_card_it_sits_on() {
        assert!(
            contrast(ACCENT, CARD) >= 3.0,
            "contrast {}",
            contrast(ACCENT, CARD)
        );
    }

    #[test]
    fn the_surfaces_get_lighter_as_they_come_forward() {
        assert!(luminance(BACKGROUND) < luminance(CARD));
        assert!(luminance(CARD) < luminance(ROW));
        assert!(luminance(ROW) < luminance(ROW_ACTIVE));
    }

    #[test]
    fn dim_is_dimmer_than_muted_which_is_dimmer_than_body() {
        assert!(luminance(DIM) < luminance(MUTED));
        assert!(luminance(MUTED) < luminance(TEXT));
    }

    #[test]
    fn the_warning_colour_is_not_mistakable_for_success_or_failure() {
        assert_ne!(WARNING, SUCCESS);
        assert_ne!(WARNING, FAILURE);
        assert!(contrast(WARNING, CARD) >= 3.0);
    }
}
