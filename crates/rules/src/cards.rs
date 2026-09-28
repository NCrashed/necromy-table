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
    /// The caster slips out of sight (§11.6).
    Hide,
    /// Reads Trishna's stage (§5): Generosity feeds everyone near, Thirst
    /// feeds the caster at the neighbours' cost, Devouring burns the bodies near.
    Feast,
    /// Stacks of poison of the card's element (docs/design.md §20.1).
    Poison(u8),
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
                | Effect::Hide
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
                | Effect::Poison(_)
        )
    }

    /// Heals someone, so its element may cure or feed a poison (§20.1).
    pub const fn heals(self) -> bool {
        matches!(
            self,
            Effect::Heal(_)
                | Effect::Drain(_)
                | Effect::BodyDissolve
                | Effect::BodyRest
                | Effect::BodySeed
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrapEffect {
    Damage(u8),
    Root,
    Poison(u8),
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

/// A change on one copy of a card, not its definition (docs/design.md §7.3):
/// a wish blessed or blighted it, or forged it. Deltas are clamped where
/// they apply: cost 0..=3, numbers and ranges at least 1.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardMod {
    pub cost: i8,
    /// To the effect's number (damage, healing, move points, cards, poison).
    pub power: i8,
    /// To the target's range.
    pub range: i8,
    /// A new element, if changed.
    pub element: Option<Option<Element>>,
    /// A new timing, if changed.
    pub timing: Option<Timing>,
    /// Its own name: a forged card's.
    pub name: Option<String>,
}

impl CardMod {
    /// Adds another change on top of this one.
    pub fn stack(&mut self, more: &CardMod) {
        self.cost = self.cost.saturating_add(more.cost);
        self.power = self.power.saturating_add(more.power);
        self.range = self.range.saturating_add(more.range);
        if more.element.is_some() {
            self.element = more.element;
        }
        if more.timing.is_some() {
            self.timing = more.timing;
        }
        if more.name.is_some() {
            self.name = more.name.clone();
        }
    }
}

fn bend(n: u8, delta: i8) -> u8 {
    (i16::from(n) + i16::from(delta)).max(1) as u8
}

fn reach(range: u32, delta: i8) -> u32 {
    (range as i64 + i64::from(delta)).max(1) as u32
}

impl Effect {
    /// The effect with its number moved by `delta`, never below 1; effects
    /// without a number stay as they are.
    pub fn bent(self, delta: i8) -> Effect {
        match self {
            Effect::Damage(n) => Effect::Damage(bend(n, delta)),
            Effect::Heal(n) => Effect::Heal(bend(n, delta)),
            Effect::Drain(n) => Effect::Drain(bend(n, delta)),
            Effect::Finish(n) => Effect::Finish(bend(n, delta)),
            Effect::Haste(n) => Effect::Haste(bend(n, delta)),
            Effect::Draw(n) => Effect::Draw(bend(n, delta)),
            Effect::Poison(n) => Effect::Poison(bend(n, delta)),
            Effect::Trap(TrapEffect::Damage(n)) => Effect::Trap(TrapEffect::Damage(bend(n, delta))),
            Effect::Trap(TrapEffect::Poison(n)) => Effect::Trap(TrapEffect::Poison(bend(n, delta))),
            other => other,
        }
    }
}

impl TargetRule {
    /// The rule with its range moved by `delta`, never below 1.
    pub fn reaching(self, delta: i8) -> TargetRule {
        match self {
            TargetRule::Champion { range } => TargetRule::Champion {
                range: reach(range, delta),
            },
            TargetRule::Enemy { range } => TargetRule::Enemy {
                range: reach(range, delta),
            },
            TargetRule::EmptyHex { range } => TargetRule::EmptyHex {
                range: reach(range, delta),
            },
            other => other,
        }
    }
}

impl CardDef {
    /// This definition as one changed copy plays.
    pub fn with(self, m: &CardMod) -> CardDef {
        CardDef {
            cost: (i16::from(self.cost) + i16::from(m.cost)).clamp(0, 3) as u8,
            effect: self.effect.bent(m.power),
            target: self.target.reaching(m.range),
            element: m.element.unwrap_or(self.element),
            timing: m.timing.unwrap_or(self.timing),
            ..self
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
    card("Болиголов", "Сопернику в 2 шагах яд 2: −1 здоровья в начале хода, не ниже 1.", Some(Wood), Trick, Instant, 0, Enemy { range: 2 }, Poison(2)),
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
    card("Чумной вздох", "Сопернику в 2 шагах яд 3: −1 здоровья в начале хода, не ниже 1.", Some(Earth), Rite, Own, 1, Enemy { range: 2 }, Poison(3)),
    // Metal — Ahamar
    card("Именной оберег", "Оберег железа на чемпиона в 3 шагах.", Some(Metal), Rite, Instant, 1, Champion { range: 3 }, Ward),
    card("Вписать в легион", "Тело под тобой: оберег железа.", Some(Metal), Body, Own, 0, Corpse, BodyLegion),
    card("Реестр", "Ловушка рядом: вошедший соперник получает 2 урона.", Some(Metal), Trick, Own, 0, EmptyHex { range: 1 }, Trap(TrapEffect::Damage(2))),
    card("Приговор порядка", "2 урона сопернику в 2 шагах.", Some(Metal), Rite, Own, 2, Enemy { range: 2 }, Damage(2)),
    card("Присяга", "Ответ: оберег железа на себя до удара.", Some(Metal), Trick, Response, 0, Caster, Ward),
    card("Калёное железо", "Вылечить чемпиона рядом на 1. Снимает яд дерева.", Some(Metal), Trick, Instant, 0, Champion { range: 1 }, Heal(1)),
    // Water — Maya
    card("Растворить душу", "Тело под тобой: +1 Духа, вылечись на 2.", Some(Water), Body, Own, 0, Corpse, BodyDissolve),
    card("Туманный шаг", "Перенестись на пустую клетку в 3 шагах.", Some(Water), Rite, Own, 1, EmptyHex { range: 3 }, Blink),
    card("Морок", "Ответ: карта против тебя гаснет, если её стихия не гасит воду.", Some(Water), Rite, Response, 1, Pending, Cancel),
    card("Дымная ладонь", "1 урона сопернику в 3 шагах, вылечись на 1.", Some(Water), Trick, Instant, 0, Enemy { range: 3 }, Drain(1)),
    card("Бирюзовый оберег", "Оберег воды на себя.", Some(Water), Rite, Own, 1, Caster, Ward),
    card("Пелена", "Скройся: соперники не видят тебя, пока ты не раскроешься.", Some(Water), Rite, Own, 1, Caster, Hide),
    card("Мёртвая вода", "Ловушка рядом: вошедший соперник получает яд 2.", Some(Water), Trick, Own, 0, EmptyHex { range: 1 }, Trap(TrapEffect::Poison(2))),
    // Neutral
    card("Короткий путь", "+1 очко движения в этот ход.", None, Trick, Own, 0, Caster, Haste(1)),
    card("Бинт", "Вылечись на 1.", None, Trick, Instant, 0, Caster, Heal(1)),
];

/// Distinct cards in a match and copies of each.
pub const SLICE_SIZE: usize = 20;
pub const COPIES: u32 = 2;

/// Picks this match's slice of the pool, then makes sure every ward and
/// every poison in it has an answer (§2, §20.1): a harmful card, or a heal,
/// of the one element that quenches it.
pub fn match_slice(rng: &mut Rng) -> Vec<DefId> {
    let mut all: Vec<DefId> = (0..POOL.len() as u16).map(DefId).collect();
    rng.shuffle(&mut all);
    let mut slice: Vec<DefId> = all[..SLICE_SIZE.min(all.len())].to_vec();

    loop {
        let missing = slice.iter().find_map(|id| {
            let need = Need::of(id.def())?;
            let answered = slice.iter().any(|a| need.met_by(a.def()));
            (!answered).then_some(need)
        });
        let Some(need) = missing else { break };
        // The pool itself always holds an answer; tests check that.
        let answer = all
            .iter()
            .find(|a| !slice.contains(a) && need.met_by(a.def()))
            .copied()
            .expect("the pool answers every ward and poison");
        slice.push(answer);
    }
    slice.sort();
    slice
}

/// What a card in the slice asks the slice to answer.
#[derive(Clone, Copy)]
enum Need {
    Ward(Element),
    Poison(Element),
}

impl Need {
    fn of(def: &CardDef) -> Option<Need> {
        ward_element(def)
            .map(Need::Ward)
            .or_else(|| poison_element(def).map(Need::Poison))
    }

    fn met_by(self, def: &CardDef) -> bool {
        match self {
            Need::Ward(ward) => answers(def, ward),
            Need::Poison(poison) => cures(def, poison),
        }
    }
}

/// Element of the ward a card raises, if it raises one.
pub fn ward_element(def: &CardDef) -> Option<Element> {
    match def.effect {
        Effect::Ward => def.element,
        Effect::BodyLegion => Some(Element::Metal),
        _ => None,
    }
}

/// Element of the poison a card lays, if it lays one.
pub fn poison_element(def: &CardDef) -> Option<Element> {
    match def.effect {
        Effect::Poison(_) | Effect::Trap(TrapEffect::Poison(_)) => def.element,
        _ => None,
    }
}

fn answers(def: &CardDef, ward: Element) -> bool {
    def.effect.is_harmful() && def.element == Some(ward.quenched_by())
}

/// A heal of the element that quenches the poison takes it off.
pub fn cures(def: &CardDef, poison: Element) -> bool {
    def.effect.heals() && def.element == Some(poison.quenched_by())
}

/// The pool's card of this name.
pub fn def_named(name: &str) -> Option<DefId> {
    POOL.iter()
        .position(|d| d.name == name)
        .map(|i| DefId(i as u16))
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
    fn slices_always_cure_their_poisons() {
        for def in POOL {
            if let Some(poison) = poison_element(def) {
                assert!(
                    POOL.iter().any(|a| cures(a, poison)),
                    "{} has no cure",
                    def.name
                );
            }
        }
        for seed in 0..200 {
            let slice = match_slice(&mut Rng::new(seed));
            for id in &slice {
                if let Some(poison) = poison_element(id.def()) {
                    assert!(slice.iter().any(|a| cures(a.def(), poison)), "seed {seed}");
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
