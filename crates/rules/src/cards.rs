//! Card definitions and the match deck (docs/design.md §9, §14).
//!
//! A card is an element (or none), a timing, a cost, a targeting rule and one
//! keyword effect. Depth comes from how keywords meet the ring, wards and
//! reaction windows, not from bespoke card text.

use serde::{Deserialize, Serialize};

use necromy_dice::Face;

use crate::gods::Element;
use crate::rng::Rng;

/// Index into [`POOL`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct DefId(pub u16);

impl DefId {
    pub fn def(self) -> &'static CardDef {
        &POOL[self.0 as usize]
    }
}

/// One physical card in this match.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CardId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CardKind {
    /// Spells; cost Spirit.
    Rite,
    /// Tricks and traps.
    Trick,
    /// Actions on a corpse under the champion.
    Body,
}

/// When a card may be played (docs/design.md §11.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Timing {
    /// Only on your own turn.
    Own,
    /// Your turn, or the Enter and End windows of someone else's.
    Instant,
    /// Only in a Target window, against the card that opened it.
    Response,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TargetRule {
    /// The caster's own champion.
    Caster,
    /// Any champion within range of the caster, the caster included.
    Champion { range: u32 },
    /// Another champion within range.
    Enemy { range: u32 },
    /// A hex within range with no champion on it.
    EmptyHex { range: u32 },
    /// The corpse under the caster.
    Corpse,
    /// The card that opened the Target window.
    Pending,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Effect {
    Damage(u8),
    Heal(u8),
    /// Damage, and the caster heals as much.
    Drain(u8),
    /// Damage only if the target is already wounded.
    Finish(u8),
    /// A ward of the card's element (§4).
    Ward,
    /// The target loses its movement.
    Root,
    /// Extra move points this turn.
    Haste(u8),
    Draw(u8),
    /// Hidden on a hex; springs on the next rival who enters it.
    Trap(TrapEffect),
    /// The hex becomes a grove.
    Grow,
    /// The caster steps to the target hex without walking.
    Blink,
    /// The pending card fizzles, unless its element quenches this card's.
    Cancel,
    /// Corpse → Spirit and a move point (Trishna).
    BodyFuel,
    /// Corpse → a Metal ward (Ahamar).
    BodyLegion,
    /// Corpse → Spirit and healing (Maya).
    BodyDissolve,
    /// Corpse → healing and a card (Zaga).
    BodyRest,
    /// Corpse → a grove at once, and healing (Bhava).
    BodySeed,
    /// Reads Trishna's stage (§5): Generosity feeds everyone near, Thirst
    /// feeds the caster at the neighbours' cost, Devouring burns the bodies near.
    Feast,
}

impl Effect {
    /// Has a number the generation chain and the god's stage can bend.
    pub const fn scales(self) -> bool {
        !matches!(
            self,
            Effect::Ward
                | Effect::Root
                | Effect::Grow
                | Effect::Blink
                | Effect::Cancel
                | Effect::BodyLegion
                | Effect::Trap(TrapEffect::Root)
        )
    }

    /// Hurts its target, so a ward can stop it.
    pub const fn is_harmful(self) -> bool {
        matches!(
            self,
            Effect::Damage(_)
                | Effect::Drain(_)
                | Effect::Finish(_)
                | Effect::Root
                | Effect::Trap(_)
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrapEffect {
    Damage(u8),
    Root,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CardDef {
    pub name: &'static str,
    pub text: &'static str,
    pub element: Option<Element>,
    pub kind: CardKind,
    pub timing: Timing,
    /// Spirit to pay.
    pub cost: u8,
    pub target: TargetRule,
    pub effect: Effect,
}

impl CardDef {
    /// The face a card gives when burned in battle (§12.1): tricks strike,
    /// bodies shield, rites carry their element's side of the day. Earth,
    /// on both sides, gives the Element face.
    pub fn burn_face(&self) -> Face {
        match (self.kind, self.element) {
            (CardKind::Trick, _) => Face::Strike,
            (CardKind::Body, _) | (CardKind::Rite, None) => Face::Shield,
            (CardKind::Rite, Some(Element::Earth)) => Face::Element,
            (CardKind::Rite, Some(e)) if e.is_yang() => Face::Sun,
            (CardKind::Rite, Some(_)) => Face::Moon,
        }
    }
}

// One positional row per card keeps the pool readable as a table.
#[allow(clippy::too_many_arguments)]
const fn card(
    name: &'static str,
    text: &'static str,
    element: Option<Element>,
    kind: CardKind,
    timing: Timing,
    cost: u8,
    target: TargetRule,
    effect: Effect,
) -> CardDef {
    CardDef {
        name,
        text,
        element,
        kind,
        timing,
        cost,
        target,
        effect,
    }
}

use CardKind::*;
use Effect::*;
use Element::*;
use TargetRule::*;
use Timing::*;

/// The whole collection. A match plays a slice of it (§14).
#[rustfmt::skip]
pub const POOL: &[CardDef] = &[
    // Wood — Bhava
    card("Побег сквозь камень", "Клетка в 2 шагах зарастает рощей.", Some(Wood), Rite, Own, 1, EmptyHex { range: 2 }, Grow),
    card("Цепкий корень", "Ловушка рядом: вошедший соперник теряет ход.", Some(Wood), Trick, Own, 0, EmptyHex { range: 1 }, Trap(TrapEffect::Root)),
    card("Живица", "Вылечить чемпиона в 3 шагах на 2.", Some(Wood), Rite, Instant, 1, Champion { range: 3 }, Heal(2)),
    card("Шипы чащи", "1 урона сопернику в 2 шагах.", Some(Wood), Trick, Instant, 0, Enemy { range: 2 }, Damage(1)),
    card("Семя в мёртвом", "Тело под тобой сразу прорастает рощей. Вылечись на 2.", Some(Wood), Body, Own, 0, Corpse, BodySeed),
    // Fire — Trishna
    card("Пламя пира", "2 урона сопернику в 3 шагах.", Some(Fire), Rite, Own, 2, Enemy { range: 3 }, Damage(2)),
    card("Сжечь как топливо", "Тело под тобой: +2 Духа и +1 очко движения.", Some(Fire), Body, Own, 0, Corpse, BodyFuel),
    card("Второе блюдо", "Возьми 2 карты.", Some(Fire), Trick, Own, 0, Caster, Draw(2)),
    card("Жар в крови", "+2 очка движения в этот ход.", Some(Fire), Rite, Own, 1, Caster, Haste(2)),
    card("Искра", "1 урона сопернику в 2 шагах.", Some(Fire), Trick, Instant, 0, Enemy { range: 2 }, Damage(1)),
    card("Пир урожая", "Щедрость: все рядом +1 здоровья. Жажда: ты +2, соседи −1. Пожирание: тела рядом сгорают, +1 Духа за каждое, ты −1.", Some(Fire), Rite, Own, 1, Caster, Feast),
    // Earth — Zaga
    card("Тишь", "Ответ: карта против тебя гаснет, если её стихия не гасит землю.", Some(Earth), Rite, Response, 1, Pending, Cancel),
    card("Оковы", "Соперник в 2 шагах теряет ход.", Some(Earth), Rite, Own, 1, Enemy { range: 2 }, Root),
    card("Упокоить", "Тело под тобой: вылечись на 1 и возьми карту.", Some(Earth), Body, Own, 0, Corpse, BodyRest),
    card("Бремя", "Соперник в 2 шагах теряет ход.", Some(Earth), Trick, Instant, 0, Enemy { range: 2 }, Root),
    card("Отзвучавшая нота", "3 урона раненому сопернику в 2 шагах.", Some(Earth), Rite, Own, 2, Enemy { range: 2 }, Finish(3)),
    card("Власяница", "Оберег земли на себя.", Some(Earth), Trick, Own, 0, Caster, Ward),
    // Metal — Ahamar
    card("Именной оберег", "Оберег железа на чемпиона в 3 шагах.", Some(Metal), Rite, Instant, 1, Champion { range: 3 }, Ward),
    card("Вписать в легион", "Тело под тобой: оберег железа.", Some(Metal), Body, Own, 0, Corpse, BodyLegion),
    card("Реестр", "Ловушка рядом: вошедший соперник получает 2 урона.", Some(Metal), Trick, Own, 0, EmptyHex { range: 1 }, Trap(TrapEffect::Damage(2))),
    card("Приговор порядка", "2 урона сопернику в 2 шагах.", Some(Metal), Rite, Own, 2, Enemy { range: 2 }, Damage(2)),
    card("Присяга", "Ответ: оберег железа на себя до удара.", Some(Metal), Trick, Response, 0, Caster, Ward),
    // Water — Maya
    card("Растворить душу", "Тело под тобой: +1 Духа, вылечись на 2.", Some(Water), Body, Own, 0, Corpse, BodyDissolve),
    card("Туманный шаг", "Перенестись на пустую клетку в 3 шагах.", Some(Water), Rite, Own, 1, EmptyHex { range: 3 }, Blink),
    card("Морок", "Ответ: карта против тебя гаснет, если её стихия не гасит воду.", Some(Water), Rite, Response, 1, Pending, Cancel),
    card("Дымная ладонь", "1 урона сопернику в 3 шагах, вылечись на 1.", Some(Water), Trick, Instant, 0, Enemy { range: 3 }, Drain(1)),
    card("Бирюзовый оберег", "Оберег воды на себя.", Some(Water), Rite, Own, 1, Caster, Ward),
    // Neutral
    card("Короткий путь", "+1 очко движения в этот ход.", None, Trick, Own, 0, Caster, Haste(1)),
    card("Бинт", "Вылечись на 1.", None, Trick, Instant, 0, Caster, Heal(1)),
];

/// Distinct cards in a match and copies of each.
pub const SLICE_SIZE: usize = 20;
pub const COPIES: u32 = 2;

/// Picks this match's slice of the pool, then makes sure every ward in it
/// has an answer: a harmful card of the one element that quenches it (§2).
pub fn match_slice(rng: &mut Rng) -> Vec<DefId> {
    let mut all: Vec<DefId> = (0..POOL.len() as u16).map(DefId).collect();
    rng.shuffle(&mut all);
    let mut slice: Vec<DefId> = all[..SLICE_SIZE.min(all.len())].to_vec();

    loop {
        let missing = slice.iter().find_map(|id| {
            let def = id.def();
            let ward = ward_element(def)?;
            let answered = slice.iter().any(|a| answers(a.def(), ward));
            (!answered).then_some(ward)
        });
        let Some(ward) = missing else { break };
        // The pool itself always holds an answer; tests check that.
        let answer = all
            .iter()
            .find(|a| !slice.contains(a) && answers(a.def(), ward))
            .copied()
            .expect("the pool answers every ward");
        slice.push(answer);
    }
    slice.sort();
    slice
}

/// Element of the ward a card raises, if it raises one.
pub fn ward_element(def: &CardDef) -> Option<Element> {
    match def.effect {
        Effect::Ward => def.element,
        Effect::BodyLegion => Some(Element::Metal),
        _ => None,
    }
}

fn answers(def: &CardDef, ward: Element) -> bool {
    def.effect.is_harmful() && def.element == Some(ward.quenched_by())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pool_answers_every_ward() {
        for def in POOL {
            if let Some(ward) = ward_element(def) {
                assert!(
                    POOL.iter().any(|a| answers(a, ward)),
                    "{} has no answer in the pool",
                    def.name
                );
            }
        }
    }

    #[test]
    fn slices_always_answer_their_wards() {
        for seed in 0..200 {
            let slice = match_slice(&mut Rng::new(seed));
            for id in &slice {
                if let Some(ward) = ward_element(id.def()) {
                    assert!(slice.iter().any(|a| answers(a.def(), ward)), "seed {seed}");
                }
            }
        }
    }

    #[test]
    fn slices_differ_between_seeds() {
        assert_ne!(match_slice(&mut Rng::new(1)), match_slice(&mut Rng::new(2)));
    }

    #[test]
    fn responses_target_the_pending_card_or_the_caster() {
        for def in POOL.iter().filter(|d| d.timing == Timing::Response) {
            assert!(
                matches!(def.target, TargetRule::Pending | TargetRule::Caster),
                "{}",
                def.name
            );
        }
    }
}
