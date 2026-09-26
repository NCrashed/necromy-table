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

pub fn face(face: necromy_rules::Face) -> &'static str {
    use necromy_rules::Face;
    match face {
        Face::Strike => "удар",
        Face::Shield => "щит",
        Face::Sun => "солнце",
        Face::Moon => "луна",
        Face::Element => "стихия",
        Face::Blank => "пусто",
    }
}

/// Stage names from light (0) to dark (2), docs/design.md §5.
pub fn stage(god: God, stage: u8) -> &'static str {
    let names = match god {
        God::Trishna => ["Щедрость", "Жажда", "Пожирание"],
        God::Ahamar => ["Маска", "Трещина", "Разоблачение"],
        God::Maya => ["Покой", "Манипуляция", "Гнев"],
        God::Zaga => ["Покой", "Бремя", "Приговор"],
        God::Bhava => ["Росток", "Чаща", "Заросль"],
    };
    names[(stage as usize).min(2)]
}

pub fn deed(deed: necromy_rules::Deed) -> String {
    use necromy_rules::{BodyVerb, Deed};
    match deed {
        Deed::Played(e) => format!("играть карты стихии «{}»", element(e)),
        Deed::Body(BodyVerb::Fuel) => "сжигать тела".into(),
        Deed::Body(BodyVerb::Legion) => "вписывать тела в легион".into(),
        Deed::Body(BodyVerb::Dissolve) => "растворять души".into(),
        Deed::Body(BodyVerb::Rest) => "упокаивать тела".into(),
        Deed::Body(BodyVerb::Seed) => "сеять в мёртвом".into(),
        Deed::Attacked => "нападать первым".into(),
        Deed::Fought => "драться".into(),
        Deed::Prayed => "молиться в храмах".into(),
        Deed::Claimed => "занимать поселения и храмы".into(),
    }
}

pub fn taste(kind: necromy_rules::TasteKind) -> &'static str {
    use necromy_rules::TasteKind;
    match kind {
        TasteKind::Balance => "Равновесие: ценится всё понемногу",
        TasteKind::Land => "Земля: владения стоят вдвое",
        TasteKind::Stage => "Сцена: земля почти ничего не стоит, характер вдвое",
        TasteKind::Arena => "Арена: победы стоят вдвое, храмы ничего",
    }
}

pub fn style_reason(reason: necromy_rules::StyleReason) -> &'static str {
    use necromy_rules::StyleReason;
    match reason {
        StyleReason::Territory => "владения",
        StyleReason::Battle => "бой",
        StyleReason::Manner => "верность манере",
        StyleReason::Oath => "нарушенная клятва",
    }
}

/// Name and what the terrain does, for the hover tooltip.
pub fn terrain(terrain: necromy_rules::Terrain) -> (&'static str, &'static str) {
    use necromy_rules::Terrain;
    match terrain {
        Terrain::Plains => ("Равнина", "обычная земля"),
        Terrain::Forest => ("Лес", "идти дороже"),
        Terrain::Mountain => ("Горы", "идти дороже; защитнику +1 кубик"),
        Terrain::Swamp => ("Топь", "обычный проход"),
        Terrain::Settlement => ("Поселение", "войди — займёшь; на рассвете даёт Стиль"),
        Terrain::Temple => (
            "Храм",
            "займи ради Стиля; конец хода здесь — молитва богу края",
        ),
        Terrain::Ruins => ("Руины", "пока без особых свойств"),
        Terrain::Stones => ("Камни силы", "пока без особых свойств"),
        Terrain::Grove => ("Роща", "выросла из нетронутого тела; идти дороже"),
        Terrain::Table => ("Стол Ахамара", "центр; займи — больше всего Стиля"),
    }
}

/// "край Бхавы": the god's name in the genitive.
pub fn god_genitive(god: God) -> &'static str {
    match god {
        God::Bhava => "Бхавы",
        God::Trishna => "Тришны",
        God::Zaga => "Заги",
        God::Ahamar => "Ахамара",
        God::Maya => "Майи",
    }
}

/// The taste's short name and its explanation, split for chip and tooltip.
pub fn taste_parts(kind: necromy_rules::TasteKind) -> (&'static str, &'static str) {
    let full = taste(kind);
    full.split_once(": ").unwrap_or((full, ""))
}
