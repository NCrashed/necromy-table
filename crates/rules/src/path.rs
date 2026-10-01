//! The path to a Great Deed (docs/storyteller-plan.md, §21.7): its next
//! step, one clear thing with its place on the board, read off the world
//! as it stands. The bot walks the same path, so what the hint says can be
//! done is what the bots do to win (`deeds_in_bot_games` checks it).

use hexx::Hex;
use serde::{Deserialize, Serialize};

use crate::cards::Effect;
use crate::game::{Act, CheckKind, Game, GreatDeed, Intent, PlayerId, Target};
use crate::gods::God;

/// The next thing to do for one's deed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Step {
    pub what: StepWhat,
    /// Where: the hex to go to, or the one the move is made at.
    pub at: Option<Hex>,
    /// The check of the deed it works on, the first not met.
    pub check: Option<CheckKind>,
    /// Who tells it: the deed's patron.
    pub god: God,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StepWhat {
    /// A move to make now, where the champion stands.
    Do(Intent),
    /// Go to `at`, for this.
    Go(Errand),
    /// At dusk, ask this god for this.
    Wish { god: God, act: Act },
    /// Spirit to gather first: this much is needed.
    Spirit(u8),
    /// Everything holds: keep it so until dusk.
    Hold,
    /// Nothing to do towards it now: the world has to move first.
    Wait(Why),
}

/// What a wait is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Why {
    /// Dusk moves the world on: goods, food, groves, the dusks counted.
    Dusk,
    /// Goods come to the settlements at dusk.
    Goods,
    /// The one called to the arena has to come.
    Duel,
    /// Bets are settled at dusk.
    Bets,
}

/// Why a hex is worth going to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Errand {
    /// Land of the region it dissolves, to call the mist there.
    Dissolve,
    /// The grove grown from a champion, to keep it.
    HeroGrove,
    /// The end of a river, to run it on.
    RiverEnd,
    /// Two hexes from the Table, where water may run to it.
    Spring,
    /// Standing stones, to draw a circle.
    Stones,
    /// A body, to take it up.
    Body,
    /// Its circle, to lay the body in.
    Circle,
    /// Its monster, to fell.
    Monster,
    /// A trial on a mountain, for the egg.
    MountainTrial,
    /// Woods, to lay the egg in.
    Nest,
    /// Beside the egg, to set its woods alight.
    EggFire,
    /// The egg, to take it up again.
    Egg,
    /// A settlement of its own, to build this there.
    BuildSite(crate::game::Building),
    /// Its arena, to wait for the challenged.
    Arena,
    /// Its way down.
    Delve,
    /// Ruins, to open a way down under.
    Ruins,
    /// A grove, to wake.
    Grove,
    /// Its pen, to tether a beast.
    Pen,
    /// Beside a beast, to tame it.
    Beast,
    /// The guest, to lead.
    Guest,
    /// The Table.
    Table,
    /// Ground by the graveyard, to consecrate.
    Consecrate,
    /// The graveyard, to bury.
    Bury,
    /// Its plague pit.
    Pit,
    /// An undead in Zaga's land, to put down.
    Undead,
    /// An undead, to enlist.
    Enlist,
    /// A ruler, to win over.
    Ruler,
    /// A vassal, to set against the rest.
    Vassal,
    /// A free settlement, to take.
    Settlement,
    /// A settlement of its own, to open a fair in.
    FairSite,
    /// Its fair.
    Fair,
    /// Goods lying, to take up.
    Goods,
    /// Its feast hall.
    Hall,
    /// Its settlement, to store the food.
    Store,
    /// Plains by its settlement, to sow.
    Field,
    /// Food lying, to take up.
    Food,
    /// Woods in a region its fire has not passed.
    Burn,
    /// A settlement where two lands meet, for a shrine of both.
    Border,
    /// A settlement of a land whose goods the fair lacks.
    GoodsTown,
    /// A rival to fell: a champion's body grows the grove.
    Prey,
    /// A ruler fond of it, to match with another.
    Match,
    /// Land where two quenching lands meet.
    BorderLand,
}

impl Game {
    /// The next step of `player`'s Great Deed, if they have chosen one.
    pub fn next_step(&self, player: PlayerId) -> Option<Step> {
        if self.winner().is_some() {
            return None;
        }
        let deed = self.deed(player)?;
        let check = self
            .checks(player, deed)
            .into_iter()
            .find(|c| !c.met())
            .map(|c| c.kind);
        let step = |what, at| Step {
            what,
            at,
            check,
            god: deed.patron(),
        };
        if self.on_eve(player) || check.is_none() {
            return Some(step(StepWhat::Hold, None));
        }
        let here = self.champion(player)?.hex;
        if let Some(&missing) = deed.needs().iter().find(|&&f| !self.has(f))
            && self.can_awaken(missing)
        {
            let act = Act::Awaken {
                feature: Some(missing),
            };
            return Some(step(
                StepWhat::Wish {
                    god: missing.domain(),
                    act,
                },
                None,
            ));
        }
        if let Some(intent) = crate::bot::deed_work(self, player)
            .filter(|i| !matches!(i, Intent::PayDebt { .. }))
            .or_else(|| crate::bot::mechanic_play(self, player).filter(|i| self.for_deed(i)))
        {
            let at = at_of(self, &intent, here);
            return Some(step(StepWhat::Do(intent), Some(at)));
        }
        if let Some((hex, errand)) = deed_target(self, player) {
            return Some(step(StepWhat::Go(errand), Some(hex)));
        }
        if let Some((god, act)) = own_wish(self, player) {
            return Some(step(StepWhat::Wish { god, act }, None));
        }
        let spirit = self.champion(player)?.spirit_points;
        let need = match deed {
            GreatDeed::City => crate::QUARTER_SPIRIT,
            GreatDeed::Reconciliation | GreatDeed::Arena | GreatDeed::Ark => crate::BUILD_SPIRIT,
            GreatDeed::GreatFire | GreatDeed::Dragon => crate::KINDLE_SPIRIT,
            _ => 0,
        };
        if let Some((what, at)) =
            fallback(self, player, deed).filter(|(w, _)| !matches!(w, StepWhat::Wait(_)))
        {
            return Some(step(what, at));
        }
        if spirit < need {
            return Some(step(StepWhat::Spirit(need), None));
        }
        Some(match fallback(self, player, deed) {
            Some((what, at)) => step(what, at),
            None => step(StepWhat::Wait(Why::Dusk), None),
        })
    }

    /// A card of the mechanics played for one's own deed, not to rob or
    /// bite a rival.
    fn for_deed(&self, intent: &Intent) -> bool {
        let Intent::Play { card, .. } = intent else {
            return false;
        };
        !matches!(
            self.def(*card).effect,
            Effect::Rob | Effect::Lure | Effect::Piranha(_) | Effect::Levy | Effect::Rain
        )
    }
}

/// Where a move is made: its hex, else where one stands.
fn at_of(game: &Game, intent: &Intent, here: Hex) -> Hex {
    match *intent {
        Intent::Kindle { hex }
        | Intent::Douse { hex }
        | Intent::Gift { hex }
        | Intent::Quarter { hex }
        | Intent::Play {
            target: Target::Hex(hex),
            ..
        } => hex,
        Intent::Play {
            target: Target::Champion(p),
            ..
        }
        | Intent::Challenge { rival: p }
        | Intent::BetOn { rival: p, .. } => game.champion(p).map_or(here, |c| c.hex),
        Intent::Recruit { mob } => game
            .mobs()
            .iter()
            .find(|m| m.id == mob)
            .map_or(here, |m| m.hex),
        _ => here,
    }
}

/// What a wish at dusk can do for the deed's next unmet check (§21.10).
pub(crate) fn own_wish(game: &Game, player: PlayerId) -> Option<(God, Act)> {
    let deed = game.deed(player)?;
    if let Some(&missing) = deed.needs().iter().find(|&&f| !game.has(f))
        && game.can_awaken(missing)
    {
        return Some((
            missing.domain(),
            Act::Awaken {
                feature: Some(missing),
            },
        ));
    }
    let unmet = game
        .checks(player, deed)
        .into_iter()
        .find(|c| !c.met())?
        .kind;
    use CheckKind::*;
    Some(match (deed, unmet) {
        // Its ground cut off from the world, then a settlement of its own there.
        (GreatDeed::Island, IslandSize) => (God::Maya, Act::Cut),
        (GreatDeed::Island, IslandSettled) => (God::Trishna, Act::Settle),
        // A region too small grows first, by its own god; then the mist
        // round a rival standing there, or round itself when it is there.
        (GreatDeed::DissolvedLand, RegionInMist) => {
            let (god, _, all) = game.dissolving(player);
            let region_of = |p: PlayerId| {
                game.champion(p)
                    .and_then(|c| game.board().tile(c.hex))
                    .and_then(|t| t.region)
            };
            let me = game.champion(player)?.hex;
            let near = game
                .left_to_dissolve(player)
                .is_some_and(|h| h.unsigned_distance_to(me) <= 2);
            if all < crate::game::DISSOLVED {
                (god, Act::Rise { terrain: None })
            } else if near {
                (God::Maya, Act::Veil { target: None })
            } else {
                let rival = game
                    .players()
                    .find(|&p| p != player && region_of(p) == Some(god))?;
                (
                    God::Maya,
                    Act::Veil {
                        target: Some(rival),
                    },
                )
            }
        }
        // No undead to enlist: bodies near, to rise.
        (GreatDeed::Legion, LegionSize) if !game.mobs().iter().any(|m| m.is_undead()) => {
            (God::Zaga, Act::Dead { target: None })
        }
        (GreatDeed::River, RiverLength | RiverSource | RiverMouth) => {
            (God::Maya, Act::River { target: None })
        }
        (GreatDeed::FloodedTable, TableFlooded) => (God::Maya, Act::Flood { target: None }),
        (GreatDeed::Roads, TemplesLinked) => (God::Ahamar, Act::Road),
        (GreatDeed::GreatFire, RegionsBurnt | FireBurning) => {
            (God::Trishna, Act::Fire { target: None })
        }
        (GreatDeed::Amazon, JungleWoods) => (God::Bhava, Act::Land),
        (GreatDeed::Amazon, JungleRiver) => (God::Maya, Act::River { target: None }),
        // Bhava's woods round where it stands.
        (GreatDeed::WorldTree, WoodsAround) => (God::Bhava, Act::Land),
        // Groves round it to wake.
        (GreatDeed::WalkingForest, GroveRooted)
            if !game
                .board()
                .land()
                .any(|(_, t)| t.terrain == crate::Terrain::Grove) =>
        {
            (God::Bhava, Act::Land)
        }
        // A god of the pair out of its light: an offering to the god that
        // quenches it cools it (§5.1).
        (GreatDeed::Reconciliation, PairLight) => {
            let dark = God::ALL
                .into_iter()
                .find(|&g| game.stage(g) > 0 && game.stage(God::from_index(g.index() + 2)) == 0)
                .or_else(|| God::ALL.into_iter().find(|&g| game.stage(g) > 0))?;
            (God::from_index(dark.index() + 3), Act::Peace)
        }
        _ => return None,
    })
}

/// The nearest of `hexes` to `me`.
fn nearest(me: Hex, hexes: impl IntoIterator<Item = Hex>) -> Option<Hex> {
    hexes
        .into_iter()
        .min_by_key(|h| (h.unsigned_distance_to(me), h.x(), h.y()))
}

/// Where the deed wants its champion to stand, and why (§21.10).
pub(crate) fn deed_target(game: &Game, player: PlayerId) -> Option<(Hex, Errand)> {
    use crate::{Building, Cargo, MobKind, Terrain};
    let me = game.champion(player)?.hex;
    let go = |errand: Errand| move |h: Hex| (h, errand);
    match game.deed(player)? {
        GreatDeed::DissolvedLand => game.left_to_dissolve(player).map(go(Errand::Dissolve)),
        GreatDeed::WorldTree => game.hero_grove().map(go(Errand::HeroGrove)),
        GreatDeed::Island | GreatDeed::Amazon | GreatDeed::Roads | GreatDeed::DebtBondage => None,
        // By the river's end, to run it on.
        GreatDeed::River => game
            .river_head(me)
            .filter(|h| h.unsigned_distance_to(me) > 2)
            .map(go(Errand::RiverEnd)),
        // Two hexes from the Table, where a spring runs towards it.
        GreatDeed::FloodedTable => {
            if me.ulength() == 2 {
                return None;
            }
            nearest(
                me,
                Hex::ZERO
                    .ring(2)
                    .filter(|&h| game.board().contains(h) && game.occupant(h).is_none()),
            )
            .map(go(Errand::Spring))
        }
        // Stones for a circle, bodies to feed it, then its monster.
        GreatDeed::Summoning => {
            if let Some(m) = game.mobs().iter().find(
                |m| matches!(m.kind, MobKind::Monster { summoner, .. } if summoner == Some(player)),
            ) {
                return Some((m.hex, Errand::Monster));
            }
            match game.circles().find(|(_, c)| c.owner == player) {
                Some((circle, _)) if matches!(game.cargo(player), Some(Cargo::Body { .. })) => {
                    Some((circle, Errand::Circle))
                }
                Some(_) => {
                    nearest(me, game.board().corpses().map(|(h, _)| h)).map(go(Errand::Body))
                }
                None => nearest(
                    me,
                    game.board()
                        .land()
                        .filter(|(h, t)| t.terrain == Terrain::Stones && game.circle(*h).is_none())
                        .map(|(h, _)| h),
                )
                .map(go(Errand::Stones)),
            }
        }
        // A mountain trial for the egg, woods to lay it in, beside it to set
        // them alight.
        GreatDeed::Dragon => {
            if matches!(game.cargo(player), Some(Cargo::Egg { .. })) {
                return nearest(
                    me,
                    game.board()
                        .land()
                        .filter(|(h, t)| {
                            t.terrain.burns() && game.occupant(*h).is_none() && *h != me
                        })
                        .map(|(h, _)| h),
                )
                .map(go(Errand::Nest));
            }
            if let Some((egg, _)) = game
                .loads()
                .iter()
                .find(|(_, c)| matches!(c, Cargo::Egg { by, .. } if *by == Some(player)))
            {
                let burnable = game.board().tile(*egg).is_some_and(|t| t.terrain.burns());
                return if burnable {
                    (me.unsigned_distance_to(*egg) != 1)
                        .then(|| {
                            egg.all_neighbors()
                                .into_iter()
                                .find(|&h| game.board().contains(h) && game.occupant(h).is_none())
                        })
                        .flatten()
                        .map(go(Errand::EggFire))
                } else {
                    Some((*egg, Errand::Egg))
                };
            }
            nearest(
                me,
                game.trials()
                    .iter()
                    .filter(|t| {
                        game.trial_for(player, t.hex).is_some()
                            && game
                                .board()
                                .tile(t.hex)
                                .is_some_and(|x| x.terrain == Terrain::Mountain)
                    })
                    .map(|t| t.hex),
            )
            .map(go(Errand::MountainTrial))
        }
        // An arena at home, and there to wait for the challenged.
        GreatDeed::Arena => {
            let arena = game
                .buildings()
                .find(|&(h, b)| b == Building::Arena && game.owner(h) == Some(player))
                .map(|(h, _)| h);
            match arena {
                Some(a) => (a != me).then_some((a, Errand::Arena)),
                None => nearest(
                    me,
                    game.claims()
                        .filter(|&(h, p)| p == player && game.building(h).is_none() && h != me)
                        .map(|(h, _)| h),
                )
                .map(go(Errand::BuildSite(Building::Arena))),
            }
        }
        // Its way down, else ruins to open one under.
        GreatDeed::Treasury => {
            if let Some((h, _)) = game.delves().find(|(_, d)| d.owner == player) {
                return (h != me).then_some((h, Errand::Delve));
            }
            nearest(
                me,
                game.board()
                    .land()
                    .filter(|(h, t)| {
                        t.terrain == Terrain::Ruins && game.delve(*h).is_none() && *h != me
                    })
                    .map(|(h, _)| h),
            )
            .map(go(Errand::Ruins))
        }
        // A grove to wake, while none of its own walks.
        GreatDeed::WalkingForest => {
            if game.walkers().any(|(_, p)| p == player) {
                return None;
            }
            nearest(
                me,
                game.board()
                    .land()
                    .filter(|(h, t)| t.terrain == Terrain::Grove && *h != me)
                    .map(|(h, _)| h),
            )
            .map(go(Errand::Grove))
        }
        // Home to build the pen, beasts of the elements it lacks, the pen to
        // tether them in.
        GreatDeed::Ark => {
            let away = |hexes: Vec<Hex>| nearest(me, hexes.into_iter().filter(|h| *h != me));
            let pen = game
                .buildings()
                .find(|&(h, b)| b == Building::Pen && game.owner(h) == Some(player))
                .map(|(h, _)| h);
            let Some(pen) = pen else {
                return away(
                    game.claims()
                        .filter(|&(h, p)| p == player && game.building(h).is_none())
                        .map(|(h, _)| h)
                        .collect(),
                )
                .map(go(Errand::BuildSite(Building::Pen)));
            };
            let lacks = |e: crate::Element| game.penned(pen) & (1 << e.index()) == 0;
            if game
                .companions(player)
                .iter()
                .any(|c| matches!(c, crate::Companion::Beast(e) if lacks(*e)))
            {
                return (me != pen).then_some((pen, Errand::Pen));
            }
            away(
                game.mobs()
                    .iter()
                    .filter(|m| m.is_beast() && lacks(game.beast_element(m)))
                    .flat_map(|m| m.hex.all_neighbors())
                    .filter(|&h| game.board().contains(h) && game.occupant(h).is_none())
                    .collect(),
            )
            .map(go(Errand::Beast))
        }
        // The stranger, then the Table.
        GreatDeed::Guest => {
            if game.companions(player).contains(&crate::Companion::Guest) {
                return Some((Hex::ZERO, Errand::Table));
            }
            game.mobs()
                .iter()
                .find(|m| matches!(m.kind, MobKind::Guest))
                .and_then(|m| {
                    nearest(
                        me,
                        m.hex.all_neighbors().into_iter().filter(|&h| {
                            game.board().contains(h) && game.occupant(h).is_none_or(|p| p == player)
                        }),
                    )
                })
                .map(go(Errand::Guest))
        }
        // Ground to consecrate by the graveyard, bodies to bring to it or the
        // pit, the dead to clear from Zaga's land.
        deed @ (GreatDeed::Necropolis | GreatDeed::PlaguePit) => {
            let corpses =
                || nearest(me, game.board().corpses().map(|(h, _)| h)).map(go(Errand::Body));
            let ground = |t: Terrain| {
                game.board()
                    .land()
                    .filter(move |(_, x)| x.terrain == t)
                    .map(|(h, _)| h)
            };
            let carrying = matches!(game.cargo(player), Some(Cargo::Body { .. }));
            if deed == GreatDeed::PlaguePit {
                let pit = game.pits().find(|(_, p)| p.owner == player).map(|(h, _)| h);
                return match pit {
                    Some(pit) if carrying || game.may_settle_pit(player) => {
                        Some((pit, Errand::Pit))
                    }
                    _ => corpses(),
                };
            }
            let (size, _) = game.best_necropolis();
            if size < crate::NECROPOLIS {
                let yard: Vec<Hex> = ground(Terrain::Graveyard).collect();
                return nearest(
                    me,
                    game.board()
                        .land()
                        .filter(|(h, t)| {
                            matches!(t.terrain, Terrain::Plains | Terrain::Ash)
                                && (yard.is_empty()
                                    || h.all_neighbors().iter().any(|n| yard.contains(n)))
                                && game.occupant(*h).is_none_or(|p| p == player)
                        })
                        .map(|(h, _)| h),
                )
                .map(go(Errand::Consecrate));
            }
            if carrying {
                return nearest(me, ground(Terrain::Graveyard)).map(go(Errand::Bury));
            }
            if !game.zaga_land_quiet() {
                return nearest(
                    me,
                    game.mobs()
                        .iter()
                        .filter(|m| {
                            m.is_undead()
                                && game.board().tile(m.hex).and_then(|t| t.region)
                                    == Some(God::Zaga)
                        })
                        .map(|m| m.hex),
                )
                .map(go(Errand::Undead));
            }
            corpses()
        }
        // Rulers to win over; for the empire the Table once three are sworn,
        // then a vassal to set against the rest.
        deed @ (GreatDeed::TripleUnion | GreatDeed::FallenEmpire) => {
            let away = |hexes: Vec<Hex>| nearest(me, hexes.into_iter().filter(|h| *h != me));
            if deed == GreatDeed::FallenEmpire {
                if game.emperor() == Some(player) {
                    return away(game.vassals(player)).map(go(Errand::Vassal));
                }
                if game.vassals(player).len() >= crate::CROWN_VASSALS && !game.crowned(player) {
                    return (me != Hex::ZERO).then_some((Hex::ZERO, Errand::Table));
                }
            }
            away(
                game.rulers()
                    .filter(|(_, r)| {
                        r.spouse.is_none()
                            && r.sworn != Some(player)
                            && r.favourite() != Some(player)
                    })
                    .map(|(h, _)| h)
                    .collect(),
            )
            .map(go(Errand::Ruler))
        }
        // Its fair: open one at home, then bring the goods it lacks; for the
        // dead, bide at the fair and let them come.
        deed @ (GreatDeed::FairOfFive | GreatDeed::DeadFeast) => {
            let mine = game.fairs().find(|(_, f)| f.host == player);
            let Some((fair, f)) = mine else {
                return own_town(game, player, me)
                    .map(go(Errand::FairSite))
                    .or_else(|| free_town(game, me).map(go(Errand::Settlement)));
            };
            if deed == GreatDeed::DeadFeast {
                // Near enough the fair to keep the living from it, not in the dead's way.
                return (me.unsigned_distance_to(fair) > 3).then_some((fair, Errand::Fair));
            }
            if matches!(game.cargo(player), Some(Cargo::Goods(g)) if !f.has(g)) {
                return Some((fair, Errand::Fair));
            }
            nearest(
                me,
                game.loads()
                    .iter()
                    .filter(|(h, c)| {
                        matches!(c, Cargo::Goods(g) if !f.has(*g))
                            && game.occupant(*h).is_none_or(|p| p == player)
                    })
                    .map(|(h, _)| *h),
            )
            .map(go(Errand::Goods))
        }
        // Sow, carry the harvest home, wait at the hall for guests.
        GreatDeed::Feast | GreatDeed::DeadBall => {
            if let Some(hall) = game.feast_hall(player) {
                return Some((hall, Errand::Hall));
            }
            let own: Vec<Hex> = game
                .claims()
                .filter(|&(h, p)| {
                    p == player
                        && game
                            .board()
                            .tile(h)
                            .is_some_and(|t| t.terrain == Terrain::Settlement)
                })
                .map(|(h, _)| h)
                .collect();
            if game.cargo(player) == Some(Cargo::Food) {
                return nearest(me, own).map(go(Errand::Store));
            }
            if own.is_empty() {
                return free_town(game, me).map(go(Errand::Settlement));
            }
            if game.fields_of(player) < crate::FEAST_FIELDS {
                return nearest(
                    me,
                    own.iter().flat_map(|h| h.all_neighbors()).filter(|&h| {
                        game.board()
                            .tile(h)
                            .is_some_and(|t| t.terrain == Terrain::Plains)
                            && game.occupant(h).is_none()
                    }),
                )
                .map(go(Errand::Field));
            }
            nearest(
                me,
                game.loads()
                    .iter()
                    .filter(|(h, c)| *c == Cargo::Food && game.occupant(*h).is_none())
                    .map(|(h, _)| *h),
            )
            .map(go(Errand::Food))
        }
        // Woods in a region its fire has not passed yet.
        GreatDeed::GreatFire => nearest(
            me,
            game.board()
                .land()
                .filter(|(h, t)| {
                    t.terrain.burns()
                        && *h != me
                        && t.region.is_some_and(|g| !game.burnt_by(player, g))
                })
                .map(|(h, _)| h),
        )
        .map(go(Errand::Burn)),
        // The nearest undead to write into the legion.
        GreatDeed::Legion => nearest(
            me,
            game.mobs().iter().filter(|m| m.is_undead()).map(|m| m.hex),
        )
        .map(go(Errand::Enlist)),
        // A settlement of its own to build on, or one to take, but only
        // with the Spirit to build there, else it would idle on it.
        deed @ (GreatDeed::City | GreatDeed::Reconciliation) => {
            let spirit = game.champion(player)?.spirit_points;
            let need = match deed {
                GreatDeed::City => crate::QUARTER_SPIRIT,
                _ if game.buildings().any(|(h, b)| {
                    border_pair(game, h).is_some_and(|p| b == Building::Shrine(p))
                }) =>
                {
                    return None;
                }
                _ => crate::BUILD_SPIRIT,
            };
            if spirit < need {
                return None;
            }
            let city = deed == GreatDeed::City;
            nearest(
                me,
                game.board()
                    .land()
                    // Not where it stands: deed_work found nothing to do there.
                    .filter(|(h, t)| {
                        *h != me
                            && t.terrain == Terrain::Settlement
                            && game.owner(*h).is_none_or(|o| o == player)
                            && if city {
                                game.building(*h).is_none() || room_to_grow(game, *h)
                            } else {
                                game.building(*h).is_none() && border_pair(game, *h).is_some()
                            }
                    })
                    .map(|(h, _)| h),
            )
            .map(go(if city {
                Errand::Settlement
            } else {
                Errand::Border
            }))
        }
    }
}

/// The settlement of `player`'s nearest `me`.
fn own_town(game: &Game, player: PlayerId, me: Hex) -> Option<Hex> {
    nearest(
        me,
        game.claims()
            .filter(|&(h, p)| {
                p == player
                    && game
                        .board()
                        .tile(h)
                        .is_some_and(|t| t.terrain == crate::Terrain::Settlement)
            })
            .map(|(h, _)| h),
    )
}

/// The settlement nobody holds nearest `me`.
fn free_town(game: &Game, me: Hex) -> Option<Hex> {
    nearest(
        me,
        game.board()
            .land()
            .filter(|(h, t)| t.terrain == crate::Terrain::Settlement && game.owner(*h).is_none())
            .map(|(h, _)| h),
    )
}

/// Free open land next to `hex` for a new quarter.
pub(crate) fn room_to_grow(game: &Game, hex: Hex) -> bool {
    use crate::Terrain;
    hex.all_neighbors().iter().any(|&n| {
        game.board().tile(n).is_some_and(|t| {
            matches!(
                t.terrain,
                Terrain::Plains | Terrain::Forest | Terrain::Grove | Terrain::Ruins
            ) && t.corpse.is_none()
        }) && game.occupant(n).is_none()
    })
}

/// The quenching pair whose lands lie within two of `hex` (a shrine of both
/// may stand there), if any: its own god first.
pub(crate) fn border_pair(game: &Game, hex: Hex) -> Option<[God; 2]> {
    let own = game.board().tile(hex)?.region?;
    hex.range(2)
        .filter_map(|n| game.board().tile(n).and_then(|t| t.region))
        .find(|&g| g.index() == (own.index() + 2) % 5 || own.index() == (g.index() + 2) % 5)
        .map(|g| [own, g])
}

/// When the deed's own plan has nothing here: what the world still lacks
/// for it, or what it waits on, said plainly.
fn fallback(game: &Game, player: PlayerId, deed: GreatDeed) -> Option<(StepWhat, Option<Hex>)> {
    use crate::{Cargo, Terrain};
    let me = game.champion(player)?.hex;
    let go = |errand: Errand| move |h: Hex| (StepWhat::Go(errand), Some(h));
    let settle = (
        StepWhat::Wish {
            god: God::Trishna,
            act: Act::Settle,
        },
        None,
    );
    let towns = |player_only: bool| {
        game.board()
            .land()
            .filter(move |(h, t)| {
                t.terrain == Terrain::Settlement && (!player_only || game.owner(*h) == Some(player))
            })
            .map(|(h, _)| h)
    };
    match deed {
        // Settlements are the stuff of these: none to work on, ask for one.
        GreatDeed::City | GreatDeed::Feast | GreatDeed::DeadBall
            if towns(true).next().is_none() && free_town(game, me).is_none() =>
        {
            Some(settle)
        }
        GreatDeed::City => Some(settle),
        GreatDeed::FairOfFive | GreatDeed::DeadFeast => {
            let Some((_, f)) = game.fairs().find(|(_, f)| f.host == player) else {
                return Some(settle);
            };
            if deed == GreatDeed::DeadFeast {
                return Some((StepWhat::Wait(Why::Dusk), None));
            }
            // Goods of a land it lacks come where a settlement of that land
            // is held: to one, or to take one.
            let lacking = |h: &Hex| {
                game.board()
                    .tile(*h)
                    .and_then(|t| t.region)
                    .is_some_and(|g| !f.has(g))
            };
            if matches!(game.cargo(player), Some(Cargo::Goods(_))) {
                return Some((StepWhat::Wait(Why::Goods), None));
            }
            nearest(
                me,
                towns(false).filter(|h| lacking(h) && game.owner(*h).is_some() && *h != me),
            )
            .map(go(Errand::GoodsTown))
            .or_else(|| {
                nearest(
                    me,
                    towns(false).filter(|h| lacking(h) && game.owner(*h).is_none()),
                )
                .map(go(Errand::Settlement))
            })
            .or(Some((StepWhat::Wait(Why::Goods), None)))
        }
        GreatDeed::Arena => Some((StepWhat::Wait(Why::Duel), None)),
        GreatDeed::DebtBondage => Some((StepWhat::Wait(Why::Bets), None)),
        // Two fond rulers of two lands: to one of them, to match them.
        GreatDeed::TripleUnion
            if {
                let fond: Vec<God> = game
                    .rulers()
                    .filter(|(_, r)| {
                        r.spouse.is_none()
                            && r.betrothed.is_none()
                            && r.favourite() == Some(player)
                            && r.regard_for(player) >= crate::MATCH_REGARD
                    })
                    .filter_map(|(h, _)| game.board().tile(h)?.region)
                    .collect();
                fond.iter().any(|g| fond.iter().any(|o| o != g))
            } =>
        {
            nearest(
                me,
                game.rulers()
                    .filter(|(_, r)| {
                        r.spouse.is_none() && r.betrothed.is_none() && r.favourite() == Some(player)
                    })
                    .map(|(h, _)| h),
            )
            .map(go(Errand::Match))
        }
        // Rulers not yet fond enough to be matched: more gifts.
        GreatDeed::TripleUnion | GreatDeed::FallenEmpire => nearest(
            me,
            game.rulers()
                .filter(|(_, r)| r.spouse.is_none() && r.regard_for(player) < crate::MATCH_REGARD)
                .map(|(h, _)| h)
                .filter(|h| *h != me),
        )
        .map(go(Errand::Ruler)),
        // A shrine where two lands meet: to such land, then ask for a
        // settlement there.
        GreatDeed::Reconciliation => {
            let border = |h: Hex| {
                game.board()
                    .tile(h)
                    .is_some_and(|t| t.terrain == Terrain::Plains)
                    && border_pair(game, h).is_some()
            };
            if me.all_neighbors().into_iter().chain([me]).any(border) {
                Some(settle)
            } else {
                nearest(
                    me,
                    game.board().land().map(|(h, _)| h).filter(|&h| border(h)),
                )
                .map(go(Errand::BorderLand))
            }
        }
        // A beast of an element the pen lacks: its god sends one.
        GreatDeed::Ark => {
            let pen = game
                .buildings()
                .find(|&(h, b)| b == crate::Building::Pen && game.owner(h) == Some(player))
                .map(|(h, _)| h)?;
            let lacks = crate::Element::ALL
                .into_iter()
                .find(|e| game.penned(pen) & (1 << e.index()) == 0)?;
            Some((
                StepWhat::Wish {
                    god: God::from_index(lacks.index()),
                    act: Act::Beast { target: None },
                },
                None,
            ))
        }
        // An egg is won on a mountain trial: ask for one.
        GreatDeed::Dragon => Some((
            StepWhat::Wish {
                god: God::Zaga,
                act: Act::Ordeal,
            },
            None,
        )),
        // A grove grows from a champion's body: the weakest rival near.
        GreatDeed::WorldTree if game.hero_grove().is_none() => nearest(
            me,
            game.players()
                .filter(|&p| p != player)
                .filter_map(|p| game.champion(p))
                .filter(|c| c.hp * 2 <= c.body + 1)
                .map(|c| c.hex),
        )
        .map(go(Errand::Prey))
        .or(Some((StepWhat::Wait(Why::Dusk), None))),
        _ => None,
    }
}
