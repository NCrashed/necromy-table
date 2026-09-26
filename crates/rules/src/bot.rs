//! Placeholder bot: greedy card play, then walks towards the nearest corpse.
//!
//! Real bots are utility AI with a champion's character (docs/design.md §15).
//! This one exercises the turn loop and the reaction windows. It reads the
//! same state a human could see (plus its own hand) and uses no randomness,
//! so replays stay trivial. It never returns an illegal intent.

use hexx::Hex;

use crate::cards::{CardId, Effect};
use crate::game::{Game, Intent, PlayerId, Target, TimeOfDay, WindowKind};
use necromy_dice::Face;

pub fn choose(game: &Game, player: PlayerId) -> Intent {
    match game.window() {
        Some(window) => respond(game, player, window.kind),
        None if game.current_player() == player => own_turn(game, player),
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
        WindowKind::End { .. } => {
            if let Some(intent) = mend(game, player, &cards) {
                return intent;
            }
        }
        _ => {}
    }
    Intent::Pass
}

fn own_turn(game: &Game, player: PlayerId) -> Intent {
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

/// Attack a neighbour we are at least as healthy and strong as.
fn attack(game: &Game, player: PlayerId) -> Option<Intent> {
    let me = game.champion(player)?;
    game.attackable().into_iter().find_map(|hex| {
        let foe = game.champion(game.occupant(hex)?)?;
        (me.hp >= foe.hp && me.might >= foe.might).then_some(Intent::Move { to: hex })
    })
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
            Effect::Damage(_) | Effect::Drain(_) | Effect::Finish(_) | Effect::Root
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

/// Heal ourselves when hurt.
fn mend(game: &Game, player: PlayerId, cards: &[CardId]) -> Option<Intent> {
    let me = game.champion(player)?;
    if me.hp * 2 > me.body {
        return None;
    }
    cards
        .iter()
        .find(|&&c| {
            matches!(game.def(c).effect, Effect::Heal(_))
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
        .board()
        .corpses()
        .map(|(hex, _)| hex)
        .filter(|&hex| game.occupant(hex).is_none_or(|p| p == player))
        .min_by_key(|&hex| (me.hex.unsigned_distance_to(hex), hex.x(), hex.y()))
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
                .is_ok_and(|cost| cost <= game.move_points())
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
