//! The Dominant's wish (docs/design.md §7), offline.
//!
//! At dawn the Dominant asks one god for one of a few prepared wishes, or
//! refuses to wish at all. The god grades the wish by its own nature and by
//! how new it is (§7.4), grants it through its nature, never quite as asked
//! (§7.2), and the grade turns into Style. Crude wishes always get 0: they
//! come true, cut down, with a curse.
//!
//! This is the stand-in for the LLM: later a free-text wish is interpreted by
//! a model into the same closed set of effects, and the rules stay the same.

use hexx::Hex;
use serde::{Deserialize, Serialize};

use super::style::StyleReason;
use super::{Event, Game, PlayerId};
use crate::board::Terrain;
use crate::gods::God;

/// Prepared wishes: what the Dominant can ask for offline.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WishKind {
    /// "Give me strength": health, spirit, a ward.
    Strength,
    /// "Let my rival falter": hurt and hold a rival.
    Weaken,
    /// "Let the land answer me": the god's own ground around you.
    Land,
    /// "Let the dead serve me": bodies around you.
    Dead,
    /// "Quiet the noise around me": Threat falls.
    Peace,
    /// "Give me riches": crude.
    Fortune,
    /// "Give me victory": crude.
    Doom,
}

impl WishKind {
    pub const ALL: [WishKind; 7] = [
        WishKind::Strength,
        WishKind::Weaken,
        WishKind::Land,
        WishKind::Dead,
        WishKind::Peace,
        WishKind::Fortune,
        WishKind::Doom,
    ];

    /// Asks for the outcome itself instead of going through the world: always
    /// graded 0 (§7.4).
    pub const fn is_crude(self) -> bool {
        matches!(self, WishKind::Fortune | WishKind::Doom)
    }

    pub const fn needs_target(self) -> bool {
        matches!(self, WishKind::Weaken)
    }
}

/// How a god likes being asked for something: +1, 0 or −1 to the grade.
pub const fn taste_for(god: God, kind: WishKind) -> i8 {
    use WishKind::*;
    match (god, kind) {
        // Hunger loves strength and feasts of the dead; quiet bores it.
        (God::Trishna, Strength | Dead) => 1,
        (God::Trishna, Peace) => -1,
        // Order loves land and judgement; the dead are paperwork.
        (God::Ahamar, Land | Weaken) => 1,
        (God::Ahamar, Dead) => -1,
        // Dissolution loves letting go and loosening a rival's grip.
        (God::Maya, Peace | Weaken) => 1,
        (God::Maya, Strength) => -1,
        // Renunciation loves quiet and giving the dead rest; not strength.
        (God::Zaga, Peace | Dead) => 1,
        (God::Zaga, Strength) => -1,
        // Growth loves the land and strength; not harm.
        (God::Bhava, Land | Strength) => 1,
        (God::Bhava, Weaken) => -1,
        _ => 0,
    }
}

/// Ground each god raises when asked for land.
pub const fn god_terrain(god: God) -> Terrain {
    match god {
        God::Bhava => Terrain::Grove,
        God::Trishna => Terrain::Plains,
        God::Zaga => Terrain::Mountain,
        God::Ahamar => Terrain::Stones,
        God::Maya => Terrain::Swamp,
    }
}

impl Game {
    /// The Dominant owes a wish (or a refusal) before play goes on.
    pub fn wish_due(&self) -> Option<PlayerId> {
        self.wish_due
    }

    /// The grade `god` would give `kind` right now (§7.4). The same rule the
    /// god applies, so bots and the UI can reason about it.
    pub fn wish_grade(&self, god: God, kind: WishKind) -> u8 {
        if kind.is_crude() {
            return 0;
        }
        let novelty = if self.asked.contains(&(god, kind)) {
            -1
        } else {
            1
        };
        (1 + taste_for(god, kind) + novelty).clamp(0, 3) as u8
    }

    /// Curses on a champion: one god each, draining health every turn until
    /// lifted by a card of the element that quenches that god.
    pub fn curses(&self, player: PlayerId) -> &[God] {
        self.curses
            .get(player.0 as usize)
            .map_or(&[][..], Vec::as_slice)
    }

    pub(super) fn grant_wish(
        &mut self,
        player: PlayerId,
        god: God,
        kind: WishKind,
        target: Option<PlayerId>,
        events: &mut Vec<Event>,
    ) {
        let grade = self.wish_grade(god, kind);
        self.asked.push((god, kind));
        self.wish_due = None;
        events.push(Event::WishGranted {
            player,
            god,
            kind,
            target,
            grade,
        });
        // Asking is an offering too.
        self.offer(Some(player), god, 1, events);

        let power = grade + 1;
        let me = self.hex_of(player);
        match kind {
            WishKind::Strength => {
                self.heal(player, power, events);
                self.gain_spirit(player, power, events);
                self.raise_ward(player, god.element(), events);
            }
            WishKind::Weaken => {
                if let Some(t) = target {
                    // A god's hand passes any ward.
                    self.damage(t, power.saturating_sub(1).max(1), events);
                    self.root(t, events);
                }
            }
            WishKind::Land => {
                let terrain = god_terrain(god);
                let spots: Vec<Hex> = me
                    .all_neighbors()
                    .into_iter()
                    .chain(std::iter::once(me))
                    .filter(|&h| {
                        self.board.tile(h).is_some_and(|t| {
                            t.terrain.can_grow_grove() || t.terrain == Terrain::Mountain
                        }) && !self.guard_at(h)
                    })
                    .take(power as usize)
                    .collect();
                for hex in spots {
                    if let Some(tile) = self.board.tile_mut(hex) {
                        tile.terrain = terrain;
                    }
                    events.push(Event::TerrainChanged { hex, terrain });
                }
            }
            WishKind::Dead => {
                let free: Vec<Hex> = (1..=2)
                    .flat_map(|r| me.ring(r).collect::<Vec<_>>())
                    .filter(|&h| {
                        self.board.tile(h).is_some_and(|t| t.corpse.is_none())
                            && self.occupant(h).is_none()
                    })
                    .take(power as usize)
                    .collect();
                for hex in free {
                    if let Some(tile) = self.board.tile_mut(hex) {
                        tile.corpse = Some(crate::board::Corpse { age: 0 });
                    }
                    events.push(Event::CorpseAppeared { hex });
                }
            }
            WishKind::Peace => {
                self.add_threat(player, -(2 * power as i8), events);
            }
            WishKind::Fortune => {
                self.add_style(player, 2, StyleReason::Wish, events);
            }
            WishKind::Doom => {
                for p in self.players().filter(|&p| p != player).collect::<Vec<_>>() {
                    self.damage(p, 1, events);
                }
            }
        }

        self.twist(player, god, events);

        // The grade becomes Style; a wish without style also carries a curse.
        // Kept small: a wish is power already, and Style keeps the Crown.
        let style = match grade {
            3 => 1,
            1 | 2 => 0,
            _ => -1,
        };
        if style != 0 {
            self.add_style(player, style, StyleReason::Wish, events);
        }
        if grade == 0 {
            self.curses[player.0 as usize].push(god);
            events.push(Event::CurseLaid { player, god });
        }
    }

    /// Every god grants through its own nature (§7.2).
    fn twist(&mut self, player: PlayerId, god: God, events: &mut Vec<Event>) {
        match god {
            // The gift feeds the hunger.
            God::Trishna => self.offer(None, God::Trishna, 1, events),
            // Written down as a debt in the registry.
            God::Ahamar => self.add_threat(player, 1, events),
            // Not quite as you held it: a card dissolves into spirit.
            God::Maya => {
                let hand = &self.hands[player.0 as usize];
                if !hand.is_empty() {
                    let i = self.rng.below(hand.len() as u32) as usize;
                    let card = self.hands[player.0 as usize].remove(i);
                    self.discard.push(card);
                    events.push(Event::CardDissolved { player, card });
                    self.gain_spirit(player, 1, events);
                }
            }
            // The price of one wish is another.
            God::Zaga => {
                let champ = self.champ_mut(player);
                if champ.spirit_points > 0 {
                    champ.spirit_points -= 1;
                    let spirit = champ.spirit_points;
                    events.push(Event::SpiritChanged { player, spirit });
                }
            }
            // Growth nobody controls: a grove rises by a rival too.
            God::Bhava => {
                let rivals: Vec<PlayerId> = self.players().filter(|&p| p != player).collect();
                if let Some(&rival) = self.rng.pick(&rivals) {
                    let near = self.hex_of(rival).all_neighbors().into_iter().find(|&h| {
                        self.occupant(h).is_none()
                            && self.board.tile(h).is_some_and(|t| {
                                t.terrain.can_grow_grove() && t.terrain != Terrain::Grove
                            })
                    });
                    if let Some(hex) = near {
                        self.grow(hex, events);
                    }
                }
            }
        }
    }

    /// Cursed champions lose health as their turn starts.
    pub(super) fn bite_curses(&mut self, player: PlayerId, events: &mut Vec<Event>) {
        let gods = self.curses[player.0 as usize].clone();
        for god in gods {
            events.push(Event::CurseBit { player, god });
            self.damage(player, 1, events);
        }
    }

    /// A card of the element that quenches a cursing god lifts its curse.
    pub(super) fn lift_curse(
        &mut self,
        player: PlayerId,
        element: crate::gods::Element,
        events: &mut Vec<Event>,
    ) {
        let curses = &mut self.curses[player.0 as usize];
        if let Some(i) = curses
            .iter()
            .position(|g| g.element().quenched_by() == element)
        {
            let god = curses.remove(i);
            events.push(Event::CurseLifted { player, god });
        }
    }
}
