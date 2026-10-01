//! Great Deeds (docs/design.md §21.7): how a match is won.
//!
//! At the start everyone is offered three deeds and picks one; all are open
//! to the table. A deed builds or changes the world and needs mechanics a
//! new world does not have, so whoever wants it must bring them in. When
//! every step of a deed holds, its eve begins and the table hears of it;
//! the deed is done at the next dusk if everything still holds then. The
//! others have that day to break it.
//!
//! A deed is a list of checks with a number to reach, so the client can
//! show everyone's progress without knowing the rules behind it.

use hexx::Hex;
use serde::{Deserialize, Serialize};

use super::{Event, Game, PlayerId, RuleError};
use crate::board::Terrain;
use crate::features::Feature;
use crate::gods::God;
use crate::rng::Rng;

/// A late-game goal that makes the world (§21.7).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum GreatDeed {
    /// A grove grown from a champion's body, ringed by woods, with beasts
    /// near, standing through two dusks.
    WorldTree,
    /// Land of seven hexes or more cut off from the Table by the mist, with a
    /// settlement of yours on it, and you there.
    Island,
    /// Seven settlements side by side, all yours, with a tavern, a forge and
    /// a shrine among them.
    City,
    /// Two gods of a quenching pair both in their light, and a shrine of
    /// both of yours, two dusks running.
    Reconciliation,
    /// Five undead of your legion following you.
    Legion,
    /// A river of eight hexes and more, rising by the mountains and running
    /// out to the rim of the world.
    River,
    /// The Table and the six hexes round it under a lake.
    FloodedTable,
    /// Woods of six hexes a river runs through, with piranhas in it.
    Amazon,
    /// One network of roads joining all five temples and the Table.
    Roads,
    /// A fire of yours has passed through four regions, and burns still.
    GreatFire,
    /// Three fields by your settlements, and a feast of yours with two
    /// guests or more.
    Feast,
    /// A fair of yours with goods of all five lands sold at it.
    FairOfFive,
    /// Three fairs of yours eaten by the dead.
    DeadFeast,
    /// A graveyard of three hexes with five buried in it, and no undead in
    /// Zaga's land at dusk.
    Necropolis,
    /// A plague pit of yours filled with five bodies and settled.
    PlaguePit,
    /// A monster out of your circle's gate, felled by your hand.
    Summoning,
    /// A dragon hatched and following you.
    Dragon,
    /// The stranger from beyond the mist led to the Table alive.
    Guest,
    /// A pen of yours by a shrine of yours with a beast of every element.
    Ark,
    /// A grove you woke walked to the Table and put down roots there.
    WalkingForest,
    /// Three duels won in an arena of yours, those never fought included.
    Arena,
    /// Three rivals in your debt at once.
    DebtBondage,
    /// A treasury of yours under ruins that nobody took for three dusks.
    Treasury,
    /// A night feast of yours with two rivals, the dead and the militia
    /// near, and no fight until dawn.
    DeadBall,
    /// Marriages of your making binding three lands into one house.
    TripleUnion,
    /// Crowned over three vassals, then their oaths broken and three of
    /// them feuding.
    FallenEmpire,
    /// A whole region of another god gone into the mist, but its temple and
    /// the champions' homes, and fifteen hexes of it at least.
    DissolvedLand,
}

impl GreatDeed {
    pub const ALL: [GreatDeed; 27] = [
        GreatDeed::WorldTree,
        GreatDeed::Island,
        GreatDeed::DissolvedLand,
        GreatDeed::City,
        GreatDeed::Reconciliation,
        GreatDeed::Legion,
        GreatDeed::River,
        GreatDeed::FloodedTable,
        GreatDeed::Amazon,
        GreatDeed::Roads,
        GreatDeed::GreatFire,
        GreatDeed::Feast,
        GreatDeed::FairOfFive,
        GreatDeed::DeadFeast,
        GreatDeed::TripleUnion,
        GreatDeed::FallenEmpire,
        GreatDeed::Necropolis,
        GreatDeed::PlaguePit,
        GreatDeed::Summoning,
        GreatDeed::Dragon,
        GreatDeed::Guest,
        GreatDeed::Ark,
        GreatDeed::WalkingForest,
        GreatDeed::Arena,
        GreatDeed::DebtBondage,
        GreatDeed::DeadBall,
        GreatDeed::Treasury,
    ];

    /// The god whose deed it is: its card's colour, its voice.
    pub const fn patron(self) -> God {
        match self {
            GreatDeed::WorldTree
            | GreatDeed::Amazon
            | GreatDeed::TripleUnion
            | GreatDeed::Ark
            | GreatDeed::WalkingForest => God::Bhava,
            GreatDeed::Island
            | GreatDeed::DissolvedLand
            | GreatDeed::River
            | GreatDeed::FloodedTable
            | GreatDeed::Guest
            | GreatDeed::DeadBall => God::Maya,
            GreatDeed::City
            | GreatDeed::GreatFire
            | GreatDeed::Feast
            | GreatDeed::FairOfFive
            | GreatDeed::DeadFeast => God::Trishna,
            GreatDeed::Roads
            | GreatDeed::FallenEmpire
            | GreatDeed::Summoning
            | GreatDeed::Arena
            | GreatDeed::DebtBondage => God::Ahamar,
            GreatDeed::Reconciliation
            | GreatDeed::Legion
            | GreatDeed::Necropolis
            | GreatDeed::PlaguePit
            | GreatDeed::Dragon
            | GreatDeed::Treasury => God::Zaga,
        }
    }

    /// What the world must have for it: on the card, so it says what to
    /// bring in.
    pub const fn needs(self) -> &'static [Feature] {
        match self {
            GreatDeed::WorldTree => &[Feature::Bodies, Feature::Groves, Feature::Beasts],
            GreatDeed::Island => &[Feature::Settlements],
            GreatDeed::DissolvedLand => &[],
            GreatDeed::City => &[Feature::Settlements, Feature::Buildings, Feature::City],
            GreatDeed::Reconciliation => &[Feature::Settlements, Feature::Buildings],
            GreatDeed::River => &[Feature::Rivers],
            GreatDeed::Roads => &[Feature::Settlements, Feature::Roads],
            GreatDeed::GreatFire => &[Feature::Fires],
            GreatDeed::Necropolis => &[
                Feature::Bodies,
                Feature::Undead,
                Feature::Cargo,
                Feature::Burial,
            ],
            GreatDeed::PlaguePit => &[Feature::Bodies, Feature::Cargo, Feature::Burial],
            GreatDeed::Summoning => &[
                Feature::Bodies,
                Feature::Cargo,
                Feature::Ritual,
                Feature::Monsters,
            ],
            GreatDeed::Dragon => &[
                Feature::Trials,
                Feature::Fires,
                Feature::Cargo,
                Feature::Companions,
                Feature::Dragons,
            ],
            GreatDeed::Guest => &[Feature::Companions, Feature::Guests],
            GreatDeed::Arena => &[Feature::Settlements, Feature::Buildings, Feature::Arena],
            GreatDeed::DebtBondage => &[Feature::Debts],
            GreatDeed::Treasury => &[
                Feature::Settlements,
                Feature::Undead,
                Feature::Ruins,
                Feature::Underworld,
            ],
            GreatDeed::DeadBall => &[
                Feature::Bodies,
                Feature::Undead,
                Feature::Settlements,
                Feature::Militia,
                Feature::Cargo,
                Feature::Fields,
            ],
            GreatDeed::WalkingForest => &[Feature::Bodies, Feature::Groves, Feature::WalkingGroves],
            GreatDeed::Ark => &[
                Feature::Beasts,
                Feature::Companions,
                Feature::Wilds,
                Feature::Settlements,
                Feature::Buildings,
                Feature::Pens,
            ],
            GreatDeed::TripleUnion | GreatDeed::FallenEmpire => {
                &[Feature::Settlements, Feature::Rulers]
            }
            GreatDeed::Feast => &[Feature::Settlements, Feature::Cargo, Feature::Fields],
            GreatDeed::FairOfFive => &[
                Feature::Settlements,
                Feature::Cargo,
                Feature::Goods,
                Feature::Fairs,
            ],
            GreatDeed::DeadFeast => &[
                Feature::Settlements,
                Feature::Cargo,
                Feature::Goods,
                Feature::Fairs,
                Feature::Bodies,
                Feature::Undead,
            ],
            GreatDeed::FloodedTable => &[Feature::Rivers, Feature::Lakes],
            GreatDeed::Amazon => &[Feature::Beasts, Feature::Rivers, Feature::Piranhas],
            GreatDeed::Legion => &[
                Feature::Bodies,
                Feature::Undead,
                Feature::Companions,
                Feature::Legion,
            ],
        }
    }
}

/// What a check measures; the client names and draws it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CheckKind {
    /// A grove grown from a champion's body stands.
    HeroGrove,
    /// Woods and groves round it, of six.
    WoodsAround,
    /// A beast within two hexes of it.
    BeastsNear,
    /// Dusks in a row it has stood so.
    Dusks,
    /// Hexes of the land cut off from the Table that you stand on.
    IslandSize,
    /// A settlement of yours on it.
    IslandSettled,
    /// Hexes of another god's region gone into the mist.
    RegionInMist,
    /// Settlements side by side in a city all yours.
    CitySize,
    /// Of a tavern, a forge and a shrine, how many stand in it.
    CityHas,
    /// Gods of the pair in their light.
    PairLight,
    /// A shrine of both, yours.
    SharedShrine,
    /// Undead of your legion following you.
    LegionSize,
    /// Temples on the register's roads.
    TemplesLinked,
    /// Regions a fire of yours has burnt through.
    RegionsBurnt,
    /// A fire of yours burns still.
    FireBurning,
    /// Fields beside your settlements.
    FieldsOwned,
    /// A feast of yours with guests enough.
    FeastHeld,
    /// Kinds of goods sold at a fair of yours.
    FairGoods,
    /// Fairs of yours the dead have eaten.
    DeadFeasts,
    /// Graveyard hexes side by side.
    NecropolisSize,
    /// Bodies buried in it.
    NecropolisBodies,
    /// No undead in Zaga's land.
    ZagaQuiet,
    /// A plague pit of yours settled.
    PitSettled,
    /// Your own monster felled by you.
    Summoned,
    /// A dragon follows you.
    DragonFollows,
    /// The guest led home.
    GuestHome,
    /// A grove of yours rooted by the Table.
    GroveRooted,
    /// Duels won in your arena.
    ArenaWins,
    /// Rivals in your debt.
    Debtors,
    /// A ball of the dead kept till dawn.
    BallKept,
    /// Dusks a treasury of yours has held.
    TreasuryHeld,
    /// Elements of the beasts in a pen of yours.
    PenElements,
    /// A shrine of yours in its city.
    PenByShrine,
    /// Lands bound by your marriages.
    UnionLands,
    /// You have been crowned.
    Crowned,
    /// Your former vassals feuding.
    Feuding,
    /// Hexes of the longest river.
    RiverLength,
    /// It rises by the mountains.
    RiverSource,
    /// It runs out to the rim.
    RiverMouth,
    /// Of the Table and the six round it, under water.
    TableFlooded,
    /// Woods a river runs through.
    JungleWoods,
    /// A river runs through them.
    JungleRiver,
    /// Piranhas in the world's rivers.
    Piranhas,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Check {
    pub kind: CheckKind,
    pub have: u16,
    pub need: u16,
}

impl Check {
    pub fn met(&self) -> bool {
        self.have >= self.need
    }
}

/// Settlements side by side a City needs.
pub const CITY: usize = 7;
/// Dusks before any deed may begin its eve: Great Deeds are the late game.
pub const EARLIEST_EVE: u32 = 10;
/// Undead in a legion for the Legion.
pub const LEGION: usize = 5;
/// Hexes of a river for the River.
pub const RIVER: usize = 9;
/// Woods round a river for the Amazon.
pub const JUNGLE: usize = 10;
/// River hexes in the woods for the Amazon.
pub const JUNGLE_RIVER: usize = 3;

/// Hexes an Island needs.
pub const ISLAND: usize = 7;
/// Hexes of a region the mist must take at least: all of it, and a small
/// one must first grow.
pub const DISSOLVED: usize = 15;

/// Deeds offered to each player to pick from.
pub const OFFERED: usize = 3;

/// Threat a refused wish costs the Crown: turning the table down is loud.
pub const REFUSAL_THREAT: i8 = 2;

/// Per-player counters the checks need.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Progress {
    /// Dusks the Crown was theirs in a row (the storyteller's boredom).
    pub crown_streak: u8,
    /// Dusks in a row their deed has held but for its dusks (a World Tree
    /// standing, a pair at peace, a legion together).
    pub held_dusks: u8,
}

/// Three deeds for each player: of different patrons where it can, and none
/// offered twice at the table while the pool lasts.
pub fn deal(rng: &mut Rng, players: usize) -> Vec<Vec<GreatDeed>> {
    let mut pool = GreatDeed::ALL.to_vec();
    rng.shuffle(&mut pool);
    let mut dealt: Vec<GreatDeed> = Vec::new();
    (0..players)
        .map(|_| {
            let mut hand: Vec<GreatDeed> = Vec::new();
            // Fresh ones of new patrons first, then fresh ones, then any.
            type Pass<'a> = &'a dyn Fn(&GreatDeed, &[GreatDeed]) -> bool;
            let passes: [Pass; 3] = [
                &|d, hand| !dealt.contains(d) && !hand.iter().any(|h| h.patron() == d.patron()),
                &|d, _| !dealt.contains(d),
                &|_, _| true,
            ];
            for pass in passes {
                for &d in &pool {
                    if hand.len() < OFFERED && !hand.contains(&d) && pass(&d, &hand) {
                        hand.push(d);
                    }
                }
            }
            dealt.extend(hand.iter().copied());
            hand
        })
        .collect()
}

impl Game {
    /// The deeds `player` was offered to pick from.
    pub fn offers(&self, player: PlayerId) -> &[GreatDeed] {
        self.offers
            .get(player.0 as usize)
            .map_or(&[][..], Vec::as_slice)
    }

    /// The deed `player` chose, once they have.
    pub fn deed(&self, player: PlayerId) -> Option<GreatDeed> {
        self.chosen.get(player.0 as usize).copied().flatten()
    }

    /// `player`'s deed holds and waits for dusk to be done.
    pub fn on_eve(&self, player: PlayerId) -> bool {
        self.eves
            .get(player.0 as usize)
            .is_some_and(Option::is_some)
    }

    /// The winner and their deed, once the match is over.
    pub fn winner(&self) -> Option<(PlayerId, GreatDeed)> {
        self.winner
    }

    /// Those who still have to pick their deed.
    pub fn choosing(&self) -> Vec<PlayerId> {
        self.order
            .iter()
            .copied()
            .filter(|&p| self.deed(p).is_none() && !self.offers(p).is_empty())
            .collect()
    }

    /// `player` picks the deed of the match, one of their offers.
    pub(super) fn choose_deed(
        &mut self,
        player: PlayerId,
        deed: GreatDeed,
        events: &mut Vec<Event>,
    ) -> Result<(), RuleError> {
        if self.deed(player).is_some() || !self.offers(player).contains(&deed) {
            return Err(RuleError::InvalidDeed);
        }
        self.chosen[player.0 as usize] = Some(deed);
        // What the world already holds of it is no step of theirs.
        let near = self.nearness(player);
        if let Some(m) = self.path_marks.get_mut(player.0 as usize) {
            m.0 = near;
        }
        events.push(Event::DeedChosen { player, deed });
        // The walking grove needs its card in the deck (§21.8).
        if deed == GreatDeed::WalkingForest
            && self.has(Feature::WalkingGroves)
            && !self
                .slice
                .iter()
                .any(|d| d.def().effect == crate::cards::Effect::Ent)
        {
            self.deal_in(Feature::WalkingGroves, events);
        }
        Ok(())
    }

    /// Where `player` stands on `deed`.
    pub fn checks(&self, player: PlayerId, deed: GreatDeed) -> Vec<Check> {
        let check = |kind, have: usize, need: usize| Check {
            kind,
            have: have.min(u16::MAX as usize) as u16,
            need: need as u16,
        };
        match deed {
            GreatDeed::WorldTree => {
                let (grove, woods, beasts) = self.best_tree();
                vec![
                    check(CheckKind::HeroGrove, usize::from(grove.is_some()), 1),
                    check(CheckKind::WoodsAround, woods, 6),
                    check(CheckKind::BeastsNear, usize::from(beasts), 1),
                    check(
                        CheckKind::Dusks,
                        usize::from(self.progress[player.0 as usize].held_dusks),
                        2,
                    ),
                ]
            }
            GreatDeed::Island => {
                let island = self.island_of(player);
                let settled = island.iter().any(|&h| {
                    self.owner(h) == Some(player)
                        && self
                            .board
                            .tile(h)
                            .is_some_and(|t| t.terrain == Terrain::Settlement)
                });
                vec![
                    check(CheckKind::IslandSize, island.len(), ISLAND),
                    check(CheckKind::IslandSettled, usize::from(settled), 1),
                ]
            }
            GreatDeed::DissolvedLand => {
                // A land worth the name: a small region must grow before it goes.
                let (gone, all) = self.dissolved_for(player);
                vec![check(CheckKind::RegionInMist, gone, all.max(DISSOLVED))]
            }
            GreatDeed::City => {
                let (size, has) = self.best_city(player);
                vec![
                    check(CheckKind::CitySize, size, CITY),
                    check(CheckKind::CityHas, has, 3),
                ]
            }
            GreatDeed::Roads => vec![check(CheckKind::TemplesLinked, self.temples_linked(), 5)],
            GreatDeed::Necropolis => {
                let (size, buried) = self.best_necropolis();
                vec![
                    check(CheckKind::NecropolisSize, size, super::NECROPOLIS),
                    check(
                        CheckKind::NecropolisBodies,
                        buried,
                        super::NECROPOLIS_BODIES,
                    ),
                    check(CheckKind::ZagaQuiet, usize::from(self.zaga_land_quiet()), 1),
                ]
            }
            GreatDeed::Summoning => vec![check(
                CheckKind::Summoned,
                usize::from(self.summoned(player)),
                1,
            )],
            GreatDeed::Dragon => vec![check(
                CheckKind::DragonFollows,
                usize::from(self.companions(player).contains(&super::Companion::Dragon)),
                1,
            )],
            GreatDeed::Arena => vec![check(
                CheckKind::ArenaWins,
                self.arena_wins(player),
                super::ARENA_WINS,
            )],
            GreatDeed::Treasury => vec![check(
                CheckKind::TreasuryHeld,
                self.treasury_held(player),
                usize::from(super::TREASURY_DUSKS),
            )],
            GreatDeed::DeadBall => vec![check(
                CheckKind::BallKept,
                usize::from(self.kept_ball(player)),
                1,
            )],
            GreatDeed::DebtBondage => vec![check(
                CheckKind::Debtors,
                self.debtors(player),
                super::DEBTORS,
            )],
            GreatDeed::WalkingForest => vec![check(
                CheckKind::GroveRooted,
                usize::from(self.rooted(player)),
                1,
            )],
            GreatDeed::Ark => {
                let (elements, shrine) = self.best_ark(player);
                vec![
                    check(CheckKind::PenElements, elements, super::ARK),
                    check(CheckKind::PenByShrine, usize::from(shrine), 1),
                ]
            }
            GreatDeed::Guest => vec![check(
                CheckKind::GuestHome,
                usize::from(self.guest_home(player)),
                1,
            )],
            GreatDeed::PlaguePit => vec![check(
                CheckKind::PitSettled,
                usize::from(self.settled_pit(player)),
                1,
            )],
            GreatDeed::TripleUnion => vec![check(
                CheckKind::UnionLands,
                self.union_lands(player),
                super::UNION_LANDS,
            )],
            GreatDeed::FallenEmpire => vec![
                check(
                    CheckKind::Crowned,
                    usize::from(self.crowned[player.0 as usize]),
                    1,
                ),
                check(CheckKind::Feuding, self.feuding(player), super::FEUDING),
            ],
            GreatDeed::FairOfFive => {
                let best = self
                    .fairs()
                    .filter(|(_, f)| f.host == player)
                    .map(|(_, f)| f.kinds())
                    .max()
                    .unwrap_or(0);
                vec![check(CheckKind::FairGoods, best, 5)]
            }
            GreatDeed::DeadFeast => vec![check(
                CheckKind::DeadFeasts,
                self.dead_feasts(player),
                super::DEAD_FEASTS,
            )],
            GreatDeed::Feast => vec![
                check(
                    CheckKind::FieldsOwned,
                    self.fields_of(player),
                    super::FEAST_FIELDS,
                ),
                check(
                    CheckKind::FeastHeld,
                    usize::from(self.feasted[player.0 as usize]),
                    1,
                ),
            ],
            GreatDeed::GreatFire => vec![
                check(
                    CheckKind::RegionsBurnt,
                    self.regions_burnt(player),
                    super::GREAT_FIRE,
                ),
                check(
                    CheckKind::FireBurning,
                    usize::from(self.fires().any(|(_, f)| f.by == Some(player))),
                    1,
                ),
            ],
            GreatDeed::River => {
                let (len, source, mouth) = self.best_river();
                vec![
                    check(CheckKind::RiverLength, len, RIVER),
                    check(CheckKind::RiverSource, usize::from(source), 1),
                    check(CheckKind::RiverMouth, usize::from(mouth), 1),
                    check(
                        CheckKind::Dusks,
                        usize::from(self.progress[player.0 as usize].held_dusks),
                        2,
                    ),
                ]
            }
            GreatDeed::FloodedTable => vec![
                check(CheckKind::TableFlooded, self.table_flooded(), 7),
                check(
                    CheckKind::Dusks,
                    usize::from(self.progress[player.0 as usize].held_dusks),
                    2,
                ),
            ],
            GreatDeed::Amazon => {
                let (woods, river) = self.best_jungle();
                vec![
                    check(CheckKind::JungleWoods, woods, JUNGLE),
                    check(CheckKind::JungleRiver, river, JUNGLE_RIVER),
                    check(
                        CheckKind::Piranhas,
                        usize::from(self.has(Feature::Piranhas)),
                        1,
                    ),
                    check(
                        CheckKind::Dusks,
                        usize::from(self.progress[player.0 as usize].held_dusks),
                        2,
                    ),
                ]
            }
            GreatDeed::Legion => {
                let legion = self
                    .companions(player)
                    .iter()
                    .filter(|&&c| c == super::Companion::Undead)
                    .count();
                vec![
                    check(CheckKind::LegionSize, legion, LEGION),
                    check(
                        CheckKind::Dusks,
                        usize::from(self.progress[player.0 as usize].held_dusks),
                        2,
                    ),
                ]
            }
            GreatDeed::Reconciliation => {
                let (light, shrine) = self.best_peace(player);
                vec![
                    check(CheckKind::PairLight, light, 2),
                    check(CheckKind::SharedShrine, usize::from(shrine), 1),
                    check(
                        CheckKind::Dusks,
                        usize::from(self.progress[player.0 as usize].held_dusks),
                        2,
                    ),
                ]
            }
        }
    }

    /// The grove grown from a champion that is nearest a World Tree.
    pub fn hero_grove(&self) -> Option<Hex> {
        self.best_tree().0
    }

    /// The best grove grown from a champion: woods round it, a beast near.
    fn best_tree(&self) -> (Option<Hex>, usize, bool) {
        self.hero_groves
            .iter()
            .map(|&(x, y)| Hex::new(x, y))
            .filter(|&h| {
                self.board
                    .tile(h)
                    .is_some_and(|t| t.terrain == Terrain::Grove)
            })
            .map(|h| {
                let woods = h
                    .all_neighbors()
                    .iter()
                    .filter(|&&n| {
                        self.board
                            .tile(n)
                            .is_some_and(|t| matches!(t.terrain, Terrain::Forest | Terrain::Grove))
                    })
                    .count();
                let beasts = self
                    .mobs
                    .iter()
                    .any(|m| m.is_beast() && m.hex.unsigned_distance_to(h) <= 2);
                (Some(h), woods, beasts)
            })
            .max_by_key(|&(h, woods, beasts)| {
                (woods + 6 * usize::from(beasts), h.map(|h| (-h.x(), -h.y())))
            })
            .unwrap_or((None, 0, false))
    }

    /// The land `player` stands on, if the mist cuts it off from the Table.
    fn island_of(&self, player: PlayerId) -> Vec<Hex> {
        let start = self.hex_of(player);
        let mut seen = vec![start];
        let mut i = 0;
        while i < seen.len() {
            for n in seen[i].all_neighbors() {
                if self.board.contains(n) && !seen.contains(&n) {
                    seen.push(n);
                }
            }
            i += 1;
        }
        if seen.contains(&Hex::ZERO) {
            Vec::new()
        } else {
            seen
        }
    }

    /// Of another god's region, the one most gone: hexes in the mist, and
    /// all that can go (its temple and the champions' homes stay).
    fn dissolved_for(&self, player: PlayerId) -> (usize, usize) {
        let (_, gone, all) = self.dissolving(player);
        (gone, all)
    }

    /// The land of the region `player` dissolves nearest them, still out of
    /// the mist.
    pub fn left_to_dissolve(&self, player: PlayerId) -> Option<Hex> {
        let (god, ..) = self.dissolving(player);
        let me = self.hex_of(player);
        self.board
            .land()
            .filter(|(h, t)| {
                t.region == Some(god)
                    && *h != self.board.temple_of(god)
                    && !God::ALL.iter().any(|&o| self.board.start_of(o) == *h)
            })
            .map(|(h, _)| h)
            .min_by_key(|h| (h.unsigned_distance_to(me), h.x(), h.y()))
    }

    /// The region `player` is dissolving best (another god's): its god, hexes
    /// in the mist, and all that can go.
    pub fn dissolving(&self, player: PlayerId) -> (God, usize, usize) {
        let own = self.champions[player.0 as usize].god;
        God::ALL
            .into_iter()
            .filter(|&g| g != own)
            .map(|g| {
                let mut gone = 0;
                let mut all = 0;
                for (h, t) in self.board.tiles() {
                    let home = God::ALL.iter().any(|&o| self.board.start_of(o) == h);
                    if t.region != Some(g) || h == self.board.temple_of(g) || home {
                        continue;
                    }
                    all += 1;
                    if t.terrain == Terrain::Mist {
                        gone += 1;
                    }
                }
                (g, gone, all)
            })
            .max_by_key(|&(g, gone, all)| {
                (gone * 1000 / all.max(1), gone, std::cmp::Reverse(g.index()))
            })
            .expect("four other gods")
    }

    fn deed_holds(&self, player: PlayerId) -> bool {
        self.deed(player)
            .is_some_and(|d| self.checks(player, d).iter().all(Check::met))
    }

    /// Dusk: the Crown's streak.
    pub(super) fn count_crown(&mut self) {
        for p in self.players().collect::<Vec<_>>() {
            let crowned = self.dominant == Some(p);
            let progress = &mut self.progress[p.0 as usize];
            progress.crown_streak = if crowned {
                progress.crown_streak.saturating_add(1)
            } else {
                0
            };
        }
    }

    /// Of `player`'s cities (every quarter theirs), the one furthest on: its
    /// size, and how many of a tavern, a forge and a shrine stand in it.
    fn best_city(&self, player: PlayerId) -> (usize, usize) {
        let mut seen: Vec<Hex> = Vec::new();
        let mut best = (0, 0);
        for (hex, owner) in self.claims() {
            if owner != player || seen.contains(&hex) {
                continue;
            }
            let city = self.city_of(hex);
            seen.extend(city.iter().copied());
            if city.is_empty() || city.iter().any(|&h| self.owner(h) != Some(player)) {
                continue;
            }
            let kinds = city.iter().filter_map(|&h| self.building(h));
            let mut has = [false; 3];
            for b in kinds {
                match b {
                    super::Building::Tavern => has[0] = true,
                    super::Building::Forge => has[1] = true,
                    super::Building::Shrine(_) => has[2] = true,
                    super::Building::Wall | super::Building::Pen | super::Building::Arena => {}
                }
            }
            let found = (city.len(), has.iter().filter(|&&x| x).count());
            best = best.max(found);
        }
        best
    }

    /// Of the quenching pairs, the one nearest peace for `player`: gods of
    /// it in their light, and whether they hold a shrine of both.
    fn best_peace(&self, player: PlayerId) -> (usize, bool) {
        God::ALL
            .into_iter()
            .map(|a| {
                let b = God::from_index(a.index() + 2);
                let light = usize::from(self.stage(a) == 0) + usize::from(self.stage(b) == 0);
                let shrine = self.buildings().any(|(h, building)| {
                    matches!(building, super::Building::Shrine(gods)
                        if gods.contains(&a) && gods.contains(&b))
                        && self.owner(h) == Some(player)
                });
                (light, shrine)
            })
            .max_by_key(|&(light, shrine)| (light + 2 * usize::from(shrine), shrine))
            .unwrap_or((0, false))
    }

    /// Dusk: a World Tree that stood whole counts another dusk; so does a
    /// pair kept in peace.
    fn count_trees(&mut self) {
        for p in self.players().collect::<Vec<_>>() {
            let Some(deed) = self.deed(p) else {
                continue;
            };
            let checks = self.checks(p, deed);
            if !checks.iter().any(|c| c.kind == CheckKind::Dusks) {
                continue;
            }
            let holding = checks
                .iter()
                .filter(|c| c.kind != CheckKind::Dusks)
                .all(Check::met);
            let dusks = &mut self.progress[p.0 as usize].held_dusks;
            *dusks = if holding { dusks.saturating_add(1) } else { 0 };
        }
    }

    /// After every intent: a deed that holds begins its eve, one that no
    /// longer holds loses it.
    pub(super) fn check_victory(&mut self, events: &mut Vec<Event>) {
        if self.winner.is_some() {
            return;
        }
        for p in self.order.clone() {
            let Some(deed) = self.deed(p) else {
                continue;
            };
            // A deed is the late game: no eve before `EARLIEST_EVE` dusks.
            let holds = self.deed_holds(p) && self.dusks >= EARLIEST_EVE;
            let eve = &mut self.eves[p.0 as usize];
            match (holds, *eve) {
                (true, None) => {
                    *eve = Some(self.dusks);
                    events.push(Event::DeedEve { player: p, deed });
                }
                (false, Some(_)) => {
                    *eve = None;
                    events.push(Event::EveBroken { player: p, deed });
                }
                _ => {}
            }
        }
    }

    /// Dusk falls: trees count their dusk, and a deed whose eve began before
    /// this dusk and still holds is done. Initiative settles a tie.
    pub(super) fn dusk_of_deeds(&mut self, events: &mut Vec<Event>) {
        self.dusks += 1;
        self.count_trees();
        self.check_victory(events);
        for p in self.order.clone() {
            let (Some(deed), Some(since)) = (self.deed(p), self.eves[p.0 as usize]) else {
                continue;
            };
            if since < self.dusks && self.deed_holds(p) {
                self.winner = Some((p, deed));
                events.push(Event::Victory { player: p, deed });
                return;
            }
        }
    }
}

impl Game {
    /// After `player`'s intent: if it took their deed on, a step of the
    /// path is done, +1 Style once a round; a step back is remembered too.
    pub(super) fn note_step(&mut self, player: PlayerId, events: &mut Vec<Event>) {
        if self.deed(player).is_none() || self.winner.is_some() {
            return;
        }
        let near = self.nearness(player);
        let round = self.round;
        let Some(&(before, paid)) = self.path_marks.get(player.0 as usize) else {
            return;
        };
        self.path_marks[player.0 as usize].0 = near;
        if near > before && paid != round {
            self.path_marks[player.0 as usize].1 = round;
            events.push(Event::StepDone { player });
            self.add_style(player, 1, super::StyleReason::Path, events);
        }
    }
}
