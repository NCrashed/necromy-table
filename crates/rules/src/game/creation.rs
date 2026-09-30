//! The world as players make it (docs/design.md §21): land rising at the
//! rim, land going into the mist and coming back out, new things on it, and
//! new rules coming into the world.
//!
//! A mechanic comes in only when the world has what it stands on
//! (`Feature::requires`), and at most one a dusk: a second one asked for the
//! same dusk the god sets aside for the next (§21.4). Its first thing
//! appears near whoever brought it: a settlement, bodies, a trial.

use hexx::Hex;

use super::{Event, Game, PlayerId, mobs};
use crate::board::{Corpse, Terrain};
use crate::features::{Feature, Need};
use crate::game::wish::{AWAKEN_COST, Act};
use crate::gods::God;

/// Land each god raises, its own kind first (§21.2).
pub const fn god_lands(god: God) -> &'static [Terrain] {
    match god {
        God::Bhava => &[Terrain::Forest, Terrain::Plains, Terrain::Swamp],
        God::Trishna => &[Terrain::Plains, Terrain::Forest],
        God::Zaga => &[Terrain::Mountain, Terrain::Plains, Terrain::Stones],
        God::Ahamar => &[Terrain::Plains, Terrain::Mountain],
        God::Maya => &[Terrain::Swamp, Terrain::Plains, Terrain::Forest],
    }
}

impl Game {
    /// New land on `hex`, off the board and next to it.
    pub(super) fn raise_land(
        &mut self,
        hex: Hex,
        terrain: Terrain,
        events: &mut Vec<Event>,
    ) -> bool {
        if !self.board.raise(hex, terrain) {
            return false;
        }
        events.push(Event::LandRaised { hex, terrain });
        true
    }

    /// The land on `hex` goes into the mist, with whatever lay on it: a
    /// body, traps, a trial, items, a claim, the militia there, the undead
    /// or a beast it swallows. Never where a champion stands, the royal
    /// guard marches or a temple is.
    pub(super) fn veil(&mut self, hex: Hex, events: &mut Vec<Event>) -> bool {
        if self.champion_at(hex).is_some() || self.guard_at(hex) || !self.board.veil(hex) {
            return false;
        }
        let key = (hex.x(), hex.y());
        let swallowed: Vec<u32> = self
            .mobs
            .iter()
            .filter(|m| m.hex == hex)
            .map(|m| m.id)
            .collect();
        for id in swallowed {
            self.mobs.retain(|m| m.id != id);
            events.push(Event::MobLeft { id });
        }
        self.militia.retain(|_, m| m.at != Some(hex));
        self.traps.retain(|t| t.hex != hex);
        self.trials.retain(|t| t.hex != hex);
        self.ground.retain(|(h, _)| *h != hex);
        self.loads.retain(|(h, _)| *h != hex);
        self.wash_road(hex);
        self.put_out(hex, events);
        self.graves.remove(&key);
        self.pits.remove(&key);
        self.buildings.remove(&key);
        self.claims.remove(&key);
        self.militia.remove(&key);
        self.ruins.remove(&key);
        events.push(Event::TerrainChanged {
            hex,
            terrain: Terrain::Mist,
        });
        true
    }

    /// The mist on `hex` lifts, and the land is as it was.
    pub(super) fn unveil(&mut self, hex: Hex, events: &mut Vec<Event>) -> bool {
        let Some(terrain) = self.board.unveil(hex) else {
            return false;
        };
        events.push(Event::TerrainChanged { hex, terrain });
        true
    }

    // ---- What may come in ----

    fn need_met(&self, need: Need) -> bool {
        match need {
            Need::Has(f) => self.has(f),
            Need::AnyOf(fs) => fs.iter().any(|&f| self.has(f)),
            Need::Land(kinds) => self.board.land().any(|(_, t)| kinds.contains(&t.terrain)),
        }
    }

    /// `feature` could come into the world now: it is not there yet, and
    /// everything it stands on is.
    pub fn can_awaken(&self, feature: Feature) -> bool {
        !self.has(feature) && feature.requires().iter().all(|&n| self.need_met(n))
    }

    /// Mechanics that could come in now, `god`'s own first.
    pub fn awakenable(&self, god: God) -> Vec<Feature> {
        let mut all: Vec<Feature> = Feature::ALL
            .into_iter()
            .filter(|&f| self.can_awaken(f))
            .collect();
        all.sort_by_key(|f| f.domain() != god);
        all
    }

    /// A mechanic has come in this dusk already: the next waits a day.
    pub fn awakened_tonight(&self) -> bool {
        self.awakened == Some(self.round)
    }

    /// The mechanic a thing asked for stands on, if the world lacks it:
    /// asking for the dead where there are no bodies brings bodies in.
    fn lacks_for(&self, act: Act) -> Option<Feature> {
        let needs = match act {
            Act::Dead => Feature::Bodies,
            Act::Settle => Feature::Settlements,
            Act::River => Feature::Rivers,
            Act::Flood => Feature::Lakes,
            Act::Road => Feature::Roads,
            Act::Fire => Feature::Fires,
            Act::Treasure => Feature::Loot,
            Act::Ordeal => Feature::Trials,
            _ => return None,
        };
        (!self.has(needs)).then_some(needs)
    }

    /// What `act` costs in this world (§21.9): a thing the world has no rule
    /// for yet brings the rule in, at an awakening's price.
    pub fn act_cost(&self, act: Act) -> u8 {
        match self.lacks_for(act) {
            Some(_) => act.cost().max(AWAKEN_COST),
            None => act.cost(),
        }
    }

    // ---- Bringing things in ----

    /// `feature` comes into the world, brought by `god` for `player` (a
    /// wish) or by the world (`None`); its first thing appears near `near`.
    /// If a mechanic came in this dusk already, the god sets it aside for
    /// the next one: false.
    pub(super) fn awaken(
        &mut self,
        player: Option<PlayerId>,
        god: God,
        feature: Feature,
        near: Hex,
        events: &mut Vec<Event>,
    ) -> bool {
        if !self.can_awaken(feature) {
            return false;
        }
        if self.awakened_tonight() {
            if let Some(p) = player {
                self.deferred.push((p, god, feature));
                events.push(Event::AwakeningDeferred {
                    player: p,
                    god,
                    feature,
                });
            }
            return false;
        }
        self.awakened = Some(self.round);
        self.world.add(feature);
        events.push(Event::WorldGrew {
            feature,
            god,
            player,
        });
        self.deal_in(feature, events);
        if let Some(p) = player {
            self.first(p, super::Novelty::Brought(feature), events);
        }
        self.first_of(feature, player, near, events);
        true
    }

    /// Cards of a mechanic just come in go into the deck (§21.2): a few of
    /// its own and whatever answers they need, two copies each.
    pub(super) fn deal_in(&mut self, feature: Feature, events: &mut Vec<Event>) {
        let allowed: Vec<crate::cards::DefId> = (0..crate::cards::POOL.len() as u16)
            .map(crate::cards::DefId)
            .filter(|d| d.def().needs().is_none_or(|f| self.has(f)))
            .collect();
        let mut fresh: Vec<crate::cards::DefId> = allowed
            .iter()
            .copied()
            .filter(|d| d.def().needs() == Some(feature) && !self.slice.contains(d))
            .collect();
        self.rng.shuffle(&mut fresh);
        fresh.truncate(DEALT_IN);
        self.slice.extend(fresh.iter().copied());
        fresh.extend(crate::cards::answer(&mut self.slice, &allowed));
        self.slice.sort();
        let mut count = 0;
        for def in fresh {
            for _ in 0..crate::cards::COPIES {
                let card = crate::cards::CardId(self.defs.len() as u32);
                self.defs.push(def);
                self.deck.push(card);
                count += 1;
            }
        }
        if count > 0 {
            self.rng.shuffle(&mut self.deck);
            events.push(Event::DeckGrew {
                feature,
                cards: count,
            });
        }
    }

    /// An awakening set aside at the last dusk comes in now, before the
    /// wishes: the first one that still can; the rest wait on.
    pub(super) fn awaken_deferred(&mut self, events: &mut Vec<Event>) {
        while !self.deferred.is_empty() && !self.awakened_tonight() {
            let (player, god, feature) = self.deferred.remove(0);
            let near = self.hex_of(player);
            self.awaken(Some(player), god, feature, near, events);
        }
    }

    /// The first thing of a mechanic just come in, near `near`.
    fn first_of(
        &mut self,
        feature: Feature,
        player: Option<PlayerId>,
        near: Hex,
        events: &mut Vec<Event>,
    ) {
        match feature {
            Feature::Bodies => {
                for _ in 0..2 {
                    let spot = self
                        .free_land_near(near, 1, |_, t| t.corpse.is_none() && !t.terrain.crowded());
                    if let Some(hex) = spot {
                        self.board.tile_mut(hex).expect("found on the board").corpse =
                            Some(Corpse::fresh());
                        events.push(Event::CorpseAppeared { hex });
                    }
                }
            }
            Feature::Groves => {
                if let Some(hex) = self.nearest_corpse(near) {
                    self.grow(hex, events);
                }
            }
            Feature::Settlements => {
                self.settle_near(near, events);
            }
            Feature::Militia => {
                let homes: Vec<Hex> = self
                    .board
                    .land()
                    .filter(|(_, t)| t.terrain == Terrain::Settlement)
                    .map(|(h, _)| h)
                    .collect();
                for home in homes {
                    self.militia
                        .entry((home.x(), home.y()))
                        .or_insert(super::Militia {
                            men: super::MILITIA,
                            at: Some(home),
                        });
                }
            }
            Feature::Undead => {
                if let Some(hex) = self.nearest_corpse(near)
                    && self.champion_at(hex).is_none()
                    && !self.mob_at(hex)
                {
                    self.board.tile_mut(hex).expect("a body lies there").corpse = None;
                    self.next_mob += 1;
                    let undead = mobs::Mob {
                        id: self.next_mob,
                        kind: mobs::MobKind::Undead,
                        hex,
                        hp: super::UNDEAD_HEALTH,
                    };
                    self.mobs.push(undead);
                    events.push(Event::MobAppeared { mob: undead });
                }
            }
            Feature::Trials => {
                if let Some(hex) = self.trial_spot(Some((near, 1, 4))) {
                    self.set_trial(hex, events);
                }
            }
            Feature::Loot => {
                if let Some(p) = player {
                    self.gain_loot(p, events);
                }
            }
            Feature::Rivers => {
                self.run_river(near, super::RIVER_RUN, events);
            }
            Feature::Lakes => {
                self.flood(near, 2, events);
            }
            Feature::Roads => {
                self.run_road(near, super::ROAD_RUN, events);
            }
            Feature::Fields => {
                let town = self
                    .board
                    .land()
                    .filter(|(_, t)| t.terrain == Terrain::Settlement)
                    .map(|(h, _)| h)
                    .min_by_key(|h| (h.unsigned_distance_to(near), h.x(), h.y()));
                if let Some(town) = town {
                    let plains: Vec<Hex> = town
                        .all_neighbors()
                        .into_iter()
                        .filter(|&h| {
                            self.board
                                .tile(h)
                                .is_some_and(|t| t.terrain == Terrain::Plains)
                        })
                        .take(2)
                        .collect();
                    for hex in plains {
                        self.make_field(hex, events);
                    }
                }
            }
            Feature::Goods => self.make_goods(events),
            Feature::Rulers => self.seat_rulers(),
            Feature::Guests => self.guest_at_dusk(events),
            Feature::Fires => {
                if let Some(p) = player {
                    self.fire_near(p, near, 1, events);
                }
            }
            // These show themselves in their own time: the undead laying a
            // settlement waste, poison on a card, the guard on the loud, a
            // champion slipping away, beasts by night.
            Feature::Ruins
            | Feature::Poison
            | Feature::Guard
            | Feature::Stealth
            | Feature::Beasts
            | Feature::Cargo
            | Feature::Buildings
            | Feature::City
            | Feature::Companions
            | Feature::Legion
            | Feature::Piranhas
            | Feature::Fairs
            | Feature::Burial
            | Feature::Ritual
            | Feature::Monsters
            | Feature::Dragons
            | Feature::Wilds
            | Feature::Pens
            | Feature::WalkingGroves
            | Feature::Arena
            | Feature::Debts
            | Feature::Underworld => {}
        }
    }

    // ---- The land ----

    /// The nearest land to `near`, `min` rings out or more, that passes
    /// `fits`, where nobody stands and nothing walks.
    fn free_land_near(
        &self,
        near: Hex,
        min: u32,
        fits: impl Fn(Hex, &crate::board::Tile) -> bool,
    ) -> Option<Hex> {
        let reach = self.board.extent() * 2 + 2;
        (min..=reach).find_map(|r| {
            near.ring(r).find(|&h| {
                self.board
                    .tile(h)
                    .is_some_and(|t| t.terrain.is_land() && fits(h, t))
                    && self.champion_at(h).is_none()
                    && !self.mob_at(h)
                    && !self.traps.iter().any(|tr| tr.hex == h)
            })
        })
    }

    fn nearest_corpse(&self, near: Hex) -> Option<Hex> {
        self.board
            .corpses()
            .map(|(h, _)| h)
            .min_by_key(|h| (h.unsigned_distance_to(near), h.x(), h.y()))
    }

    /// Ground anything may be built on: open land, nothing sacred on it.
    fn buildable(terrain: Terrain) -> bool {
        matches!(
            terrain,
            Terrain::Plains | Terrain::Forest | Terrain::Swamp | Terrain::Grove | Terrain::Ruins
        )
    }

    /// New land for `god` at the rim of its region, nearest `near`: `count`
    /// hexes of the kind asked for, if the god makes it, else its own.
    pub(super) fn rise(
        &mut self,
        god: God,
        terrain: Option<Terrain>,
        near: Hex,
        count: usize,
        events: &mut Vec<Event>,
    ) -> Vec<Hex> {
        let lands = god_lands(god);
        let terrain = terrain.filter(|t| lands.contains(t)).unwrap_or(lands[0]);
        let mut raised = Vec::new();
        for _ in 0..count {
            // Asked again each time: a raised hex opens the rim beyond it.
            let Some(&hex) = self.board.frontier(god, near).first() else {
                break;
            };
            if self.raise_land(hex, terrain, events) {
                raised.push(hex);
            }
        }
        raised
    }

    /// Land around `centre` goes into the mist: `count` hexes, nearest first
    /// from ring `from` out, never where someone stands. Round a rival it
    /// closes in (from 1); round the asker it keeps their ground and cuts
    /// it off (from 2).
    pub(super) fn veil_around(
        &mut self,
        centre: Hex,
        from: u32,
        count: usize,
        events: &mut Vec<Event>,
    ) {
        let spots: Vec<Hex> = (from..=from + 1)
            .flat_map(|r| centre.ring(r).collect::<Vec<_>>())
            .collect();
        let mut done = 0;
        for hex in spots {
            if done == count {
                break;
            }
            if self.veil(hex, events) {
                done += 1;
            }
        }
    }

    /// The ring two hexes round `centre` goes into the mist, `count` hexes of
    /// it, those nearest the Table first: the ground within is cut off.
    pub(super) fn cut_off(&mut self, centre: Hex, count: usize, events: &mut Vec<Event>) {
        let mut ring: Vec<Hex> = centre.ring(2).collect();
        ring.sort_by_key(|h| (h.ulength(), h.x(), h.y()));
        let mut done = 0;
        for hex in ring {
            if done == count {
                break;
            }
            if self.veil(hex, events) {
                done += 1;
            }
        }
    }

    /// The mist nearest `near` lifts: `count` hexes.
    pub(super) fn unveil_near(&mut self, near: Hex, count: usize, events: &mut Vec<Event>) {
        let mut mist: Vec<Hex> = self
            .board
            .tiles()
            .filter(|(_, t)| t.terrain == Terrain::Mist)
            .map(|(h, _)| h)
            .collect();
        mist.sort_by_key(|h| (h.unsigned_distance_to(near), h.x(), h.y()));
        for hex in mist.into_iter().take(count) {
            self.unveil(hex, events);
        }
    }

    /// A settlement on free land near `near`, not beside another, with its
    /// militia if the world has militia; nobody holds it yet.
    pub(super) fn settle_near(&mut self, near: Hex, events: &mut Vec<Event>) -> Option<Hex> {
        let hex = self.free_land_near(near, 1, |h, t| {
            Self::buildable(t.terrain)
                && t.corpse.is_none()
                && self.trial_at(h).is_none()
                && !h.all_neighbors().iter().any(|&n| {
                    self.board
                        .tile(n)
                        .is_some_and(|t| t.terrain == Terrain::Settlement)
                })
        })?;
        self.board
            .tile_mut(hex)
            .expect("found on the board")
            .terrain = Terrain::Settlement;
        self.ruins.remove(&(hex.x(), hex.y()));
        events.push(Event::TerrainChanged {
            hex,
            terrain: Terrain::Settlement,
        });
        if self.has(Feature::Militia) {
            self.militia.insert(
                (hex.x(), hex.y()),
                super::Militia {
                    men: 1,
                    at: Some(hex),
                },
            );
        }
        Some(hex)
    }

    /// Standing stones on free land near `near`.
    pub(super) fn stones_near(&mut self, near: Hex, events: &mut Vec<Event>) -> Option<Hex> {
        let hex = self.free_land_near(near, 1, |h, t| {
            Self::buildable(t.terrain) && t.corpse.is_none() && self.trial_at(h).is_none()
        })?;
        self.board
            .tile_mut(hex)
            .expect("found on the board")
            .terrain = Terrain::Stones;
        events.push(Event::TerrainChanged {
            hex,
            terrain: Terrain::Stones,
        });
        Some(hex)
    }

    /// A creation wish, granted (§21.9).
    pub(super) fn create(
        &mut self,
        player: PlayerId,
        god: God,
        act: Act,
        power: u8,
        events: &mut Vec<Event>,
    ) {
        let me = self.hex_of(player);
        match act {
            Act::Rise { terrain } => {
                let raised = self.rise(god, terrain, me, usize::from(power) + 1, events);
                self.raised.extend(raised);
            }
            Act::Veil { target } => {
                let centre = target.map_or(me, |t| self.hex_of(t));
                self.veil_around(centre, 1, usize::from(power), events);
            }
            Act::Unveil => self.unveil_near(me, usize::from(power), events),
            Act::Cut => self.cut_off(me, usize::from(power) + 2, events),
            Act::Settle => {
                if self.has(Feature::Settlements) {
                    self.settle_near(me, events);
                } else {
                    self.awaken(Some(player), god, Feature::Settlements, me, events);
                }
            }
            Act::Stones => {
                self.stones_near(me, events);
            }
            Act::River => {
                if self.has(Feature::Rivers) {
                    self.run_river(me, super::RIVER_RUN + usize::from(power), events);
                } else {
                    self.awaken(Some(player), god, Feature::Rivers, me, events);
                }
            }
            Act::Fire => {
                if self.has(Feature::Fires) {
                    self.fire_near(player, me, 1 + usize::from(power) / 3, events);
                } else if self.can_awaken(Feature::Fires) {
                    self.awaken(Some(player), god, Feature::Fires, me, events);
                }
            }
            Act::Road => {
                if self.has(Feature::Roads) {
                    self.run_road(me, super::ROAD_RUN + usize::from(power), events);
                } else if self.can_awaken(Feature::Roads) {
                    self.awaken(Some(player), god, Feature::Roads, me, events);
                }
            }
            Act::Flood => {
                if self.has(Feature::Lakes) {
                    self.flood(me, 1 + usize::from(power) / 2, events);
                } else if self.can_awaken(Feature::Lakes) {
                    self.awaken(Some(player), god, Feature::Lakes, me, events);
                }
            }
            Act::Awaken { feature } => {
                let chosen = feature
                    .filter(|&f| self.can_awaken(f))
                    .or_else(|| self.awakenable(god).first().copied());
                if let Some(f) = chosen {
                    self.awaken(Some(player), god, f, me, events);
                }
            }
            _ => {}
        }
    }

    /// The god's twist on a creation (§21.9).
    pub(super) fn creation_twist(&mut self, player: PlayerId, god: God, events: &mut Vec<Event>) {
        let me = self.hex_of(player);
        match god {
            // What was made grows over: a wood beside the asker, and one
            // beside the nearest rival.
            God::Bhava => {
                let rival = self
                    .players()
                    .filter(|&p| p != player)
                    .min_by_key(|&p| (self.hex_of(p).unsigned_distance_to(me), p.0));
                let centres: Vec<Hex> = std::iter::once(me)
                    .chain(rival.map(|r| self.hex_of(r)))
                    .collect();
                for centre in centres {
                    let spot = self.free_land_near(centre, 1, |_, t| t.terrain == Terrain::Plains);
                    if let Some(hex) = spot {
                        self.board.tile_mut(hex).expect("found").terrain = Terrain::Forest;
                        events.push(Event::TerrainChanged {
                            hex,
                            terrain: Terrain::Forest,
                        });
                    }
                }
            }
            // What was made is hungry: Trishna comes nearer to Devouring.
            God::Trishna => self.offer(None, God::Trishna, 1, events),
            // She gives by taking: the far rim of the asker's land goes.
            God::Zaga => {
                let region = self.board.tile(me).and_then(|t| t.region);
                let mut rim: Vec<Hex> = self
                    .board
                    .land()
                    .filter(|(h, t)| {
                        region.is_some()
                            && t.region == region
                            && h.all_neighbors()
                                .iter()
                                .any(|&n| self.board.tile(n).is_none())
                    })
                    .map(|(h, _)| h)
                    .collect();
                rim.sort_by_key(|h| (std::cmp::Reverse(h.unsigned_distance_to(me)), h.x(), h.y()));
                for hex in rim {
                    if self.veil(hex, events) {
                        break;
                    }
                }
            }
            // Written into the register: the asker owes for it.
            God::Ahamar => self.add_threat(player, 1, events),
            // What rose stays in her fog: the others see mist there until
            // the next dusk.
            God::Maya => {
                for hex in std::mem::take(&mut self.raised) {
                    self.fog.insert((hex.x(), hex.y()), player);
                }
            }
        }
        self.raised.clear();
    }

    /// A wish without style: the god makes what it likes near the asker,
    /// and not for their good (§21.3). A mechanic of its own if one can come
    /// in tonight, else its land around them.
    pub(super) fn god_creates(&mut self, player: PlayerId, god: God, events: &mut Vec<Event>) {
        let me = self.hex_of(player);
        let own = self.awakenable(god).into_iter().find(|f| f.domain() == god);
        if let Some(feature) = own
            && !self.awakened_tonight()
            && self.awaken(Some(player), god, feature, me, events)
        {
            return;
        }
        self.grant_land(god, me, 2, events);
    }

    /// Mist to `viewer` where Maya's fog lies (her twist), until dusk.
    pub fn fogged(&self, hex: Hex, viewer: PlayerId) -> bool {
        self.fog
            .get(&(hex.x(), hex.y()))
            .is_some_and(|&owner| owner != viewer)
    }

    /// The god's own land around `centre`, `count` hexes (the Land wish).
    pub(super) fn grant_land(
        &mut self,
        god: God,
        centre: Hex,
        count: usize,
        events: &mut Vec<Event>,
    ) {
        let terrain = super::wish::god_terrain(god);
        let spots: Vec<Hex> = centre
            .all_neighbors()
            .into_iter()
            .chain(std::iter::once(centre))
            .filter(|&h| {
                self.board.tile(h).is_some_and(|t| {
                    t.terrain.can_grow_grove()
                            || t.terrain == Terrain::Mountain
                            // The god's land drains the water it takes.
                            || t.terrain.is_water()
                }) && !self.mob_at(h)
            })
            .take(count)
            .collect();
        for hex in spots {
            if let Some(tile) = self.board.tile_mut(hex) {
                tile.terrain = terrain;
            }
            events.push(Event::TerrainChanged { hex, terrain });
        }
    }
}

/// Distinct cards a mechanic brings into the deck when it comes in.
pub const DEALT_IN: usize = 3;
/// Radius of a world being created (§21.1): 61 hexes, room for five.
pub const CREATION_RADIUS: u32 = 4;
/// Distinct cards a world being created starts its deck with.
pub const CREATION_SLICE: usize = 16;

/// A world to create (§21.1): a small board of plains and two or three
/// other kinds of land, and one mechanic from those that need nothing the
/// board lacks.
pub(super) fn seed_world(
    rng: &mut crate::rng::Rng,
) -> (crate::board::Board, crate::features::World) {
    let mut kinds = vec![
        Terrain::Forest,
        Terrain::Mountain,
        Terrain::Swamp,
        Terrain::Stones,
    ];
    rng.shuffle(&mut kinds);
    kinds.truncate(2 + rng.below(2) as usize);
    let mut board = crate::board::Board::seed_world(rng, CREATION_RADIUS, &kinds);
    let has_land = |board: &crate::board::Board, ks: &[Terrain]| {
        board.land().any(|(_, t)| ks.contains(&t.terrain))
    };
    let options: Vec<Feature> = [
        Feature::Bodies,
        Feature::Settlements,
        Feature::Stealth,
        Feature::Trials,
    ]
    .into_iter()
    .filter(|f| {
        f.requires().iter().all(|n| match *n {
            Need::Land(ks) => has_land(&board, ks),
            _ => false,
        }) || f.requires().is_empty()
    })
    .collect();
    let start = *rng.pick(&options).expect("bodies need nothing");
    if start == Feature::Settlements {
        board.settle_regions(rng, 1);
    }
    (board, crate::features::World::of([start]))
}
