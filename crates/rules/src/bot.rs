//! Placeholder bot: greedy card play, then walks towards the nearest corpse.
//!
//! Real bots are utility AI with a champion's character (docs/design.md §15).
//! This one exercises the turn loop and the reaction windows. It reads the
//! same state a human could see (plus its own hand) and uses no randomness,
//! so replays stay trivial. It never returns an illegal intent.

use hexx::Hex;

use crate::cards::{self, CardId, Effect};
use crate::game::{
    Game, Goal, GreatDeed, Intent, PlayerId, Target, TimeOfDay, WindowKind, WishKind,
};
use crate::gods::God;
use necromy_dice::Face;

pub fn choose(game: &Game, player: PlayerId) -> Intent {
    // The deed of the match first: its own patron's, if offered.
    if game.choosing().contains(&player) {
        let patron = game.champion(player).map(|c| c.god);
        let offers = game.offers(player);
        let deed = offers
            .iter()
            .copied()
            .find(|d| Some(d.patron()) == patron)
            .unwrap_or(offers[0]);
        return Intent::ChooseDeed { deed };
    }
    // Tonight's wish first, on its turn or when dusk waits for it (§21.4).
    let wish_now = game.may_wish(player) && (game.at_dusk().is_some() || game.free_to_act(player));
    if wish_now && game.to_answer(player).is_none() {
        return wish(game, player);
    }
    choose_turn(game, player)
}

/// Like `choose`, but leaves tonight's wish to someone else: a person
/// writes it while the bot plays their turn.
pub fn choose_turn(game: &Game, player: PlayerId) -> Intent {
    match game.to_answer(player) {
        Some(window) => respond(game, player, window.kind),
        None if game.free_to_act(player) => own_turn(game, player),
        // Not our move at all; the caller should not have asked.
        None => Intent::EndTurn,
    }
}

fn respond(game: &Game, player: PlayerId, kind: WindowKind) -> Intent {
    let play = |card: CardId, target: Target| Intent::Play { card, target };
    let cards = game.playable(player);
    match kind {
        WindowKind::Target { target, .. } if target == player => {
            // Cancel or ward against a card aimed at us.
            for card in cards {
                match game.def(card).effect {
                    Effect::Cancel => return play(card, Target::None),
                    Effect::Ward => return play(card, Target::Champion(player)),
                    _ => {}
                }
            }
        }
        WindowKind::Enter { mover, .. } if mover != player => {
            if let Some(intent) = strike(game, player, &cards, Some(mover)) {
                return intent;
            }
        }
        WindowKind::Battle { .. } => return burn(game, player),
        // Tribute: give the cheapest card while the hand is full enough or
        // the guard is near; else take the Threat.
        WindowKind::Tribute { .. } => {
            let hand = game.hand(player);
            let loud = game.threat(player) + 2 >= game.guard_threshold();
            if (hand.len() > 2 || loud)
                && let Some(&card) = hand.iter().min_by_key(|&&c| (game.def(c).cost, c))
            {
                return play(card, Target::None);
            }
        }
        WindowKind::Trial { .. } => return burn_for_trial(game, player),
        _ => {}
    }
    Intent::Pass
}

fn own_turn(game: &Game, player: PlayerId) -> Intent {
    // Ruins underfoot are a settlement waiting to be built again (§20.4).
    if game.can_rebuild(player) {
        return Intent::Rebuild;
    }
    if let Some(intent) = deed_work(game, player) {
        return intent;
    }
    if let Some(intent) = mechanic_play(game, player) {
        return intent;
    }
    let cards = game.playable(player);
    // Bodies first: they only work while standing on one.
    if let Some(&card) = cards.iter().find(|&&c| {
        game.targets(player, c).iter().any(|t| {
            matches!(t, Target::Hex(h) if *h == hex_of(game, player)) && is_body(game.def(c).effect)
        })
    }) {
        return Intent::Play {
            card,
            target: Target::Hex(hex_of(game, player)),
        };
    }
    // A dark god's item costs more than it gives: back to a god it goes.
    if let Some(slot) = crate::items::Slot::ALL.into_iter().find(|&s| {
        game.gear(player)[s.index()].is_some_and(|i| game.item_tolls(player, i))
            && game.can_sacrifice(player, s)
    }) {
        return Intent::Sacrifice { slot };
    }
    if let Some(intent) = mend(game, player, &cards) {
        return intent;
    }
    if let Some(intent) = strike(game, player, &cards, None) {
        return intent;
    }
    for &card in &cards {
        match game.def(card).effect {
            Effect::Draw(_) => {
                return Intent::Play {
                    card,
                    target: Target::Champion(player),
                };
            }
            Effect::Ward
                if enemy_near(game, player)
                    && game.champion(player).is_some_and(|c| c.ward.is_none()) =>
            {
                return Intent::Play {
                    card,
                    target: Target::Champion(player),
                };
            }
            Effect::Trap(_) if enemy_near(game, player) => {
                if let Some(&target) = game.targets(player, card).first() {
                    return Intent::Play { card, target };
                }
            }
            _ => {}
        }
    }
    if let Some(intent) = attack(game, player) {
        return intent;
    }
    walk(game, player)
}

/// Burn one card whose face would count right now, keeping two in hand.
fn burn(game: &Game, player: PlayerId) -> Intent {
    let day = game.time() == TimeOfDay::Day;
    let hand = game.hand(player);
    let max = game.battle_dice(player).unwrap_or(0) as usize;
    let cards: Vec<CardId> = if hand.len() >= 3 && max > 0 {
        hand.iter()
            .copied()
            .find(|&c| match game.def(c).burn_face() {
                Face::Strike | Face::Shield | Face::Element => true,
                Face::Sun => day,
                Face::Moon => !day,
                Face::Blank => false,
            })
            .into_iter()
            .collect()
    } else {
        Vec::new()
    };
    Intent::Burn { cards }
}

/// Burn cards whose faces count for the trial, up to what it asks,
/// keeping two in hand.
fn burn_for_trial(game: &Game, player: PlayerId) -> Intent {
    let Some(trial) = game.trial_of(player) else {
        return Intent::Burn { cards: Vec::new() };
    };
    let need = game.trial_need(trial) as usize;
    let hand = game.hand(player);
    let spare = hand
        .len()
        .saturating_sub(2)
        .min(game.battle_dice(player).unwrap_or(0) as usize);
    let cards: Vec<CardId> = hand
        .iter()
        .copied()
        .filter(|&c| game.trial_counts(trial, game.def(c).burn_face()))
        .take(need.min(spare))
        .collect();
    Intent::Burn { cards }
}

/// Attack a neighbour we are at least as healthy and strong as. The Dominant
/// is everyone's target (§6.1): worth a fight even when a little stronger.
fn attack(game: &Game, player: PlayerId) -> Option<Intent> {
    let me = game.champion(player)?;
    // Holding a quiet-crown wager: no fights.
    if game.lines_of(player).any(|l| l.goal == Goal::AvoidBattle) {
        return None;
    }
    // Mobs next to us, fought while we are hale: the restless dead at 3
    // health, a beast of the woods (stronger) at 4 (§20.4).
    if let Some(u) = game
        .mobs()
        .iter()
        .filter(|u| game.attack_cost(player, u.hex).is_ok())
        .filter(|u| me.hp >= if u.is_beast() { 4 } else { 3 })
        .min_by_key(|u| (u.hp, u.id))
    {
        return Some(Intent::Move { to: u.hex });
    }
    // The guard hunting us, fought back while we are hale (§20.4).
    if let Some(guard) = game.guard()
        && guard.target == player
        && me.hp >= 3
        && game.attack_cost(player, guard.hex).is_ok()
    {
        return Some(Intent::Move { to: guard.hex });
    }
    let mut targets: Vec<(bool, Hex)> = game
        .attackable(player)
        .into_iter()
        .filter_map(|hex| {
            let who = game.occupant(hex)?;
            let foe = game.champion(who)?;
            let crowned = game.dominant() == Some(who);
            let slack = u8::from(crowned);
            (me.hp + slack >= foe.hp && me.might + slack >= foe.might).then_some((crowned, hex))
        })
        .collect();
    targets.sort_by_key(|&(crowned, h)| (!crowned, h.x(), h.y()));
    targets.first().map(|&(_, to)| Intent::Move { to })
}

/// A damaging or rooting card at the weakest rival in reach (or `only`).
fn strike(
    game: &Game,
    player: PlayerId,
    cards: &[CardId],
    only: Option<PlayerId>,
) -> Option<Intent> {
    let mut best: Option<(u8, CardId, PlayerId)> = None;
    for &card in cards {
        let def = game.def(card);
        if !matches!(
            def.effect,
            Effect::Damage(_)
                | Effect::Drain(_)
                | Effect::Finish(_)
                | Effect::Root
                | Effect::Poison(_)
        ) {
            continue;
        }
        for target in game.targets(player, card) {
            let Target::Champion(foe) = target else {
                continue;
            };
            if foe == player || only.is_some_and(|o| o != foe) {
                continue;
            }
            let c = game.champion(foe)?;
            // Skip wards this card cannot break, and finishers on the unhurt.
            if c.ward.is_some_and(|w| def.element != Some(w.quenched_by())) {
                continue;
            }
            if matches!(def.effect, Effect::Finish(_)) && c.hp == c.body {
                continue;
            }
            // Poison stacks up slowly and never takes the last health.
            if matches!(def.effect, Effect::Poison(_)) && (c.poison.is_some() || c.hp <= 1) {
                continue;
            }
            if best.is_none_or(|(hp, _, _)| c.hp < hp) {
                best = Some((c.hp, card, foe));
            }
        }
    }
    best.map(|(_, card, foe)| Intent::Play {
        card,
        target: Target::Champion(foe),
    })
}

/// Heal ourselves when hurt, or cure our poison; never feed it.
fn mend(game: &Game, player: PlayerId, cards: &[CardId]) -> Option<Intent> {
    let me = game.champion(player)?;
    let poison = me.poison.map(|p| p.element);
    let cures = |c: CardId| poison.is_some_and(|p| cards::cures(&game.def(c), p));
    let feeds = |c: CardId| poison.is_some_and(|p| game.def(c).element == Some(p.generated_by()));
    if me.hp * 2 > me.body && !cards.iter().any(|&c| cures(c)) {
        return None;
    }
    cards
        .iter()
        .find(|&&c| {
            matches!(game.def(c).effect, Effect::Heal(_))
                && (cures(c) || me.hp * 2 <= me.body)
                && !feeds(c)
                && game.targets(player, c).contains(&Target::Champion(player))
        })
        .map(|&card| Intent::Play {
            card,
            target: Target::Champion(player),
        })
}

fn walk(game: &Game, player: PlayerId) -> Intent {
    let Some(me) = game.champion(player) else {
        return Intent::EndTurn;
    };
    let target = game
        .lines_of(player)
        .find_map(|l| match l.goal {
            // A pilgrimage or a trial of our own beats any corpse.
            Goal::ReachHex(hex) => Some(hex),
            Goal::PassTrial(hex) if game.trial_for(player, hex).is_some() => Some(hex),
            // A tithe is prayed for at the god's temple, a turn at a time.
            Goal::Offer { god, .. } => Some(game.board().temple_of(god)),
            // New land: a settlement not yet ours.
            Goal::Claim => game
                .board()
                .land()
                .filter(|(h, t)| {
                    t.terrain == crate::Terrain::Settlement
                        && game.owner(*h) != Some(player)
                        && game.occupant(*h).is_none()
                })
                .map(|(h, _)| h)
                .min_by_key(|h| (h.unsigned_distance_to(me.hex), h.x(), h.y())),
            Goal::Body => game
                .board()
                .corpses()
                .map(|(hex, _)| hex)
                .filter(|&hex| game.occupant(hex).is_none_or(|p| p == player))
                .min_by_key(|&hex| (me.hex.unsigned_distance_to(hex), hex.x(), hex.y())),
            _ => None,
        })
        // A rival's treasury on its eve: to raid it.
        .or_else(|| {
            game.delves()
                .find(|(_, d)| d.owner != player && d.treasury && game.on_eve(d.owner))
                .map(|(h, _)| h)
        })
        // Called to a duel: to the one who called.
        .or_else(|| {
            game.duels()
                .iter()
                .find(|d| d.rival == player)
                .map(|d| hex_of(game, d.host))
        })
        // Where its deed is made.
        .or_else(|| deed_goal(game, player))
        // Something lying close by.
        .or_else(|| {
            game.ground_items()
                .iter()
                .map(|(hex, _)| *hex)
                .filter(|&hex| hex != me.hex && me.hex.unsigned_distance_to(hex) <= 3)
                .filter(|&hex| game.occupant(hex).is_none())
                .min_by_key(|&hex| (me.hex.unsigned_distance_to(hex), hex.x(), hex.y()))
        })
        // A trial close by, when healthy enough to risk its price.
        .or_else(|| {
            game.trials()
                .iter()
                .map(|t| t.hex)
                .filter(|&hex| game.trial_for(player, hex).is_some())
                .filter(|&hex| me.hex.unsigned_distance_to(hex) <= 3 && me.hp * 2 > me.body)
                .min_by_key(|&hex| (me.hex.unsigned_distance_to(hex), hex.x(), hex.y()))
        })
        .or_else(|| {
            game.board()
                .corpses()
                .map(|(hex, _)| hex)
                .filter(|&hex| game.occupant(hex).is_none_or(|p| p == player))
                .min_by_key(|&hex| (me.hex.unsigned_distance_to(hex), hex.x(), hex.y()))
        })
        .unwrap_or(Hex::ZERO);
    if target == me.hex {
        return Intent::EndTurn;
    }

    let here = me.hex.unsigned_distance_to(target);
    me.hex
        .all_neighbors()
        .into_iter()
        .filter(|&next| next.unsigned_distance_to(target) < here)
        .filter(|&next| {
            game.step_cost(player, next)
                .is_ok_and(|cost| cost <= game.move_points(player))
        })
        .min_by_key(|&next| (next.unsigned_distance_to(target), next.x(), next.y()))
        .map_or(Intent::EndTurn, |to| Intent::Move { to })
}

fn hex_of(game: &Game, player: PlayerId) -> Hex {
    game.champion(player).map_or(Hex::ZERO, |c| c.hex)
}

fn enemy_near(game: &Game, player: PlayerId) -> bool {
    let me = hex_of(game, player);
    game.players()
        .any(|p| p != player && hex_of(game, p).unsigned_distance_to(me) <= 3)
}

fn is_body(effect: Effect) -> bool {
    matches!(
        effect,
        Effect::BodyFuel
            | Effect::BodyLegion
            | Effect::BodyDissolve
            | Effect::BodyRest
            | Effect::BodySeed
    )
}

/// Tonight's wish: what its deed needs next (§21.10), else the best-graded
/// plain wish, from its own patron when tied; at the leader in Style when
/// it needs a rival.
fn wish(game: &Game, player: PlayerId) -> Intent {
    if let Some(intent) = deed_wish(game, player) {
        return intent;
    }
    let patron = game.champion(player).map(|c| c.god);
    let mut best: Option<(u8, bool, God, WishKind)> = None;
    for god in God::ALL {
        for kind in WishKind::ALL.into_iter().filter(|k| !k.is_crude()) {
            let score = (game.wish_grade(god, kind), Some(god) == patron);
            if best.is_none_or(|(g, p, _, _)| score > (g, p)) {
                best = Some((score.0, score.1, god, kind));
            }
        }
    }
    let Some((_, _, god, kind)) = best else {
        return Intent::RefuseWish;
    };
    let target = kind.needs_target().then(|| {
        game.players()
            .filter(|&p| p != player)
            .max_by_key(|&p| (game.style(p), p.0))
            .expect("a rival")
    });
    let Some(act) = crate::Act::of(kind, target) else {
        return Intent::RefuseWish;
    };
    Intent::Wish {
        god,
        wish: crate::Wish::one(act),
        said: None,
    }
}

/// A wish towards the bot's Great Deed, if its plan has one now (§21.10):
/// the first mechanic the deed needs that the world lacks, else what its
/// next unmet step asks for.
fn deed_wish(game: &Game, player: PlayerId) -> Option<Intent> {
    let wish = |god: God, act: crate::Act| Intent::Wish {
        god,
        wish: crate::Wish::one(act),
        said: None,
    };
    // A rival's deed on its eve is broken first (§21.10): mist round them
    // shrinks an island and takes the woods from a tree; new land in the
    // region they dissolve makes it bigger than the mist.
    if let Some(rival) = game.players().find(|&r| r != player && game.on_eve(r)) {
        return Some(match game.deed(rival)? {
            GreatDeed::DissolvedLand => {
                wish(game.dissolving(rival).0, crate::Act::Rise { terrain: None })
            }
            GreatDeed::Island
            | GreatDeed::WorldTree
            | GreatDeed::City
            | GreatDeed::Reconciliation
            | GreatDeed::River
            | GreatDeed::Roads
            | GreatDeed::GreatFire
            | GreatDeed::Feast
            | GreatDeed::FairOfFive
            | GreatDeed::DeadFeast
            | GreatDeed::TripleUnion
            | GreatDeed::FallenEmpire
            | GreatDeed::Necropolis
            | GreatDeed::PlaguePit
            | GreatDeed::Summoning
            | GreatDeed::Dragon
            | GreatDeed::Guest
            | GreatDeed::Ark
            | GreatDeed::WalkingForest
            | GreatDeed::Arena
            | GreatDeed::DebtBondage
            | GreatDeed::DeadBall
            | GreatDeed::Treasury
            | GreatDeed::Amazon => wish(
                God::Maya,
                crate::Act::Veil {
                    target: Some(rival),
                },
            ),
            // Zaga's mountains drain the lake round it.
            GreatDeed::FloodedTable => wish(God::Zaga, crate::Act::Land),
            // A legion thins by a god's hand.
            GreatDeed::Legion => wish(God::Ahamar, crate::Act::Weaken { target: rival }),
        });
    }
    let deed = game.deed(player)?;
    if let Some(&missing) = deed.needs().iter().find(|&&f| !game.has(f))
        && game.can_awaken(missing)
    {
        return Some(wish(
            missing.domain(),
            crate::Act::Awaken {
                feature: Some(missing),
            },
        ));
    }
    let unmet = game
        .checks(player, deed)
        .into_iter()
        .find(|c| !c.met())?
        .kind;
    use crate::CheckKind::*;
    Some(match (deed, unmet) {
        // Its ground cut off from the world, then a settlement of its own there.
        (GreatDeed::Island, IslandSize) => wish(God::Maya, crate::Act::Cut),
        (GreatDeed::Island, IslandSettled) => wish(God::Trishna, crate::Act::Settle),
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
                wish(god, crate::Act::Rise { terrain: None })
            } else if near {
                wish(God::Maya, crate::Act::Veil { target: None })
            } else {
                let rival = game
                    .players()
                    .find(|&p| p != player && region_of(p) == Some(god))?;
                wish(
                    God::Maya,
                    crate::Act::Veil {
                        target: Some(rival),
                    },
                )
            }
        }
        // No undead to enlist: bodies near, to rise.
        (GreatDeed::Legion, LegionSize) if !game.mobs().iter().any(|m| m.is_undead()) => {
            wish(God::Zaga, crate::Act::Dead)
        }
        (GreatDeed::River, RiverLength | RiverSource | RiverMouth) => {
            wish(God::Maya, crate::Act::River)
        }
        (GreatDeed::FloodedTable, TableFlooded) => wish(God::Maya, crate::Act::Flood),
        (GreatDeed::Roads, TemplesLinked) => wish(God::Ahamar, crate::Act::Road),
        (GreatDeed::GreatFire, RegionsBurnt | FireBurning) => wish(God::Trishna, crate::Act::Fire),
        (GreatDeed::Amazon, JungleWoods) => wish(God::Bhava, crate::Act::Land),
        (GreatDeed::Amazon, JungleRiver) => wish(God::Maya, crate::Act::River),
        // Bhava's woods round where it stands.
        (GreatDeed::WorldTree, WoodsAround) => wish(God::Bhava, crate::Act::Land),
        // Groves round it to wake.
        (GreatDeed::WalkingForest, GroveRooted)
            if !game
                .board()
                .land()
                .any(|(_, t)| t.terrain == crate::Terrain::Grove) =>
        {
            wish(God::Bhava, crate::Act::Land)
        }
        // A god of the pair out of its light: an offering to the god that
        // quenches it cools it (§5.1).
        (GreatDeed::Reconciliation, PairLight) => {
            let dark = God::ALL
                .into_iter()
                .find(|&g| game.stage(g) > 0 && game.stage(God::from_index(g.index() + 2)) == 0)?;
            wish(God::from_index(dark.index() + 3), crate::Act::Peace)
        }
        _ => return None,
    })
}

/// Where the bot's deed wants it to stand: in the region it dissolves, on
/// the grove of its World Tree.
fn deed_goal(game: &Game, player: PlayerId) -> Option<Hex> {
    match game.deed(player)? {
        GreatDeed::DissolvedLand => game.left_to_dissolve(player),
        GreatDeed::WorldTree => game.hero_grove(),
        GreatDeed::Island => None,
        // By the river's end, to run it on.
        GreatDeed::River => {
            let me = game.champion(player)?.hex;
            game.river_head(me)
                .filter(|h| h.unsigned_distance_to(me) > 2)
        }
        // Two hexes from the Table, where a spring runs towards it.
        GreatDeed::FloodedTable => {
            let me = game.champion(player)?.hex;
            if me.ulength() == 2 {
                return None;
            }
            Hex::ZERO
                .ring(2)
                .filter(|&h| game.board().contains(h) && game.occupant(h).is_none())
                .min_by_key(|h| (h.unsigned_distance_to(me), h.x(), h.y()))
        }
        GreatDeed::Amazon => None,
        GreatDeed::Roads => None,
        // Stones for a circle, bodies to feed it, then its monster.
        GreatDeed::Summoning => {
            let me = game.champion(player)?.hex;
            let nearest = |hexes: Vec<Hex>| {
                hexes
                    .into_iter()
                    .min_by_key(|h| (h.unsigned_distance_to(me), h.x(), h.y()))
            };
            if let Some(m) = game.mobs().iter().find(|m| {
                matches!(m.kind, crate::MobKind::Monster { summoner, .. } if summoner == Some(player))
            }) {
                return Some(m.hex);
            }
            match game.circles().find(|(_, c)| c.owner == player) {
                Some((circle, _))
                    if matches!(game.cargo(player), Some(crate::Cargo::Body { .. })) =>
                {
                    Some(circle)
                }
                Some(_) => nearest(game.board().corpses().map(|(h, _)| h).collect()),
                None => nearest(
                    game.board()
                        .land()
                        .filter(|(h, t)| {
                            t.terrain == crate::Terrain::Stones && game.circle(*h).is_none()
                        })
                        .map(|(h, _)| h)
                        .collect(),
                ),
            }
        }
        // A mountain trial for the egg, woods to lay it in, beside it to set
        // them alight.
        GreatDeed::Dragon => {
            let me = game.champion(player)?.hex;
            let nearest = |hexes: Vec<Hex>| {
                hexes
                    .into_iter()
                    .min_by_key(|h| (h.unsigned_distance_to(me), h.x(), h.y()))
            };
            let woods = || {
                game.board()
                    .land()
                    .filter(|(h, t)| t.terrain.burns() && game.occupant(*h).is_none() && *h != me)
                    .map(|(h, _)| h)
                    .collect::<Vec<_>>()
            };
            if matches!(game.cargo(player), Some(crate::Cargo::Egg { .. })) {
                return nearest(woods());
            }
            if let Some((egg, _)) = game
                .loads()
                .iter()
                .find(|(_, c)| matches!(c, crate::Cargo::Egg { by, .. } if *by == Some(player)))
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
                } else {
                    Some(*egg)
                };
            }
            nearest(
                game.trials()
                    .iter()
                    .filter(|t| {
                        game.trial_for(player, t.hex).is_some()
                            && game
                                .board()
                                .tile(t.hex)
                                .is_some_and(|x| x.terrain == crate::Terrain::Mountain)
                    })
                    .map(|t| t.hex)
                    .collect(),
            )
        }
        // An arena at home, and there to wait for the challenged.
        GreatDeed::Arena => {
            let me = game.champion(player)?.hex;
            let arena = game
                .buildings()
                .find(|&(h, b)| b == crate::Building::Arena && game.owner(h) == Some(player))
                .map(|(h, _)| h);
            match arena {
                Some(a) => (a != me).then_some(a),
                None => game
                    .claims()
                    .filter(|&(h, p)| p == player && game.building(h).is_none() && h != me)
                    .map(|(h, _)| h)
                    .min_by_key(|h| (h.unsigned_distance_to(me), h.x(), h.y())),
            }
        }
        GreatDeed::DebtBondage => None,
        // Its way down, else ruins to open one under.
        GreatDeed::Treasury => {
            let me = game.champion(player)?.hex;
            if let Some((h, _)) = game.delves().find(|(_, d)| d.owner == player) {
                return (h != me).then_some(h);
            }
            game.board()
                .land()
                .filter(|(h, t)| {
                    t.terrain == crate::Terrain::Ruins && game.delve(*h).is_none() && *h != me
                })
                .map(|(h, _)| h)
                .min_by_key(|h| (h.unsigned_distance_to(me), h.x(), h.y()))
        }
        // A grove to wake, while none of its own walks.
        GreatDeed::WalkingForest => {
            if game.walkers().any(|(_, p)| p == player) {
                return None;
            }
            let me = game.champion(player)?.hex;
            game.board()
                .land()
                .filter(|(h, t)| t.terrain == crate::Terrain::Grove && *h != me)
                .map(|(h, _)| h)
                .min_by_key(|h| (h.unsigned_distance_to(me), h.x(), h.y()))
        }
        // Home to build the pen, beasts of the elements it lacks, the pen to
        // tether them in.
        GreatDeed::Ark => {
            let me = game.champion(player)?.hex;
            let nearest = |hexes: Vec<Hex>| {
                hexes
                    .into_iter()
                    .filter(|h| *h != me)
                    .min_by_key(|h| (h.unsigned_distance_to(me), h.x(), h.y()))
            };
            let pen = game
                .buildings()
                .find(|&(h, b)| b == crate::Building::Pen && game.owner(h) == Some(player))
                .map(|(h, _)| h);
            let Some(pen) = pen else {
                return nearest(
                    game.claims()
                        .filter(|&(h, p)| p == player && game.building(h).is_none())
                        .map(|(h, _)| h)
                        .collect(),
                );
            };
            let lacks = |e: crate::Element| game.penned(pen) & (1 << e.index()) == 0;
            if game
                .companions(player)
                .iter()
                .any(|c| matches!(c, crate::Companion::Beast(e) if lacks(*e)))
            {
                return (me != pen).then_some(pen);
            }
            nearest(
                game.mobs()
                    .iter()
                    .filter(|m| m.is_beast() && lacks(game.beast_element(m)))
                    .flat_map(|m| m.hex.all_neighbors())
                    .filter(|&h| game.board().contains(h) && game.occupant(h).is_none())
                    .collect(),
            )
        }
        // The stranger, then the Table.
        GreatDeed::Guest => {
            if game.companions(player).contains(&crate::Companion::Guest) {
                return Some(Hex::ZERO);
            }
            let me = game.champion(player)?.hex;
            game.mobs()
                .iter()
                .find(|m| matches!(m.kind, crate::MobKind::Guest))
                .and_then(|m| {
                    m.hex
                        .all_neighbors()
                        .into_iter()
                        .filter(|&h| {
                            game.board().contains(h) && game.occupant(h).is_none_or(|p| p == player)
                        })
                        .min_by_key(|h| (h.unsigned_distance_to(me), h.x(), h.y()))
                })
        }
        // Ground to consecrate by the graveyard, bodies to bring to it or the
        // pit, the dead to clear from Zaga's land.
        GreatDeed::Necropolis | GreatDeed::PlaguePit => {
            let me = game.champion(player)?.hex;
            let nearest = |hexes: Vec<Hex>| {
                hexes
                    .into_iter()
                    .min_by_key(|h| (h.unsigned_distance_to(me), h.x(), h.y()))
            };
            let ground = |t: crate::Terrain| {
                game.board()
                    .land()
                    .filter(move |(_, x)| x.terrain == t)
                    .map(|(h, _)| h)
            };
            let carrying = matches!(game.cargo(player), Some(crate::Cargo::Body { .. }));
            if game.deed(player) == Some(GreatDeed::PlaguePit) {
                let pit = game.pits().find(|(_, p)| p.owner == player).map(|(h, _)| h);
                return match pit {
                    Some(pit) if carrying || game.may_settle_pit(player) => Some(pit),
                    _ => nearest(game.board().corpses().map(|(h, _)| h).collect()),
                };
            }
            let (size, _) = game.best_necropolis();
            if size < crate::NECROPOLIS {
                let yard: Vec<Hex> = ground(crate::Terrain::Graveyard).collect();
                return nearest(
                    game.board()
                        .land()
                        .filter(|(h, t)| {
                            matches!(t.terrain, crate::Terrain::Plains | crate::Terrain::Ash)
                                && (yard.is_empty()
                                    || h.all_neighbors().iter().any(|n| yard.contains(n)))
                                && game.occupant(*h).is_none_or(|p| p == player)
                        })
                        .map(|(h, _)| h)
                        .collect(),
                );
            }
            if carrying {
                return nearest(ground(crate::Terrain::Graveyard).collect());
            }
            if !game.zaga_land_quiet() {
                return nearest(
                    game.mobs()
                        .iter()
                        .filter(|m| {
                            m.is_undead()
                                && game.board().tile(m.hex).and_then(|t| t.region)
                                    == Some(God::Zaga)
                        })
                        .map(|m| m.hex)
                        .collect(),
                );
            }
            nearest(game.board().corpses().map(|(h, _)| h).collect())
        }
        // Rulers to win over; for the empire the Table once three are sworn,
        // then a vassal to set against the rest.
        GreatDeed::TripleUnion | GreatDeed::FallenEmpire => {
            let me = game.champion(player)?.hex;
            let nearest = |hexes: Vec<Hex>| {
                hexes
                    .into_iter()
                    .filter(|h| *h != me)
                    .min_by_key(|h| (h.unsigned_distance_to(me), h.x(), h.y()))
            };
            if game.deed(player) == Some(GreatDeed::FallenEmpire) {
                if game.emperor() == Some(player) {
                    return nearest(game.vassals(player));
                }
                if game.vassals(player).len() >= crate::CROWN_VASSALS && !game.crowned(player) {
                    return (me != Hex::ZERO).then_some(Hex::ZERO);
                }
            }
            nearest(
                game.rulers()
                    .filter(|(_, r)| {
                        r.spouse.is_none()
                            && r.sworn != Some(player)
                            && r.favourite() != Some(player)
                    })
                    .map(|(h, _)| h)
                    .collect(),
            )
        }
        // Its fair: open one at home, then bring the goods it lacks; for the
        // dead, bide at the fair and let them come.
        GreatDeed::FairOfFive | GreatDeed::DeadFeast => {
            let me = game.champion(player)?.hex;
            let mine = game.fairs().find(|(_, f)| f.host == player);
            let own_town = game
                .claims()
                .filter(|&(h, p)| {
                    p == player
                        && game
                            .board()
                            .tile(h)
                            .is_some_and(|t| t.terrain == crate::Terrain::Settlement)
                })
                .map(|(h, _)| h)
                .min_by_key(|h| (h.unsigned_distance_to(me), h.x(), h.y()));
            let Some((fair, f)) = mine else {
                return own_town.or_else(|| {
                    game.board()
                        .land()
                        .filter(|(h, t)| {
                            t.terrain == crate::Terrain::Settlement && game.owner(*h).is_none()
                        })
                        .map(|(h, _)| h)
                        .min_by_key(|h| (h.unsigned_distance_to(me), h.x(), h.y()))
                });
            };
            if game.deed(player) == Some(GreatDeed::DeadFeast) {
                // Near enough the fair to keep the living from it, not in the dead's way.
                return (me.unsigned_distance_to(fair) > 3).then_some(fair);
            }
            if matches!(game.cargo(player), Some(crate::Cargo::Goods(g)) if !f.has(g)) {
                return Some(fair);
            }
            game.loads()
                .iter()
                .filter(|(h, c)| {
                    matches!(c, crate::Cargo::Goods(g) if !f.has(*g))
                        && game.occupant(*h).is_none_or(|p| p == player)
                })
                .map(|(h, _)| *h)
                .min_by_key(|h| (h.unsigned_distance_to(me), h.x(), h.y()))
        }
        // Sow, carry the harvest home, wait at the hall for guests.
        GreatDeed::Feast | GreatDeed::DeadBall => {
            let me = game.champion(player)?.hex;
            let near = |hexes: Vec<Hex>| {
                hexes
                    .into_iter()
                    .min_by_key(|h| (h.unsigned_distance_to(me), h.x(), h.y()))
            };
            let own: Vec<Hex> = game
                .claims()
                .filter(|&(h, p)| {
                    p == player
                        && game
                            .board()
                            .tile(h)
                            .is_some_and(|t| t.terrain == crate::Terrain::Settlement)
                })
                .map(|(h, _)| h)
                .collect();
            if let Some(hall) = game.feast_hall(player) {
                return Some(hall);
            }
            if game.cargo(player) == Some(crate::Cargo::Food) {
                return near(own);
            }
            if own.is_empty() {
                return near(
                    game.board()
                        .land()
                        .filter(|(h, t)| {
                            t.terrain == crate::Terrain::Settlement && game.owner(*h).is_none()
                        })
                        .map(|(h, _)| h)
                        .collect(),
                );
            }
            if game.fields_of(player) < crate::FEAST_FIELDS {
                return near(
                    own.iter()
                        .flat_map(|h| h.all_neighbors())
                        .filter(|&h| {
                            game.board()
                                .tile(h)
                                .is_some_and(|t| t.terrain == crate::Terrain::Plains)
                                && game.occupant(h).is_none()
                        })
                        .collect(),
                );
            }
            near(
                game.loads()
                    .iter()
                    .filter(|(h, c)| *c == crate::Cargo::Food && game.occupant(*h).is_none())
                    .map(|(h, _)| *h)
                    .collect(),
            )
        }
        // Woods in a region its fire has not passed yet.
        GreatDeed::GreatFire => {
            let me = game.champion(player)?.hex;
            game.board()
                .land()
                .filter(|(h, t)| {
                    t.terrain.burns()
                        && *h != me
                        && t.region.is_some_and(|g| !game.burnt_by(player, g))
                })
                .map(|(h, _)| h)
                .min_by_key(|h| (h.unsigned_distance_to(me), h.x(), h.y()))
        }
        // The nearest undead to write into the legion.
        GreatDeed::Legion => {
            let me = game.champion(player)?.hex;
            game.mobs()
                .iter()
                .filter(|m| m.is_undead())
                .map(|m| m.hex)
                .min_by_key(|h| (h.unsigned_distance_to(me), h.x(), h.y()))
        }
        // A settlement of its own to build on, or one to take, but only
        // with the Spirit to build there, else it would idle on it.
        GreatDeed::City | GreatDeed::Reconciliation => {
            let me = game.champion(player)?;
            let need = match game.deed(player)? {
                GreatDeed::City => crate::QUARTER_SPIRIT,
                _ if game.buildings().any(|(h, b)| {
                    border_pair(game, h).is_some_and(|p| b == crate::Building::Shrine(p))
                }) =>
                {
                    return None;
                }
                _ => crate::BUILD_SPIRIT,
            };
            if me.spirit_points < need {
                return None;
            }
            let me = me.hex;
            game.board()
                .land()
                // Not where it stands: deed_work found nothing to do there.
                .filter(|(h, t)| {
                    *h != me
                        && t.terrain == crate::Terrain::Settlement
                        && game.owner(*h).is_none_or(|o| o == player)
                        && if game.deed(player) == Some(GreatDeed::City) {
                            game.building(*h).is_none() || room_to_grow(game, *h)
                        } else {
                            game.building(*h).is_none() && border_pair(game, *h).is_some()
                        }
                })
                .map(|(h, _)| h)
                .min_by_key(|h| (h.unsigned_distance_to(me), h.x(), h.y()))
        }
    }
}

/// Free open land next to `hex` for a new quarter.
fn room_to_grow(game: &Game, hex: Hex) -> bool {
    hex.all_neighbors().iter().any(|&n| {
        game.board().tile(n).is_some_and(|t| {
            matches!(
                t.terrain,
                crate::Terrain::Plains
                    | crate::Terrain::Forest
                    | crate::Terrain::Grove
                    | crate::Terrain::Ruins
            ) && t.corpse.is_none()
        }) && game.occupant(n).is_none()
    })
}

/// The quenching pair whose lands meet at `hex`, if any: its own god first.
fn border_pair(game: &Game, hex: Hex) -> Option<[God; 2]> {
    let own = game.board().tile(hex)?.region?;
    hex.all_neighbors()
        .iter()
        .filter_map(|&n| game.board().tile(n).and_then(|t| t.region))
        .find(|&g| g.index() == (own.index() + 2) % 5 || own.index() == (g.index() + 2) % 5)
        .map(|g| [own, g])
}

/// A move on its own turn for the bot's deed, where it stands (§21.10):
/// a quarter for its city, what the city lacks, a shrine of two.
fn deed_work(game: &Game, player: PlayerId) -> Option<Intent> {
    // A rival's Great Fire on its eve: put out what of it is near.
    let spirit_now = game.champion(player)?.spirit_points;
    if spirit_now >= crate::DOUSE_SPIRIT
        && let Some(hex) = game.dousable(player).into_iter().find(|&h| {
            game.fire(h).and_then(|f| f.by).is_some_and(|by| {
                by != player && game.on_eve(by) && game.deed(by) == Some(GreatDeed::GreatFire)
            })
        })
    {
        return Some(Intent::Douse { hex });
    }
    // A treasury it can take, it takes; its own, it works on.
    let work = game.delve_work(player);
    let here = game.champion(player)?.hex;
    if work.contains(&crate::DelveWork::Raid)
        && let Some(d) = game.delve(here)
        && game.champion(player)?.might + companions_worth(game, player) > d.guards + 1
    {
        return Some(Intent::Delve {
            work: crate::DelveWork::Raid,
        });
    }
    if game.deed(player) == Some(GreatDeed::Treasury)
        && let Some(&w) = [
            crate::DelveWork::Treasury,
            crate::DelveWork::Dig,
            crate::DelveWork::Open,
            crate::DelveWork::Guard,
        ]
        .iter()
        .find(|w| work.contains(w))
    {
        return Some(Intent::Delve { work: w });
    }
    // A debt it can pay, it pays.
    if let Some(d) = game
        .debts()
        .iter()
        .find(|d| d.debtor == player && game.style(player) >= u16::from(d.amount))
    {
        return Some(Intent::PayDebt {
            creditor: d.creditor,
        });
    }
    match game.deed(player) {
        Some(GreatDeed::Arena) => {
            let can = game.may_build(player);
            let arena = game
                .buildings()
                .any(|(h, b)| b == crate::Building::Arena && game.owner(h) == Some(player));
            if !arena && can.contains(&crate::Building::Arena) && spirit_now >= crate::BUILD_SPIRIT
            {
                return Some(Intent::Build {
                    building: crate::Building::Arena,
                });
            }
            let me = game.champion(player)?.hex;
            if let Some(rival) = game
                .challengeable(player)
                .into_iter()
                .min_by_key(|&p| (hex_of(game, p).unsigned_distance_to(me), p.0))
            {
                return Some(Intent::Challenge { rival });
            }
        }
        Some(GreatDeed::DebtBondage) => {
            if let Some(&rival) = game.bettable(player).first() {
                return Some(Intent::BetOn {
                    rival,
                    bet: crate::Bet::Fight,
                });
            }
        }
        _ => {}
    }
    // Its walking grove: «Дикий энт» on the grove nearest the Table.
    if game.deed(player) == Some(GreatDeed::WalkingForest) {
        for card in game.playable(player) {
            if game.def(card).effect != Effect::Ent {
                continue;
            }
            if let Some(target) = game
                .targets(player, card)
                .into_iter()
                .min_by_key(|t| match t {
                    Target::Hex(h) => (h.ulength(), h.x(), h.y()),
                    _ => (u32::MAX, 0, 0),
                })
            {
                return Some(Intent::Play { card, target });
            }
        }
    }
    // Its Ark: a pen at home, a shrine near it, beasts tethered.
    if game.deed(player) == Some(GreatDeed::Ark) {
        let can = game.may_build(player);
        let pens = game
            .buildings()
            .any(|(h, b)| b == crate::Building::Pen && game.owner(h) == Some(player));
        if let Some(&element) = game.tetherable(player).first() {
            return Some(Intent::Tether { element });
        }
        if spirit_now >= crate::BUILD_SPIRIT {
            if !pens && can.contains(&crate::Building::Pen) {
                return Some(Intent::Build {
                    building: crate::Building::Pen,
                });
            }
            if pens
                && !game.best_ark(player).1
                && let Some(&b) = can.iter().find(|b| matches!(b, crate::Building::Shrine(_)))
            {
                return Some(Intent::Build { building: b });
            }
        }
    }
    // Its circle: draw it, feed it.
    if game.deed(player) == Some(GreatDeed::Summoning) {
        let here = game.champion(player)?.hex;
        let carrying = matches!(game.cargo(player), Some(crate::Cargo::Body { .. }));
        let mine = game
            .circles()
            .find(|(_, c)| c.owner == player)
            .map(|(h, _)| h);
        if mine.is_none() && game.may_draw_circle(player) && spirit_now >= crate::CIRCLE_SPIRIT {
            return Some(Intent::DrawCircle);
        }
        if carrying && mine == Some(here) {
            return Some(Intent::Lay);
        }
        if mine.is_some()
            && !carrying
            && matches!(game.takeable(player), Some(crate::Cargo::Body { .. }))
        {
            return Some(Intent::Take);
        }
    }
    // Its egg: laid in the woods, the woods set alight, taken on when burnt.
    if game.deed(player) == Some(GreatDeed::Dragon) {
        let here = game.champion(player)?.hex;
        let burns = |h: Hex| game.board().tile(h).is_some_and(|t| t.terrain.burns());
        if matches!(game.cargo(player), Some(crate::Cargo::Egg { .. })) && burns(here) {
            return Some(Intent::Lay);
        }
        if let Some((egg, _)) = game
            .loads()
            .iter()
            .find(|(_, c)| matches!(c, crate::Cargo::Egg { by, .. } if *by == Some(player)))
        {
            if *egg == here && !burns(here) && game.takeable(player).is_some() {
                return Some(Intent::Take);
            }
            if game.kindleable(player).contains(egg)
                && game.fire(*egg).is_none()
                && spirit_now >= crate::KINDLE_SPIRIT
            {
                return Some(Intent::Kindle { hex: *egg });
            }
        }
    }
    // Its graveyard or its pit: consecrate, dig, bury, settle.
    if matches!(
        game.deed(player),
        Some(GreatDeed::Necropolis | GreatDeed::PlaguePit)
    ) {
        let here = game.champion(player)?.hex;
        let terrain = game.board().tile(here).map(|t| t.terrain);
        let carrying = matches!(game.cargo(player), Some(crate::Cargo::Body { .. }));
        if game.may_settle_pit(player) {
            return Some(Intent::SettlePit { raise: false });
        }
        let pit_of_mine = game.pit(here).is_some_and(|p| p.owner == player);
        if carrying && (terrain == Some(crate::Terrain::Graveyard) || pit_of_mine) {
            return Some(Intent::Lay);
        }
        if !carrying && matches!(game.takeable(player), Some(crate::Cargo::Body { .. })) {
            return Some(Intent::Take);
        }
        if game.may_consecrate(player) && spirit_now >= crate::CONSECRATE_SPIRIT {
            if game.deed(player) == Some(GreatDeed::PlaguePit) {
                if !game.pits().any(|(_, p)| p.owner == player) {
                    return Some(Intent::DigPit);
                }
            } else {
                let yard: Vec<Hex> = game
                    .board()
                    .land()
                    .filter(|(_, t)| t.terrain == crate::Terrain::Graveyard)
                    .map(|(h, _)| h)
                    .collect();
                let beside =
                    yard.is_empty() || here.all_neighbors().iter().any(|n| yard.contains(n));
                if beside && game.best_necropolis().0 < crate::NECROPOLIS {
                    return Some(Intent::Consecrate);
                }
            }
        }
    }
    // A rival's Fallen Empire on its eve: a gift makes peace.
    if spirit_now >= 1
        && let Some(hex) = game.giftable(player).into_iter().find(|&h| {
            game.ruler(h)
                .and_then(|r| r.feud)
                .is_some_and(|by| by != player && game.on_eve(by))
        })
    {
        return Some(Intent::Gift { hex });
    }
    match game.deed(player) {
        Some(GreatDeed::FallenEmpire) if game.may_sow_discord(player) => {
            return Some(Intent::Discord);
        }
        Some(GreatDeed::FallenEmpire) if game.may_crown(player) && !game.crowned(player) => {
            return Some(Intent::Coronation);
        }
        Some(GreatDeed::TripleUnion) => {
            // A match that adds a land to the house it has, else any.
            let wed: Vec<God> = game
                .rulers()
                .filter(|(_, r)| r.spouse.is_some_and(|(_, by)| by == player))
                .filter_map(|(h, _)| game.board().tile(h)?.region)
                .collect();
            let region = |h: Hex| game.board().tile(h).and_then(|t| t.region);
            let matches = game.matches(player);
            let best = matches
                .iter()
                .find(|&&(a, b)| {
                    region(a).is_some_and(|g| wed.contains(&g))
                        != region(b).is_some_and(|g| wed.contains(&g))
                })
                .or_else(|| wed.is_empty().then(|| matches.first()).flatten());
            if let Some(&(a, b)) = best {
                return Some(Intent::Betroth { a, b });
            }
        }
        _ => {}
    }
    if matches!(
        game.deed(player),
        Some(GreatDeed::TripleUnion | GreatDeed::FallenEmpire)
    ) && game.emperor() != Some(player)
        && let Some(hex) = game.giftable(player).into_iter().find(|&h| {
            game.ruler(h)
                .is_some_and(|r| r.spouse.is_none() && r.sworn != Some(player) && r.feud.is_none())
        })
        && (spirit_now >= 1
            || matches!(
                game.cargo(player),
                Some(crate::Cargo::Food | crate::Cargo::Goods(_))
            ))
    {
        return Some(Intent::Gift { hex });
    }
    // Its fair: open it at home; bring the goods it lacks.
    if matches!(
        game.deed(player),
        Some(GreatDeed::FairOfFive | GreatDeed::DeadFeast)
    ) {
        let mine = game.fairs().find(|(_, f)| f.host == player);
        if mine.is_none() && game.may_open_fair(player) && spirit_now >= crate::FAIR_SPIRIT {
            return Some(Intent::Fair);
        }
        if let Some((fair, f)) = mine {
            let here = game.champion(player)?.hex;
            match game.cargo(player) {
                Some(crate::Cargo::Goods(g)) if here == fair && !f.has(g) => {
                    return Some(Intent::Lay);
                }
                None if matches!(game.takeable(player), Some(crate::Cargo::Goods(g)) if !f.has(g)) =>
                {
                    return Some(Intent::Take);
                }
                _ => {}
            }
        }
    }
    // Its own Feast: a feast when the guests are there, the harvest carried
    // home, fields sown by its settlements.
    if matches!(
        game.deed(player),
        Some(GreatDeed::Feast | GreatDeed::DeadBall)
    ) {
        // A ball wants the night, the dead near and the militia at the gate.
        let here = game.champion(player)?.hex;
        let ball_ready = game.time() == crate::TimeOfDay::Night
            && game.militia_at(here).is_some()
            && game
                .mobs()
                .iter()
                .any(|m| m.is_undead() && m.hex.unsigned_distance_to(here) <= crate::GUEST_RANGE);
        if game.may_feast(player)
            && game.guests(player).len() >= crate::FEAST_GUESTS
            && (game.deed(player) == Some(GreatDeed::Feast) || ball_ready)
        {
            return Some(Intent::Feast);
        }
        let here = game.champion(player)?.hex;
        if game.cargo(player) == Some(crate::Cargo::Food)
            && game.owner(here) == Some(player)
            && game
                .board()
                .tile(here)
                .is_some_and(|t| t.terrain == crate::Terrain::Settlement)
        {
            return Some(Intent::Lay);
        }
        if game.takeable(player) == Some(crate::Cargo::Food) {
            return Some(Intent::Take);
        }
        if game.may_sow(player)
            && game.fields_of(player) < crate::FEAST_FIELDS
            && spirit_now >= crate::SOW_SPIRIT
        {
            return Some(Intent::Sow);
        }
    }
    // Its own Great Fire: set a region alight that its fire has not passed,
    // or keep one burning once four are.
    if game.deed(player) == Some(GreatDeed::GreatFire) && spirit_now >= crate::KINDLE_SPIRIT {
        let mine = game.fires().any(|(_, f)| f.by == Some(player));
        let enough = game.regions_burnt(player) >= crate::GREAT_FIRE;
        let fresh = |h: &Hex| {
            game.board()
                .tile(*h)
                .and_then(|t| t.region)
                .is_some_and(|g| !game.burnt_by(player, g))
        };
        if let Some(hex) = game
            .kindleable(player)
            .into_iter()
            .find(|h| (enough && !mine) || (!enough && fresh(h)))
        {
            return Some(Intent::Kindle { hex });
        }
    }
    // A companion next to it: an undead for a legion, a beast when Spirit
    // is to spare.
    let spare = game.champion(player)?.spirit_points >= crate::TAME_SPIRIT + 2;
    if let Some(mob) = game.recruitable(player).into_iter().find(|&id| {
        game.mobs().iter().any(|m| {
            m.id == id
                && match m.kind {
                    crate::MobKind::Undead => game.deed(player) == Some(GreatDeed::Legion),
                    crate::MobKind::Guest => game.deed(player) == Some(GreatDeed::Guest),
                    crate::MobKind::Beast { .. } if game.deed(player) == Some(GreatDeed::Ark) => {
                        !game
                            .companions(player)
                            .iter()
                            .any(|c| *c == crate::Companion::Beast(game.beast_element(m)))
                    }
                    _ => spare,
                }
        })
    }) {
        return Some(Intent::Recruit { mob });
    }
    let deed = game.deed(player)?;
    let spirit = game.champion(player)?.spirit_points;
    let here = game.champion(player)?.hex;
    let can = game.may_build(player);
    match deed {
        GreatDeed::City => {
            let city = game.city_of(here);
            let has = |f: fn(&crate::Building) -> bool| {
                city.iter()
                    .any(|&h| game.building(h).is_some_and(|b| f(&b)))
            };
            let want = [
                (
                    crate::Building::Tavern,
                    has(|b| *b == crate::Building::Tavern),
                ),
                (
                    crate::Building::Forge,
                    has(|b| *b == crate::Building::Forge),
                ),
            ];
            if let Some(&(b, _)) = want.iter().find(|(b, had)| !had && can.contains(b))
                && spirit >= crate::BUILD_SPIRIT
            {
                return Some(Intent::Build { building: b });
            }
            if !has(|b| matches!(b, crate::Building::Shrine(_)))
                && spirit >= crate::BUILD_SPIRIT
                && let Some(&b) = can.iter().find(|b| matches!(b, crate::Building::Shrine(_)))
            {
                return Some(Intent::Build { building: b });
            }
            if spirit >= crate::QUARTER_SPIRIT {
                let hex = *game.quarters(player).first()?;
                return Some(Intent::Quarter { hex });
            }
            None
        }
        GreatDeed::Reconciliation => {
            let [a, b] = border_pair(game, here)?;
            let building = crate::Building::Shrine([a, b]);
            (can.contains(&building) && spirit >= crate::BUILD_SPIRIT)
                .then_some(Intent::Build { building })
        }
        _ => None,
    }
}

/// Dice a champion's companions add, as the rules count them.
fn companions_worth(game: &Game, player: PlayerId) -> u8 {
    let worth: u8 = game
        .companions(player)
        .iter()
        .map(|&c| if c == crate::Companion::Dragon { 2 } else { 1 })
        .sum();
    worth.min(crate::COMPANION_DICE)
}

/// A card of the world's mechanics (§21.8), when it helps the bot's deed,
/// or robs or bites a rival where it can.
fn mechanic_play(game: &Game, player: PlayerId) -> Option<Intent> {
    let deed = game.deed(player);
    let me = game.champion(player)?.hex;
    let near_table = |t: &Target| matches!(t, Target::Hex(h) if h.ulength() <= 2);
    for card in game.playable(player) {
        let targets = game.targets(player, card);
        let pick = |f: &dyn Fn(&Target) -> bool| targets.iter().copied().find(|t| f(t));
        let target = match game.def(card).effect {
            Effect::Kindle if deed == Some(GreatDeed::GreatFire) => pick(&|t| match t {
                Target::Hex(h) => game
                    .board()
                    .tile(*h)
                    .and_then(|x| x.region)
                    .is_some_and(|g| !game.burnt_by(player, g)),
                _ => false,
            }),
            // Fire at a settlement of its own, or a rival's Great Fire.
            Effect::Rain => pick(&|t| match t {
                Target::Hex(h) => h.range(1).any(|n| {
                    game.fire(n).is_some_and(|f| {
                        game.owner(n) == Some(player)
                            || f.by.is_some_and(|by| by != player && game.on_eve(by))
                    })
                }),
                _ => false,
            }),
            Effect::Channel(_) if matches!(deed, Some(GreatDeed::River | GreatDeed::Amazon)) => {
                targets.first().copied()
            }
            Effect::Deluge(_) if deed == Some(GreatDeed::FloodedTable) && me.ulength() <= 3 => {
                targets.first().copied()
            }
            Effect::Causeway if deed == Some(GreatDeed::Roads) => pick(&near_table),
            Effect::Rob => pick(&|t| match t {
                Target::Champion(p) => game.cargo(*p).is_some() && game.cargo(player).is_none(),
                _ => false,
            }),
            Effect::Court
                if matches!(deed, Some(GreatDeed::TripleUnion | GreatDeed::FallenEmpire)) =>
            {
                pick(&|t| match t {
                    Target::Hex(h) => game.ruler(*h).is_some_and(|r| r.sworn != Some(player)),
                    _ => false,
                })
            }
            Effect::Lure => targets.first().copied(),
            Effect::Harvest if matches!(deed, Some(GreatDeed::Feast | GreatDeed::DeadBall)) => {
                targets.first().copied()
            }
            Effect::Spade if deed == Some(GreatDeed::Necropolis) => targets.first().copied(),
            Effect::Offering if deed == Some(GreatDeed::Summoning) => targets.first().copied(),
            Effect::Gauntlet if deed == Some(GreatDeed::Arena) => targets.first().copied(),
            Effect::Writ if deed == Some(GreatDeed::DebtBondage) => pick(&|t| match t {
                Target::Champion(p) => !game
                    .debts()
                    .iter()
                    .any(|d| d.debtor == *p && d.creditor == player),
                _ => false,
            }),
            // A rival standing in a river is prey.
            Effect::Piranha(_) => pick(&|t| match t {
                Target::Champion(p) => game
                    .board()
                    .tile(hex_of(game, *p))
                    .is_some_and(|x| x.terrain == crate::Terrain::River),
                _ => false,
            }),
            Effect::Levy => targets.first().copied(),
            Effect::Tunnel if deed == Some(GreatDeed::Treasury) => targets.first().copied(),
            _ => None,
        };
        if let Some(target) = target {
            return Some(Intent::Play { card, target });
        }
    }
    None
}
