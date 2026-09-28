//! The deck and the dealing (docs/design.md §9): the pile face down left of
//! the hand, with what is left in it and in the discard; each drawn card
//! flies out of it face down, to the human's hand (where it turns over) or
//! to the portrait of the rival who drew it.
//!
//! Draws are read off the view, not the events: a card id new in the
//! human's hand, a rival's hand grown longer. So a rejoin or a new match
//! deals the opening hands the same way.

use bevy::prelude::*;
use necromy_rules::{CardId, PlayerId};

use crate::card_art::{CARD_H, CARD_W, CardArt, card_back};
use crate::hud::{CARD_TUCK, HandCard, INK, UiFont};
use crate::play::Match;
use crate::stats::Seat;
use crate::ui_skin::Frame;

/// Seconds a card takes from the deck to where it goes.
const FLY: f32 = 0.45;
/// Seconds between two cards dealt one after the other.
const STAGGER: f32 = 0.14;
/// Seconds to turn a card over: half on its back as it lands, half face up.
const FLIP: f32 = 0.16;
/// How high a card arcs on its way, in pixels.
const ARC: f32 = 70.0;
/// Card backs drawn in the pile at most, one per this many cards.
const LAYERS: usize = 4;
const PER_LAYER: usize = 8;

pub struct DeckPlugin;

impl Plugin for DeckPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Dealing>().add_systems(
            crate::InGame,
            (
                (notice_draws, rebuild_pile).run_if(resource_changed::<Match>),
                raise_pile,
                fly,
                turn_over,
            )
                .chain(),
        );
    }
}

/// Where the pile goes: the first child of the hand row (`hud.rs`).
#[derive(Component)]
pub struct DeckSlot;

/// A card back on its way.
#[derive(Component)]
struct Flight {
    to: Dest,
    start: f32,
}

#[derive(Clone, Copy)]
enum Dest {
    Hand(CardId),
    Seat(PlayerId),
}

#[derive(Resource, Default)]
struct Dealing {
    /// The human's hand and every hand's size, as last dealt.
    seen: Vec<CardId>,
    sizes: Vec<usize>,
    /// When the next card may leave the deck, so draws queue up.
    next: f32,
    /// Cards in the human's hand and when their flight lands; they stay
    /// hidden until then and turn over after.
    landing: Vec<(CardId, f32)>,
}

fn notice_draws(
    mut commands: Commands,
    time: Res<Time>,
    game: Res<Match>,
    art: Res<CardArt>,
    mut dealing: ResMut<Dealing>,
) {
    let now = time.elapsed_secs();
    let g = &game.game;
    let mut next = dealing.next.max(now);
    let deal = |commands: &mut Commands, next: &mut f32, to: Dest| {
        let back = card_back(commands, &art);
        commands.entity(back).insert((
            Flight { to, start: *next },
            Node {
                position_type: PositionType::Absolute,
                width: px(CARD_W),
                height: px(CARD_H),
                ..default()
            },
            GlobalZIndex(9),
            Visibility::Hidden,
        ));
        *next += STAGGER;
    };

    let hand = g.hand(game.human);
    for &card in hand {
        if !dealing.seen.contains(&card) {
            dealing.landing.push((card, next + FLY));
            deal(&mut commands, &mut next, Dest::Hand(card));
        }
    }
    dealing.seen = hand.to_vec();

    let players: Vec<PlayerId> = g.players().collect();
    dealing.sizes.resize(players.len(), 0);
    for (i, &p) in players.iter().enumerate() {
        let size = g.hand(p).len();
        if p != game.human {
            for _ in dealing.sizes[i]..size {
                deal(&mut commands, &mut next, Dest::Seat(p));
            }
        }
        dealing.sizes[i] = size;
    }
    dealing.next = next;
}

/// The pile: a few backs stacked by how full the deck is, the count on
/// top, the discard below (in sight when the pile is raised).
fn rebuild_pile(
    mut commands: Commands,
    game: Res<Match>,
    art: Res<CardArt>,
    font: Res<UiFont>,
    slot: Single<Entity, With<DeckSlot>>,
    mut shown: Local<Option<(usize, usize)>>,
) {
    let g = &game.game;
    let counts = (g.deck_len(), g.discard_len());
    if *shown == Some(counts) {
        return;
    }
    *shown = Some(counts);
    let (deck, discard) = counts;
    let slot = *slot;
    commands.entity(slot).despawn_related::<Children>().insert((
        Button,
        Node {
            width: px(CARD_W),
            height: px(CARD_H),
            flex_shrink: 0.0,
            ..default()
        },
    ));

    let layers = deck.div_ceil(PER_LAYER).min(LAYERS);
    if layers == 0 {
        // An empty deck: only its place on the table.
        let place = commands
            .spawn((
                Frame::Inset,
                Node {
                    position_type: PositionType::Absolute,
                    width: px(CARD_W),
                    height: px(CARD_H),
                    ..default()
                },
            ))
            .id();
        commands.entity(slot).add_child(place);
    }
    for k in (0..layers).rev() {
        // Lower cards peek out below and to the left of the top one.
        let back = card_back(&mut commands, &art);
        let offset = 3.0 * k as f32;
        commands.entity(back).insert(Node {
            position_type: PositionType::Absolute,
            left: px(-offset),
            top: px(offset),
            width: px(CARD_W),
            height: px(CARD_H),
            ..default()
        });
        commands.entity(slot).add_child(back);
    }

    let chip = |commands: &mut Commands, text: String, top: f32| {
        let label = commands
            .spawn((Text::new(text), font.bold(13.0), TextColor(INK)))
            .id();
        let chip = commands
            .spawn((
                Frame::Tip,
                Node {
                    padding: UiRect::axes(px(8.0), px(4.0)),
                    ..default()
                },
            ))
            .add_child(label)
            .id();
        // Centred across the card.
        commands
            .spawn(Node {
                position_type: PositionType::Absolute,
                top: px(top),
                left: px(0.0),
                right: px(0.0),
                justify_content: JustifyContent::Center,
                ..default()
            })
            .add_child(chip)
            .id()
    };
    let count = chip(&mut commands, format!("колода: {deck}"), 104.0);
    let spent = chip(&mut commands, format!("сброс: {discard}"), 176.0);
    commands.entity(slot).add_children(&[count, spent]);
}

/// The pile rises under the mouse like a card in hand.
fn raise_pile(mut slot: Query<(&Interaction, &mut Node), With<DeckSlot>>) {
    for (interaction, mut node) in &mut slot {
        let top = if *interaction == Interaction::None {
            px(0.0)
        } else {
            px(-CARD_TUCK)
        };
        if node.top != top {
            node.top = top;
        }
    }
}

/// Centre of a node on screen, in logical pixels.
fn centre(node: &ComputedNode, at: &UiGlobalTransform) -> Vec2 {
    at.affine().translation * node.inverse_scale_factor()
}

fn ease(t: f32) -> f32 {
    1.0 - (1.0 - t).powi(3)
}

#[allow(clippy::type_complexity)]
fn fly(
    mut commands: Commands,
    time: Res<Time>,
    mut flights: Query<(
        Entity,
        &Flight,
        &mut Node,
        &mut Visibility,
        &mut UiTransform,
    )>,
    deck: Single<(&ComputedNode, &UiGlobalTransform), With<DeckSlot>>,
    cards: Query<(&HandCard, &ComputedNode, &UiGlobalTransform)>,
    seats: Query<(&Seat, &ComputedNode, &UiGlobalTransform)>,
) {
    let now = time.elapsed_secs();
    let (deck_node, deck_at) = *deck;
    let from = centre(deck_node, deck_at);
    for (entity, flight, mut node, mut visibility, mut transform) in &mut flights {
        let t = (now - flight.start) / FLY;
        if t < 0.0 {
            continue;
        }
        let to = match flight.to {
            Dest::Hand(card) => cards
                .iter()
                .find(|(c, ..)| c.0 == card)
                .map(|(_, n, at)| centre(n, at)),
            Dest::Seat(p) => seats
                .iter()
                .find(|(s, ..)| s.0 == p)
                .map(|(_, n, at)| centre(n, at)),
        };
        // Played or gone before it arrived, or already there.
        let Some(to) = to.filter(|_| t < 1.0) else {
            commands.entity(entity).despawn();
            continue;
        };
        *visibility = Visibility::Inherited;
        let e = ease(t);
        let at = from.lerp(to, e) - Vec2::Y * ARC * (std::f32::consts::PI * t).sin();
        node.left = px(at.x - CARD_W / 2.0);
        node.top = px(at.y - CARD_H / 2.0);
        transform.scale = match flight.to {
            // Turning over as it lands: the back narrows to an edge.
            Dest::Hand(_) => {
                let flip = FLIP / 2.0 / FLY;
                Vec2::new(((1.0 - t) / flip).min(1.0), 1.0)
            }
            // Shrinks into the portrait.
            Dest::Seat(_) => Vec2::splat(1.0 - 0.75 * e),
        };
    }
}

/// A dealt card stays hidden while its back flies, then widens face up.
fn turn_over(
    time: Res<Time>,
    mut dealing: ResMut<Dealing>,
    mut cards: Query<(&HandCard, &mut Visibility, &mut UiTransform)>,
) {
    let now = time.elapsed_secs();
    dealing.landing.retain(|&(_, at)| now < at + FLIP);
    for (card, mut visibility, mut transform) in &mut cards {
        let landing = dealing.landing.iter().find(|(c, _)| *c == card.0);
        let (shown, width) = match landing {
            Some(&(_, at)) if now < at => (false, 1.0),
            Some(&(_, at)) => (true, ((now - at) / (FLIP / 2.0)).min(1.0)),
            None => (true, 1.0),
        };
        let v = if shown {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        visibility.set_if_neq(v);
        if transform.scale.x != width {
            transform.scale.x = width;
        }
    }
}
