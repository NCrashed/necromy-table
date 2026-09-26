//! Russian display names and colours for rules types. The rules crate keeps
//! stable English identifiers; everything the player reads comes from here.

use bevy::prelude::*;
use necromy_rules::{CardKind, Element, God, Timing};

pub fn god(god: God) -> &'static str {
    match god {
        God::Bhava => "Бхава",
        God::Trishna => "Тришна",
        God::Zaga => "Зага",
        God::Ahamar => "Ахамар",
        God::Maya => "Майя",
    }
}

pub fn element(element: Element) -> &'static str {
    match element {
        Element::Wood => "дерево",
        Element::Fire => "огонь",
        Element::Earth => "земля",
        Element::Metal => "железо",
        Element::Water => "вода",
    }
}

pub fn kind(kind: CardKind) -> &'static str {
    match kind {
        CardKind::Rite => "обряд",
        CardKind::Trick => "уловка",
        CardKind::Body => "тело",
    }
}

pub fn timing(timing: Timing) -> &'static str {
    match timing {
        Timing::Own => "свой ход",
        Timing::Instant => "мгновенно",
        Timing::Response => "ответ",
    }
}

/// Element colour: the accent of the god who owns it; grey for neutral.
pub fn element_color(element: Option<Element>) -> Color {
    match element {
        Some(e) => {
            let [r, g, b] = God::ALL[e.index()].accent();
            Color::srgb_u8(r, g, b)
        }
        None => Color::srgb(0.62, 0.60, 0.56),
    }
}
