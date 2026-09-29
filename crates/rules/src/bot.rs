//! Placeholder bot: greedy card play, then walks towards the nearest corpse.
//!
//! Real bots are utility AI with a champion's character (docs/design.md §15).
//! This one exercises the turn loop and the reaction windows. It reads the
//! same state a human could see (plus its own hand) and uses no randomness,
//! so replays stay trivial. It never returns an illegal intent.

use hexx::Hex;

use crate::cards::{self, CardId, Effect};
use crate::game::{
    Condition, Game, Goal, Intent, PlayerId, Target, TimeOfDay, WindowKind, WishKind,
};
use crate::gods::God;
use necromy_dice::Face;

pub fn choose(game: &Game, player: PlayerId) -> Intent {
    if game.wish_due() == Some(player) {
        return wish(game, player);
    }
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
    // The restless dead next to us, laid to rest while we are hale (§20.4).
    if me.hp >= 3
        && let Some(u) = game
            .undead()
            .iter()
            .filter(|u| game.attack_cost(player, u.hex).is_ok())
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
            _ => None,
        })
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

/// The Dominant's wish: the best-graded plain wish, from its own patron when
/// tied; at the leader in Style when it needs a rival. A bot holding the
/// Wager refuses every wish: that is the bet.
fn wish(game: &Game, player: PlayerId) -> Intent {
    if let Some(Condition::Wager { .. }) = game.secret(player) {
        return Intent::RefuseWish;
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
