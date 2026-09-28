//! The event feed, left under the status panel. A card named in a line
//! («Искра») is its own highlighted span; hovering it shows the whole card
//! beside the feed, so a line says what was cast, not only what it is
//! called. Bevy's UI picking hits text spans, so the hover is the name
//! itself, not the whole line.

use bevy::picking::hover::HoverMap;
use bevy::prelude::*;
use necromy_rules::CardDef;
use necromy_rules::cards::POOL;

use crate::card_art::{CARD_H, CardArt, CardLook, card_node};
use crate::hud::UiFont;
use crate::play::Match;
use crate::stats::StatArt;
use crate::ui_skin::Frame;

const FEED_COLOR: Color = Color::srgb(0.82, 0.80, 0.74);
const CARD_COLOR: Color = Color::srgb(1.0, 0.8, 0.42);
const FONT_SIZE: f32 = 12.0;

pub struct FeedPlugin;

impl Plugin for FeedPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn).add_systems(
            crate::InGame,
            (rebuild.run_if(resource_changed::<Match>), card_tip).chain(),
        );
    }
}

#[derive(Component)]
struct Feed;

/// A card's name in a feed line.
#[derive(Component)]
struct CardRef(&'static CardDef);

/// Holds the card shown for the hovered name.
#[derive(Component)]
struct FeedCard;

fn spawn(mut commands: Commands, font: Res<UiFont>) {
    commands.spawn((
        Feed,
        Text::new(""),
        font.text(FONT_SIZE),
        TextColor(FEED_COLOR),
        Node {
            position_type: PositionType::Absolute,
            top: px(140.0),
            left: px(10.0),
            padding: UiRect::all(px(10.0)),
            max_width: px(360.0),
            ..default()
        },
        Frame::Panel,
    ));
    commands.spawn((
        FeedCard,
        Node {
            position_type: PositionType::Absolute,
            ..default()
        },
        GlobalZIndex(10),
        Visibility::Hidden,
    ));
}

/// The card a «quoted» name stands for, if it is one.
fn card_named(name: &str) -> Option<&'static CardDef> {
    POOL.iter().find(|d| d.name == name)
}

/// A line cut into plain text and card names, the quotes kept on the text.
fn pieces(line: &str) -> Vec<(String, Option<&'static CardDef>)> {
    let mut out: Vec<(String, Option<&'static CardDef>)> = Vec::new();
    let mut rest = line;
    let push_plain =
        |out: &mut Vec<(String, Option<&'static CardDef>)>, s: &str| match out.last_mut() {
            Some((text, None)) => text.push_str(s),
            _ => out.push((s.to_string(), None)),
        };
    while let Some(open) = rest.find('«') {
        let after = &rest[open + '«'.len_utf8()..];
        let Some(close) = after.find('»') else {
            break;
        };
        let name = &after[..close];
        match card_named(name) {
            Some(def) => {
                push_plain(&mut out, &rest[..open + '«'.len_utf8()]);
                out.push((name.to_string(), Some(def)));
                push_plain(&mut out, "»");
            }
            None => push_plain(
                &mut out,
                &rest[..open + '«'.len_utf8() + close + '»'.len_utf8()],
            ),
        }
        rest = &after[close + '»'.len_utf8()..];
    }
    push_plain(&mut out, rest);
    out
}

fn rebuild(
    mut commands: Commands,
    game: Res<Match>,
    font: Res<UiFont>,
    feed: Single<Entity, With<Feed>>,
) {
    let feed = *feed;
    commands.entity(feed).despawn_related::<Children>();
    let text = game.feed.join("\n");
    let spans: Vec<Entity> = pieces(&text)
        .into_iter()
        .map(|(text, card)| match card {
            Some(def) => commands
                .spawn((
                    TextSpan::new(text),
                    font.bold(FONT_SIZE),
                    TextColor(CARD_COLOR),
                    CardRef(def),
                ))
                .id(),
            None => commands
                .spawn((
                    TextSpan::new(text),
                    font.text(FONT_SIZE),
                    TextColor(FEED_COLOR),
                ))
                .id(),
        })
        .collect();
    commands.entity(feed).add_children(&spans);
}

/// The card under the mouse in the feed, beside the feed.
#[allow(clippy::too_many_arguments)]
fn card_tip(
    mut commands: Commands,
    hovers: Res<HoverMap>,
    refs: Query<&CardRef>,
    game: Res<Match>,
    font: Res<UiFont>,
    card_art: Res<CardArt>,
    stat_art: Res<StatArt>,
    feed: Single<(&ComputedNode, &UiGlobalTransform), With<Feed>>,
    tip: Single<(Entity, &mut Node, &mut Visibility), With<FeedCard>>,
    window: Single<&Window>,
    mut shown: Local<Option<&'static str>>,
) {
    let (tip, mut node, mut visibility) = tip.into_inner();
    let hovered = hovers
        .values()
        .flat_map(|hits| hits.keys())
        .find_map(|e| refs.get(*e).ok())
        .map(|r| r.0)
        // Dev aid: `NECROMY_FEED_CARD=1` pins the last card the feed names.
        .or_else(|| {
            std::env::var_os("NECROMY_FEED_CARD")
                .and_then(|_| refs.iter().last())
                .map(|r| r.0)
        });
    let Some(def) = hovered else {
        if shown.take().is_some() {
            commands.entity(tip).despawn_related::<Children>();
        }
        visibility.set_if_neq(Visibility::Hidden);
        return;
    };
    // Right of the feed, level with the mouse, kept on screen.
    let (feed_node, feed_at) = *feed;
    let scale = feed_node.inverse_scale_factor();
    let right = (feed_at.affine().translation.x + feed_node.size().x / 2.0) * scale;
    let y = window
        .cursor_position()
        .map_or(0.0, |c| c.y - CARD_H / 2.0)
        .clamp(8.0, (window.height() - CARD_H - 8.0).max(8.0));
    node.left = px(right + 8.0);
    node.top = px(y);
    visibility.set_if_neq(Visibility::Inherited);
    if *shown == Some(def.name) {
        return;
    }
    *shown = Some(def.name);
    commands.entity(tip).despawn_related::<Children>();
    let card = card_node(
        &mut commands,
        &font,
        &card_art,
        &stat_art,
        &game.game,
        def,
        CardLook {
            usable: true,
            outline: None,
            extra: Vec::new(),
            badge: None,
            cost: None,
        },
    );
    commands.entity(tip).add_child(card);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn card_names_are_cut_out_of_a_line() {
        let card = POOL[0].name;
        let line = format!("Бхава играет «{card}». Закон «Нечто» в силе.");
        let parts = pieces(&line);
        let names: Vec<_> = parts.iter().filter(|(_, d)| d.is_some()).collect();
        assert_eq!(names.len(), 1);
        assert_eq!(names[0].0, card);
        let whole: String = parts.iter().map(|(t, _)| t.as_str()).collect();
        assert_eq!(whole, line, "nothing lost or added");
    }
}
