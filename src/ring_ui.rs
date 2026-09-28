//! The ring of five on screen (docs/design.md §4): who quenches whom and
//! how chains keep or break the yin-yang rhythm, shown where a choice is
//! made.
//!
//! - Cards in hand wear a badge when the ring touches them right now: a
//!   chain on your turn (+1, or +1 with a surge of qi when the rhythm
//!   breaks), or in a window aimed at you whether an answer will put out
//!   the card coming in.
//! - A diagram of the ring opens over the hand while a card is hovered or
//!   aimed (focused on that card's element: what it quenches, what quenches
//!   it, what it chains from and into), or while the "кольцо" chip in the
//!   status panel is hovered (the whole ring with its legend).
//!
//! One relation explains both uses of quenching: when X quenches Y, a card
//! of X breaks a ward of Y, and an answer of Y cannot put out a card of X.

use bevy::prelude::*;
use necromy_rules::{CardId, Effect, Element, Game, God, PlayerId, Timing, WindowKind};

use crate::hud::{HandCard, INK, UiFont};
use crate::names;
use crate::play::{Match, Selection};
use crate::stats::StatArt;
use crate::ui_skin::{Accent, Frame};

const GOLD: Color = Color::srgb(1.0, 0.82, 0.3);
const SURGE: Color = Color::srgb(1.0, 0.55, 0.2);
const QUENCH: Color = Color::srgb(0.95, 0.32, 0.28);
const GOOD: Color = Color::srgb(0.45, 0.85, 0.45);
const DIM: Color = Color::srgb(0.62, 0.60, 0.56);

/// The diagram: its canvas, the ring's radius, an element's disc.
const CANVAS: f32 = 196.0;
const RADIUS: f32 = 70.0;
const NODE: f32 = 34.0;
const PANEL_W: f32 = 290.0;

pub struct RingUiPlugin;

impl Plugin for RingUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<RingView>()
            .add_systems(Startup, spawn_panel)
            .add_systems(
                crate::InGame,
                (focus, rebuild.run_if(resource_changed::<RingView>)).chain(),
            );
    }
}

/// The chip in the status panel that opens the whole ring.
#[derive(Component)]
pub struct RingChip;

#[derive(Component)]
struct RingPanel;

/// What the diagram shows: nothing, the whole ring, or one card's element
/// (`None` inside for a neutral card).
#[derive(Resource, Default, PartialEq, Clone, Copy)]
enum RingView {
    #[default]
    Hidden,
    Whole,
    Card(Option<Element>),
}

/// The badge a card in hand wears, if the ring touches it right now.
pub fn badge(g: &Game, human: PlayerId, card: CardId) -> Option<(String, Color)> {
    let def = g.def(card);
    // An answer in a window aimed at you: does it put out the card?
    if let Some(WindowKind::Target {
        card: pending,
        target,
        ..
    }) = g.window().map(|w| w.kind)
    {
        if target != human || def.timing != Timing::Response {
            return None;
        }
        let incoming = g.def(pending);
        let theirs = incoming.element;
        return match (def.effect, def.element) {
            (Effect::Cancel, mine) => {
                let fails = matches!((mine, theirs), (Some(m), Some(t)) if t == m.quenched_by());
                Some(if fails {
                    ("не погасит".into(), QUENCH)
                } else {
                    ("погасит".into(), GOOD)
                })
            }
            (Effect::Ward, Some(ward)) if incoming.effect.is_harmful() => {
                Some(if theirs == Some(ward.quenched_by()) {
                    ("оберег пробьют".into(), QUENCH)
                } else {
                    ("оберег удержит".into(), GOOD)
                })
            }
            _ => None,
        };
    }
    // A chain on your own turn.
    if g.current_player() != human || g.window().is_some() {
        return None;
    }
    let (prev, element) = (g.last_element()?, def.element?);
    if prev.generates() != element {
        return None;
    }
    Some(if prev.breaks_rhythm_with(element) {
        ("+1 цепь · всплеск".into(), SURGE)
    } else {
        ("+1 цепь".into(), GOLD)
    })
}

fn spawn_panel(mut commands: Commands) {
    commands.spawn((
        RingPanel,
        Frame::Plate,
        Node {
            position_type: PositionType::Absolute,
            // Over the deck, clear of a raised card.
            left: px(270.0),
            bottom: px(236.0),
            width: px(PANEL_W),
            flex_direction: FlexDirection::Column,
            align_items: AlignItems::Center,
            row_gap: px(4.0),
            padding: UiRect::all(px(18.0)),
            ..default()
        },
        GlobalZIndex(7),
        Visibility::Hidden,
    ));
}

/// Picks what the diagram shows: the chip, then a hovered or aimed card,
/// then a card aimed at the human.
fn focus(
    game: Res<Match>,
    selection: Res<Selection>,
    chip: Query<&Interaction, With<RingChip>>,
    cards: Query<(&Interaction, &HandCard)>,
    mut view: ResMut<RingView>,
) {
    let g = &game.game;
    let element = |card: CardId| RingView::Card(g.def(card).element);
    // Dev aid: `NECROMY_RING=whole` pins the whole ring for screenshots.
    let pinned = std::env::var("NECROMY_RING").is_ok_and(|v| v == "whole");
    let next = if pinned || chip.iter().any(|i| *i != Interaction::None) {
        RingView::Whole
    } else if let Some((_, card)) = cards.iter().find(|(i, _)| **i != Interaction::None) {
        element(card.0)
    } else if let Some(card) = selection.card {
        element(card)
    } else if let Some(WindowKind::Target { card, target, .. }) = g.window().map(|w| w.kind)
        && target == game.human
    {
        element(card)
    } else {
        RingView::Hidden
    };
    view.set_if_neq(next);
}

fn node_centre(i: usize) -> Vec2 {
    let a = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::TAU / 5.0;
    Vec2::new(CANVAS / 2.0, CANVAS / 2.0 + 4.0) + Vec2::new(a.cos(), a.sin()) * RADIUS
}

/// A straight arrow between two discs: a rotated bar and a head.
fn arrow(
    commands: &mut Commands,
    font: &UiFont,
    from: usize,
    to: usize,
    color: Color,
    width: f32,
) -> [Entity; 2] {
    let (a, b) = (node_centre(from), node_centre(to));
    let dir = (b - a).normalize();
    let (a, b) = (a + dir * (NODE / 2.0 + 2.0), b - dir * (NODE / 2.0 + 6.0));
    let mid = (a + b) / 2.0;
    let len = a.distance(b);
    let angle = dir.y.atan2(dir.x);
    let bar = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(mid.x - len / 2.0),
                top: px(mid.y - width / 2.0),
                width: px(len),
                height: px(width),
                ..default()
            },
            BackgroundColor(color),
            UiTransform {
                rotation: Rot2::radians(angle),
                ..default()
            },
        ))
        .id();
    let head = commands
        .spawn((
            Text::new("▶"),
            font.bold(10.0 + width * 2.0),
            TextColor(color),
            TextLayout::no_wrap(),
            Node {
                position_type: PositionType::Absolute,
                left: px(b.x - 6.0),
                top: px(b.y - 9.0),
                ..default()
            },
            UiTransform {
                rotation: Rot2::radians(angle),
                ..default()
            },
        ))
        .id();
    [bar, head]
}

fn line(commands: &mut Commands, font: &UiFont, text: String, color: Color) -> Entity {
    commands
        .spawn((
            Text::new(text),
            font.text(12.0),
            TextColor(color),
            Node {
                width: px(PANEL_W - 36.0),
                ..default()
            },
        ))
        .id()
}

fn god_of(element: Element) -> God {
    God::ALL[element.index()]
}

fn rebuild(
    mut commands: Commands,
    view: Res<RingView>,
    font: Res<UiFont>,
    art: Res<StatArt>,
    game: Res<Match>,
    panel: Single<(Entity, &mut Visibility, &mut Node), With<RingPanel>>,
) {
    let (panel, mut visibility, mut node) = panel.into_inner();
    commands.entity(panel).despawn_related::<Children>();
    let focus = match *view {
        RingView::Hidden => {
            visibility.set_if_neq(Visibility::Hidden);
            return;
        }
        RingView::Whole => None,
        RingView::Card(e) => Some(e),
    };
    visibility.set_if_neq(Visibility::Inherited);
    // The whole ring opens under the gods panel, where its chip is; a
    // card's ring over the deck, beside the hand.
    let (left, right, top, bottom) = match focus {
        None => (Val::Auto, px(10.0), px(205.0), Val::Auto),
        Some(_) => (px(270.0), Val::Auto, Val::Auto, px(236.0)),
    };
    node.left = left;
    node.right = right;
    node.top = top;
    node.bottom = bottom;

    let title = commands
        .spawn((
            Text::new(match focus {
                None => "Кольцо пяти".to_string(),
                Some(None) => "Нейтральная карта".to_string(),
                Some(Some(e)) => format!(
                    "{} · {}",
                    capitalised(names::element(e)),
                    names::god(god_of(e))
                ),
            }),
            font.bold(15.0),
            TextColor(INK),
        ))
        .id();
    commands.entity(panel).add_child(title);

    // The diagram: rim arrows (generation), star arrows (quenching); the
    // focused element's own relations bright, the rest faint.
    let canvas = commands
        .spawn(Node {
            width: px(CANVAS),
            height: px(CANVAS + 8.0),
            ..default()
        })
        .id();
    let lit = |e: Element| match focus {
        None => true,
        Some(f) => f == Some(e),
    };
    let mut parts = Vec::new();
    for e in Element::ALL {
        // Generation: e → e+1, along the rim.
        let to = e.generates();
        let surge = e.breaks_rhythm_with(to);
        let on = lit(e) || lit(to);
        let color = match (surge, on) {
            (true, true) => SURGE,
            (false, true) => GOLD,
            (true, false) => SURGE.with_alpha(0.22),
            (false, false) => GOLD.with_alpha(0.22),
        };
        parts.extend(arrow(
            &mut commands,
            &font,
            e.index(),
            to.index(),
            color,
            if on { 3.0 } else { 2.0 },
        ));
        // Quenching: e → e+2, across the star.
        let q = e.quenches();
        let on = lit(e) || lit(q);
        let color = if on { QUENCH } else { QUENCH.with_alpha(0.2) };
        parts.extend(arrow(
            &mut commands,
            &font,
            e.index(),
            q.index(),
            color,
            if on { 3.0 } else { 1.5 },
        ));
    }
    for e in Element::ALL {
        let c = node_centre(e.index());
        let focused = focus == Some(Some(e));
        let icon = commands
            .spawn((
                ImageNode::new(art.gods[god_of(e).index()].clone()),
                Node {
                    width: px(24.0),
                    height: px(24.0),
                    ..default()
                },
            ))
            .id();
        let disc = commands
            .spawn((
                Frame::Tip,
                Accent(if focused {
                    GOLD
                } else {
                    names::element_color(Some(e))
                }),
                Node {
                    position_type: PositionType::Absolute,
                    left: px(c.x - NODE / 2.0),
                    top: px(c.y - NODE / 2.0),
                    width: px(NODE),
                    height: px(NODE),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
            ))
            .add_child(icon)
            .id();
        parts.push(disc);
        // The element's name beside its disc, outside the ring.
        let out = (c - Vec2::splat(CANVAS / 2.0)).normalize_or_zero();
        let at = c + out * (NODE / 2.0 + 9.0);
        let name = commands
            .spawn((
                Text::new(names::element(e)),
                font.bold(11.0),
                TextColor(if lit(e) {
                    names::element_color(Some(e)).lighter(0.2)
                } else {
                    DIM
                }),
                Node {
                    position_type: PositionType::Absolute,
                    left: px(at.x - 22.0),
                    top: px(at.y - 7.0),
                    width: px(44.0),
                    justify_content: JustifyContent::Center,
                    ..default()
                },
                TextLayout::new(Justify::Center, LineBreak::NoWrap),
            ))
            .id();
        parts.push(name);
    }
    commands.entity(canvas).add_children(&parts);
    commands.entity(panel).add_child(canvas);

    // What it means, in words.
    let g = &game.game;
    let mut lines = Vec::new();
    match focus {
        None => {
            lines.push(("По кругу — порождение: карта следующей стихии, сыгранная после предыдущей в тот же ход, получает +1.".into(), GOLD));
            lines.push(("Оранжевые стрелки (дерево → огонь, железо → вода) ломают ритм инь-ян: +1 и всплеск ци — Угроза +1, Дух −1.".into(), SURGE));
            lines.push(("По звезде — гашение: стихия ломает обереги той, на которую указывает, а «ответ» той стихии её карту не погасит.".into(), QUENCH));
        }
        Some(None) => {
            lines.push((
                "Без стихии: не гасит и не гасится, не ломает обереги и прерывает цепь.".into(),
                DIM,
            ));
        }
        Some(Some(e)) => {
            let q = e.quenches();
            let by = e.quenched_by();
            lines.push((
                format!(
                    "Гасит {}: ломает обереги {}, и «ответ» {} эту карту не погасит.",
                    names::element_accusative(q),
                    names::element_genitive(q),
                    names::element_genitive(q)
                ),
                QUENCH,
            ));
            lines.push((
                format!(
                    "Гасится {}: только {} ломает обереги этой стихии.",
                    names::element_instrumental(by),
                    names::element(by)
                ),
                QUENCH,
            ));
            let from = Element::ALL[(e.index() + 4) % 5];
            let into = e.generates();
            let chain = |a: Element, b: Element| {
                if a.breaks_rhythm_with(b) {
                    " — ломает ритм: Угроза +1, Дух −1"
                } else {
                    ""
                }
            };
            lines.push((
                format!(
                    "Цепь: после {} она +1{}; после неё +1 у {}{}.",
                    names::element_genitive(from),
                    chain(from, e),
                    names::element_genitive(into),
                    chain(e, into)
                ),
                if from.breaks_rhythm_with(e) || e.breaks_rhythm_with(into) {
                    SURGE
                } else {
                    GOLD
                },
            ));
            // Where the chain stands right now on the human's turn.
            if g.current_player() == game.human
                && g.window().is_none()
                && let Some(prev) = g.last_element()
            {
                let now = if prev.generates() == e {
                    format!(
                        "Сейчас: после {} — эта карта в цепи.",
                        names::element_genitive(prev)
                    )
                } else {
                    format!(
                        "Сейчас: последняя карта — {}, цепь дальше даст {}.",
                        names::element(prev),
                        names::element(prev.generates())
                    )
                };
                lines.push((now, INK));
            }
        }
    }
    for (text, color) in lines {
        let l = line(&mut commands, &font, text, color);
        commands.entity(panel).add_child(l);
    }
}

fn capitalised(s: &str) -> String {
    let mut chars = s.chars();
    chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}
