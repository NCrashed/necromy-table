//! Trials on hexes (docs/design.md §20.2).
//!
//! A god sets a trial on a hex of its land. Whoever steps onto it stops
//! there and throws their dice against it: the trial wants faces of its
//! god's kind (Bhava strikes, Trishna suns, Zaga shields, Ahamar the
//! Element, Maya moons), as many as the god's stage asks. Sun counts only by
//! day and Moon only by night, as in battle; the Element counts for every
//! trial. Cards burn for faces first, as in battle (§12.1).
//!
//! Passing takes the trial off the board and gives its boon; failing costs
//! what the god takes from the weak, and the trial stays for the others.
//! Each champion tries a trial once. The storyteller sets them at dusk
//! (never the board generator), so they are not where a veteran expects.

use hexx::Hex;
use necromy_dice::Face;
use serde::{Deserialize, Serialize};

use super::style::StyleReason;
use super::{Event, Fighter, Game, PlayerId, WindowKind};
use crate::board::Terrain;
use crate::cards::CardId;
use crate::gods::{Element, God};

/// Trials the gods keep on the board at once.
pub const TRIALS_ON_BOARD: usize = 2;
/// Rounds a trial waits before it fades.
pub const TRIAL_ROUNDS: u32 = 6;
/// A new trial stands at least this far from every champion.
const TRIAL_CLEARANCE: u32 = 2;
/// Separates trial throws from battle throws.
const TRIAL_STREAM: u64 = 0x0071_21a1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trial {
    pub id: u32,
    pub hex: Hex,
    /// The god of the land it stands in: sets the face and the cost.
    pub god: God,
    pub boon: Boon,
    /// Last round it can be tried in.
    pub deadline: u32,
    /// Champions who tried it already, and failed.
    pub tried: Vec<PlayerId>,
}

/// What passing a trial gives; its size follows the god's stage.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Boon {
    Style,
    /// Favour with the trial's god, as an offering.
    Favour,
    Cards,
    /// Health back, and the poison gone.
    Mending,
    /// An item from the loot deck (§20.3).
    Loot,
}

impl Boon {
    pub const ALL: [Boon; 5] = [
        Boon::Style,
        Boon::Favour,
        Boon::Cards,
        Boon::Mending,
        Boon::Loot,
    ];
}

/// The face a god's trials want: each god's nature on the die.
pub const fn trial_face(god: God) -> Face {
    match god {
        God::Bhava => Face::Strike,
        God::Trishna => Face::Sun,
        God::Zaga => Face::Shield,
        God::Ahamar => Face::Element,
        God::Maya => Face::Moon,
    }
}

impl Trial {
    pub const fn face(&self) -> Face {
        trial_face(self.god)
    }
}

impl Game {
    pub fn trials(&self) -> &[Trial] {
        &self.trials
    }

    pub fn trial_at(&self, hex: Hex) -> Option<&Trial> {
        self.trials.iter().find(|t| t.hex == hex)
    }

    /// The trial on `hex` that `player` has not tried yet.
    pub fn trial_for(&self, player: PlayerId, hex: Hex) -> Option<&Trial> {
        self.trial_at(hex).filter(|t| !t.tried.contains(&player))
    }

    /// Faces the trial asks for now: one more for each step its god has
    /// darkened. Ahamar's Element comes up rarely, so his asks one less.
    pub fn trial_need(&self, trial: &Trial) -> u8 {
        let need = self.stage(trial.god) + 1;
        if trial.face() == Face::Element {
            need.saturating_sub(1).max(1)
        } else {
            need
        }
    }

    /// Whether a face thrown now counts for the trial.
    pub fn trial_counts(&self, trial: &Trial, face: Face) -> bool {
        let day = self.time == super::TimeOfDay::Day;
        match face {
            Face::Element => true,
            Face::Sun => trial.face() == Face::Sun && day,
            Face::Moon => trial.face() == Face::Moon && !day,
            f => f == trial.face(),
        }
    }

    /// How much the boon gives: a darker god pays more.
    pub fn boon_amount(&self, trial: &Trial) -> u8 {
        let stage = self.stage(trial.god);
        match trial.boon {
            Boon::Favour => 2 + stage,
            _ => 1 + stage,
        }
    }

    /// The trial `player` is throwing against, if they are in one.
    pub fn trial_of(&self, player: PlayerId) -> Option<&Trial> {
        self.windows.iter().find_map(|w| match w.kind {
            WindowKind::Trial { player: p, hex } if p == player => self.trial_at(hex),
            _ => None,
        })
    }

    /// `player` walked onto a trial (`step` has brought them into view):
    /// their movement ends and they pick cards to burn before the throw.
    pub(super) fn begin_trial(&mut self, player: PlayerId, hex: Hex, events: &mut Vec<Event>) {
        if self.is_active(player) {
            self.turns[player.0 as usize].move_points = 0;
        }
        if let Some(t) = self.trials.iter_mut().find(|t| t.hex == hex) {
            t.tried.push(player);
        }
        events.push(Event::TrialBegun { player, hex });
        self.open_window(
            player,
            WindowKind::Trial { player, hex },
            vec![player],
            None,
            None,
            events,
        );
    }

    /// Burned faces, then the throw; then the boon or the god's price.
    pub(super) fn resolve_trial(
        &mut self,
        player: PlayerId,
        hex: Hex,
        burned: Vec<CardId>,
        events: &mut Vec<Event>,
    ) {
        let Some(trial) = self.trial_at(hex).cloned() else {
            // Burned cards go to the discard even if the trial is gone.
            self.discard.extend(burned);
            return;
        };
        let faces: Vec<Face> = burned.iter().map(|&c| self.def(c).burn_face()).collect();
        if !burned.is_empty() {
            events.push(Event::Burned {
                player,
                cards: burned.clone(),
                faces: faces.clone(),
            });
        }
        for &card in &burned {
            if let Some(e) = self.def(card).element {
                self.offer(Some(player), God::from_index(e.index()), 1, events);
            }
        }
        self.discard.extend(burned.iter().copied());
        let count = self.champions[player.0 as usize]
            .might
            .saturating_add(self.item_trial_dice(player))
            .saturating_sub(burned.len() as u8);
        self.trial_throws += 1;
        let label = [TRIAL_STREAM, self.trial_throws];
        let faces = self.roll_with(Fighter::Champion(player), &label, count, faces, events);
        let got = faces
            .iter()
            .filter(|&&f| self.trial_counts(&trial, f))
            .count() as u8;
        let need = self.trial_need(&trial);
        if got >= need {
            self.trials.retain(|t| t.id != trial.id);
            events.push(Event::TrialPassed {
                player,
                trial: trial.clone(),
                got,
                need,
            });
            self.grant_boon(player, &trial, events);
            // The god remembers who stood its test.
            self.offer(Some(player), trial.god, 1, events);
            self.story_trial(player, hex, events);
        } else {
            events.push(Event::TrialFailed {
                player,
                trial: trial.clone(),
                got,
                need,
            });
            self.exact_price(player, &trial, events);
        }
    }

    fn grant_boon(&mut self, player: PlayerId, trial: &Trial, events: &mut Vec<Event>) {
        let n = self.boon_amount(trial);
        match trial.boon {
            Boon::Style => self.add_style(player, i16::from(n), StyleReason::Trial, events),
            Boon::Favour => self.offer(Some(player), trial.god, n, events),
            Boon::Cards => self.draw(player, n as usize, events),
            Boon::Mending => {
                self.cure(player, super::Cure::Trial, events);
                self.heal(player, n, events);
            }
            Boon::Loot => self.gain_loot(player, events),
        }
    }

    /// Each god takes its own price from the one who failed it.
    fn exact_price(&mut self, player: PlayerId, trial: &Trial, events: &mut Vec<Event>) {
        let dark = self.stage(trial.god) >= 2;
        match trial.god {
            // Thorns and sap: the wood's poison.
            God::Bhava => self.poison(player, Element::Wood, 2 + u8::from(dark), events),
            // The feast burns.
            God::Trishna => self.damage(player, 1 + u8::from(dark), events),
            // The earth holds on: the trial already ended this turn's
            // walk, so it takes the next one.
            God::Zaga => {
                self.champ_mut(player).rooted = true;
                events.push(Event::Rooted { player });
            }
            // The registry strikes the name out.
            God::Ahamar => self.add_style(player, -1 - i16::from(dark), StyleReason::Trial, events),
            // The mist drinks the Spirit.
            God::Maya => {
                let champ = self.champ_mut(player);
                champ.spirit_points = 0;
                events.push(Event::SpiritChanged { player, spirit: 0 });
            }
        }
    }

    /// Dusk: trials past their day fade, and the gods top the board up.
    pub(super) fn trials_at_dusk(&mut self, events: &mut Vec<Event>) {
        let round = self.round;
        let faded: Vec<Hex> = self
            .trials
            .iter()
            .filter(|t| round >= t.deadline)
            .map(|t| t.hex)
            .collect();
        self.trials.retain(|t| round < t.deadline);
        for hex in faded {
            events.push(Event::TrialFaded { hex });
        }
        while self.trials.len() < TRIALS_ON_BOARD {
            let Some(hex) = self.trial_spot(None) else {
                break;
            };
            self.set_trial(hex, events);
        }
    }

    /// Sets a trial of the land's god on `hex`, with a boon of chance.
    pub(super) fn set_trial(&mut self, hex: Hex, events: &mut Vec<Event>) -> Option<God> {
        let god = self.board.tile(hex)?.region?;
        let boon = *self.rng.pick(&Boon::ALL).expect("four boons");
        self.next_trial += 1;
        let trial = Trial {
            id: self.next_trial,
            hex,
            god,
            boon,
            deadline: self.round + TRIAL_ROUNDS,
            tried: Vec::new(),
        };
        self.trials.push(trial.clone());
        events.push(Event::TrialSet { trial });
        Some(god)
    }

    /// A free hex of some god's land, away from everyone; `near` asks for
    /// one within a distance range of a hex. Stones are the gods' first
    /// choice (§20.2): standing stones are where a god tests people.
    pub(super) fn trial_spot(&mut self, near: Option<(Hex, u32, u32)>) -> Option<Hex> {
        let spots: Vec<(Hex, bool)> = self
            .board
            .tiles()
            .filter(|(h, t)| {
                t.region.is_some()
                    && !matches!(
                        t.terrain,
                        Terrain::Settlement | Terrain::Temple | Terrain::Table
                    )
                    && t.corpse.is_none()
                    && self.trial_at(*h).is_none()
                    && self.champion_at(*h).is_none()
                    && !self.guard_at(*h)
                    && !self.traps.iter().any(|tr| tr.hex == *h)
                    && self.players().all(|p| {
                        self.champions[p.0 as usize].hex.unsigned_distance_to(*h) >= TRIAL_CLEARANCE
                    })
                    && near.is_none_or(|(c, lo, hi)| {
                        let d = c.unsigned_distance_to(*h);
                        (lo..=hi).contains(&d)
                    })
            })
            .map(|(h, t)| (h, t.terrain == Terrain::Stones))
            .collect();
        let stones: Vec<Hex> = spots.iter().filter(|s| s.1).map(|s| s.0).collect();
        let all: Vec<Hex> = spots.iter().map(|s| s.0).collect();
        // Stones first, half the time, when there are any free.
        if !stones.is_empty() && self.rng.below(2) == 0 {
            return self.rng.pick(&stones).copied();
        }
        self.rng.pick(&all).copied()
    }
}
