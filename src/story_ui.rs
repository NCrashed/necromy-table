//! The storyteller's lines on screen (docs/design.md §8): the quests the
//! gods give.
//!
//! - Left, above the human's sheet: their open lines under a scroll icon,
//!   each with the god who told it, what it asks (with progress), rounds
//!   left and the reward. A click on a line (or the header) opens the
//!   journal.
//! - The journal: every open line in full, the god's own words included,
//!   until closed.
//! - When a god tells the human a new line, its voice shows in the middle
//!   until the human closes it (or `VOICE_SECS` pass); it stays readable in
//!   the journal afterwards.
//! - The goal hex of a line glows on the board and carries a scroll
//!   (`board.rs`).

use bevy::prelude::*;
use necromy_rules::{Game, Line};

use crate::audio::Speech;
use crate::hud::{INK, UiFont};
use crate::icons::StatIcon;
use crate::names;
use crate::play::Match;
use crate::stats::{self, StatArt};
use crate::ui_skin::{Accent, Frame};

const VOICE: Color = Color::srgb(0.85, 0.80, 0.95);
const DIM: Color = Color::srgb(0.72, 0.70, 0.64);
const URGENT: Color = Color::srgb(0.95, 0.55, 0.4);
/// A deal that already came true.
const MET: Color = Color::srgb(0.55, 0.9, 0.5);
/// A god's voice closes by itself after this long, if the human does not.
const VOICE_SECS: f32 = 30.0;

pub struct StoryUiPlugin;

impl Plugin for StoryUiPlugin {
    fn build(&self, app: &mut App) {
        // Dev aid: `NECROMY_JOURNAL=1` opens the journal for screenshots.
        app.insert_resource(Journal {
            open: std::env::var_os("NECROMY_JOURNAL").is_some(),
        })
        .add_systems(Startup, spawn)
        .add_systems(
            crate::InGame,
            (rebuild_lines, rebuild_voice, rebuild_journal)
                .run_if(resource_changed::<Match>.or_else(resource_changed::<Journal>)),
        )
        .add_systems(crate::InGame, (expire_voice, clicks, above_the_sheet));
    }
}

#[derive(Component)]
struct StoryPanel;

#[derive(Component)]
struct VoicePanel;

#[derive(Component)]
struct JournalPanel;

/// Opens the journal (on a line: scrolled to it; there are few).
#[derive(Component)]
struct OpenJournal;

#[derive(Component)]
struct CloseJournal;

#[derive(Component)]
struct CloseVoice;

/// Takes the letter at this index, or (`None`) lets them all lie.
#[derive(Component, Clone, Copy)]
struct LetterButton(Option<u8>);

/// The quest journal is open.
#[derive(Resource, Default, PartialEq)]
struct Journal {
    open: bool,
}

fn spawn(mut commands: Commands) {
    commands.spawn((
        StoryPanel,
        Node {
            position_type: PositionType::Absolute,
            bottom: px(232.0),
            left: px(10.0),
            ..default()
        },
    ));
    commands.spawn((
        VoicePanel,
        Node {
            position_type: PositionType::Absolute,
            top: px(90.0),
            left: px(0.0),
            right: px(0.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::FlexStart,
            ..default()
        },
        GlobalZIndex(11),
        Visibility::Hidden,
    ));
    commands.spawn((
        JournalPanel,
        Node {
            position_type: PositionType::Absolute,
            top: px(90.0),
            left: px(0.0),
            right: px(0.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::FlexStart,
            ..default()
        },
        GlobalZIndex(11),
        Visibility::Hidden,
    ));
}

fn reward(line: &Line) -> String {
    if let Some(v) = line.betrays {
        format!(
            "+{} Стиля; {} отвернётся (−{} благосклонности)",
            line.style,
            names::god(v),
            necromy_rules::BETRAYAL
        )
    } else if line.chapter > 0 {
        format!(
            "+{} Стиля; станешь {} {}",
            line.style,
            names::patronage_instrumental(names::chapter_rung(line.chapter)),
            names::god_genitive(line.god)
        )
    } else if line.stake > 0 {
        format!("+{} Стиля; провал — −{}", line.style, line.stake)
    } else {
        format!(
            "+{} Стиля и благосклонность {}",
            line.style,
            names::god_genitive(line.god)
        )
    }
}

fn rounds_left(line: &Line, g: &Game) -> String {
    match line.deadline.saturating_sub(g.round()) {
        0 => "последний раунд".into(),
        1 => "остался 1 раунд".into(),
        n @ 2..=4 => format!("осталось {n} раунда"),
        n => format!("осталось {n} раундов"),
    }
}

/// The god's words for a line: the model's when they came, else the template.
fn voice_of<'a>(game: &'a Match, line: &Line) -> &'a str {
    game.oracle.line_voices.get(&line.id).map_or(
        if line.betrays.is_some() {
            "Ты служишь не тому. Сделай это — и я запомню, кому ты верен на деле."
        } else {
            names::line_voice(line.kind)
        },
        String::as_str,
    )
}

fn text(commands: &mut Commands, font: TextFont, s: String, color: Color, width: f32) -> Entity {
    commands
        .spawn((
            Text::new(s),
            font,
            TextColor(color),
            Node {
                width: px(width),
                ..default()
            },
        ))
        .id()
}

fn rebuild_lines(
    mut commands: Commands,
    game: Res<Match>,
    art: Res<StatArt>,
    font: Res<UiFont>,
    panel: Single<Entity, With<StoryPanel>>,
) {
    commands.entity(*panel).despawn_related::<Children>();
    let g = &game.game;
    let letters = g.letters(game.human).to_vec();
    let lines: Vec<_> = g.lines_of(game.human).collect();
    let deals = deals(&game);
    if lines.is_empty() && deals.is_empty() && letters.is_empty() {
        return;
    }
    let sheet = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(px(12.0)),
                row_gap: px(4.0),
                width: px(250.0),
                ..default()
            },
            Frame::Panel,
        ))
        .id();
    // Tonight's letters from the gods, one to take (docs/storyteller-plan.md).
    if !letters.is_empty() {
        let title = stats::label(
            &mut commands,
            &font,
            "Письма богов: возьми одно",
            14.0,
            true,
        );
        commands.entity(sheet).add_child(title);
        for (i, line) in letters.iter().enumerate() {
            let card = commands
                .spawn((
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: px(3.0),
                        padding: UiRect::axes(px(8.0), px(6.0)),
                        ..default()
                    },
                    Frame::Tip,
                    // A temptation burns red.
                    Accent(if line.betrays.is_some() {
                        Color::srgb(0.85, 0.25, 0.2)
                    } else {
                        crate::gods_ui::god_color(line.god)
                    }),
                ))
                .id();
            // Wrapped, and alone on its row: a wrapped text beside an icon
            // grows the row tall (flex min-content); the rim has the colour.
            let head = text(
                &mut commands,
                font.bold(13.0),
                format!("{} · {}", names::god(line.god), names::letter_head(line)),
                INK,
                218.0,
            );
            let at = line
                .at
                .zip(g.champion(game.human).map(|c| c.hex))
                .map(|(a, h)| format!(" · {} кл.", a.unsigned_distance_to(h)))
                .unwrap_or_default();
            let what = text(
                &mut commands,
                font.text(11.0),
                format!("{}{at}", names::line_goal(line, g)),
                INK,
                218.0,
            );
            let gain = text(
                &mut commands,
                font.text(11.0),
                format!("{} · срок {} р.", reward(line), necromy_rules::LINE_ROUNDS),
                DIM,
                218.0,
            );
            let take = button(&mut commands, &font, "Взять", LetterButton(Some(i as u8)));
            commands
                .entity(card)
                .add_children(&[head, what, gain, take]);
            commands.entity(sheet).add_child(card);
        }
        let lie = button(&mut commands, &font, "Отложить", LetterButton(None));
        commands.entity(sheet).add_child(lie);
    }
    // The header opens the journal too.
    let header = commands
        .spawn((
            OpenJournal,
            Button,
            Node {
                align_items: AlignItems::Center,
                column_gap: px(6.0),
                ..default()
            },
        ))
        .id();
    let scroll = stats::icon_node(&mut commands, art.icon(StatIcon::Quest), 20.0, true);
    let title = stats::label(
        &mut commands,
        &font,
        &format!("Задания ({})", lines.len()),
        14.0,
        true,
    );
    let hint = stats::label(&mut commands, &font, "подробно ›", 11.0, false);
    commands.entity(header).add_children(&[scroll, title, hint]);
    if !lines.is_empty() {
        commands.entity(sheet).add_child(header);
    } else {
        commands.entity(header).despawn();
    }
    for line in lines {
        let row = commands
            .spawn((
                OpenJournal,
                Button,
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: px(2.0),
                    ..default()
                },
            ))
            .id();
        let head = stats::row(&mut commands);
        let icon = stats::icon_node(
            &mut commands,
            art.gods[line.god.index()].clone(),
            18.0,
            true,
        );
        let name = stats::label(&mut commands, &font, &names::line_head(line), 13.0, true);
        commands.entity(head).add_children(&[icon, name]);
        let left = line.deadline.saturating_sub(g.round());
        let body = text(
            &mut commands,
            font.text(11.0),
            format!("{} · {}", names::line_goal(line, g), rounds_left(line, g)),
            if left == 0 { URGENT } else { DIM },
            234.0,
        );
        commands.entity(row).add_children(&[head, body]);
        commands.entity(sheet).add_child(row);
    }
    // Truces and wagers the human is in (§7.3), until dusk settles them.
    if !deals.is_empty() {
        let title = stats::label(&mut commands, &font, "Сделки до заката", 14.0, true);
        commands.entity(sheet).add_child(title);
    }
    for (god, what, lit) in deals {
        // Top-aligned: a wrapped line must not push the icon down.
        let row = commands
            .spawn(Node {
                align_items: AlignItems::FlexStart,
                column_gap: px(6.0),
                ..default()
            })
            .id();
        let icon = stats::icon_node(&mut commands, art.gods[god.index()].clone(), 18.0, true);
        let body = text(
            &mut commands,
            font.text(11.0),
            what,
            if lit { MET } else { DIM },
            // Beside an 18 px icon within the 250 px sheet.
            196.0,
        );
        commands.entity(row).add_children(&[icon, body]);
        commands.entity(sheet).add_child(row);
    }
    commands.entity(*panel).add_child(sheet);
}

/// The human's truces and wagers, and wagers on them: the god who holds
/// each, a line, and whether it already came true.
fn deals(game: &Match) -> Vec<(necromy_rules::God, String, bool)> {
    let g = &game.game;
    let me = game.human;
    let mut out = Vec::new();
    for t in g.truces() {
        if t.a == me || t.b == me {
            let other = if t.a == me { t.b } else { t.a };
            out.push((
                t.god,
                format!("Мир: ты и {} не сражаетесь", game.name(other)),
                false,
            ));
        }
    }
    for w in g.wagers() {
        if w.player == me {
            let line = format!(
                "Пари: {} {}{}",
                game.name(w.target),
                names::bet(w.bet),
                if w.happened {
                    " — уже сбылось"
                } else {
                    ""
                }
            );
            out.push((w.god, line, w.happened));
        } else if w.target == me {
            let line = format!(
                "{} ставит, что ты {}",
                game.name(w.player),
                names::bet_you(w.bet)
            );
            out.push((w.god, line, false));
        }
    }
    out
}

/// A small button: `marker` says what it does.
fn button(commands: &mut Commands, font: &UiFont, label: &str, marker: impl Bundle) -> Entity {
    let t = stats::label(commands, font, label, 13.0, true);
    commands
        .spawn((
            marker,
            Button,
            Frame::Button,
            Node {
                padding: UiRect::axes(px(14.0), px(7.0)),
                align_self: AlignSelf::FlexEnd,
                ..default()
            },
        ))
        .add_child(t)
        .id()
}

fn rebuild_voice(
    mut commands: Commands,
    game: Res<Match>,
    journal: Res<Journal>,
    art: Res<StatArt>,
    font: Res<UiFont>,
    panel: Single<(Entity, &mut Visibility), With<VoicePanel>>,
) {
    let (panel, mut visibility) = panel.into_inner();
    commands.entity(panel).despawn_related::<Children>();
    // A wish's answer has the same spot, and the open journal says it all.
    let Some(line) = game
        .told
        .filter(|_| game.wish_reply.is_none() && !journal.open)
    else {
        visibility.set_if_neq(Visibility::Hidden);
        return;
    };
    visibility.set_if_neq(Visibility::Inherited);
    let frame = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(4.0),
                padding: UiRect::all(px(18.0)),
                width: px(480.0),
                ..default()
            },
            Frame::Plate,
            Accent(VOICE),
        ))
        .id();
    let head = stats::row(&mut commands);
    let icon = stats::icon_node(
        &mut commands,
        art.gods[line.god.index()].clone(),
        28.0,
        true,
    );
    let who = stats::label(
        &mut commands,
        &font,
        &format!(
            "{} зовёт тебя: «{}»",
            names::god(line.god),
            names::line_title(line.kind)
        ),
        15.0,
        true,
    );
    commands.entity(head).add_children(&[icon, who]);
    // Typed out in the god's voice; when the model's words replace the
    // template, the typing starts again with them.
    let words = format!("«{}»", voice_of(&game, &line));
    let voice = text(&mut commands, font.text(13.0), String::new(), VOICE, 450.0);
    commands.entity(voice).insert(Speech {
        key: speech_key(game.told_serial, &words),
        god: line.god,
        from: 1,
        text: words,
    });
    let goal = text(
        &mut commands,
        font.text(12.0),
        format!(
            "Цель: {}, {}. Награда: {}.",
            names::line_goal(&line, &game.game),
            rounds_left(&line, &game.game),
            reward(&line)
        ),
        INK,
        450.0,
    );
    let note = text(
        &mut commands,
        font.text(11.0),
        "Задание записано в журнал: свиток слева, над твоим листом.".into(),
        DIM,
        450.0,
    );
    let ok = button(&mut commands, &font, "Понятно", CloseVoice);
    commands
        .entity(frame)
        .add_children(&[head, voice, goal, note, ok]);
    commands.entity(panel).add_child(frame);
}

fn rebuild_journal(
    mut commands: Commands,
    game: Res<Match>,
    journal: Res<Journal>,
    art: Res<StatArt>,
    font: Res<UiFont>,
    panel: Single<(Entity, &mut Visibility), With<JournalPanel>>,
) {
    let (panel, mut visibility) = panel.into_inner();
    commands.entity(panel).despawn_related::<Children>();
    let g = &game.game;
    let lines: Vec<_> = g.lines_of(game.human).collect();
    if !journal.open {
        visibility.set_if_neq(Visibility::Hidden);
        return;
    }
    visibility.set_if_neq(Visibility::Inherited);
    let frame = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(10.0),
                padding: UiRect::all(px(20.0)),
                width: px(520.0),
                ..default()
            },
            Frame::Plate,
        ))
        .id();
    let head = stats::row(&mut commands);
    let scroll = stats::icon_node(&mut commands, art.icon(StatIcon::Quest), 28.0, true);
    let title = stats::label(&mut commands, &font, "Задания богов", 17.0, true);
    commands.entity(head).add_children(&[scroll, title]);
    commands.entity(frame).add_child(head);
    if lines.is_empty() {
        let none = text(
            &mut commands,
            font.text(12.0),
            "Пока никто из богов ничего не просит.".into(),
            DIM,
            480.0,
        );
        commands.entity(frame).add_child(none);
    }
    for line in lines {
        let entry = commands
            .spawn((
                Node {
                    flex_direction: FlexDirection::Column,
                    row_gap: px(3.0),
                    padding: UiRect::all(px(10.0)),
                    ..default()
                },
                Frame::Tip,
                Accent(names::element_color(Some(line.god.element()))),
            ))
            .id();
        let top = stats::row(&mut commands);
        let icon = stats::icon_node(
            &mut commands,
            art.gods[line.god.index()].clone(),
            22.0,
            true,
        );
        let name = stats::label(
            &mut commands,
            &font,
            &format!(
                "{}: «{}»",
                names::god(line.god),
                names::line_title(line.kind)
            ),
            14.0,
            true,
        );
        commands.entity(top).add_children(&[icon, name]);
        let voice = text(
            &mut commands,
            font.text(12.0),
            format!("«{}»", voice_of(&game, line)),
            VOICE,
            460.0,
        );
        let left = line.deadline.saturating_sub(g.round());
        let goal = text(
            &mut commands,
            font.text(12.0),
            format!(
                "Цель: {}. Срок: {} (до раунда {}).",
                names::line_goal(line, g),
                rounds_left(line, g),
                line.deadline
            ),
            if left == 0 { URGENT } else { INK },
            460.0,
        );
        let prize = text(
            &mut commands,
            font.text(12.0),
            format!("Награда: {}.", reward(line)),
            DIM,
            460.0,
        );
        commands
            .entity(entry)
            .add_children(&[top, voice, goal, prize]);
        commands.entity(frame).add_child(entry);
    }
    let close = button(&mut commands, &font, "Закрыть", CloseJournal);
    commands.entity(frame).add_child(close);
    commands.entity(panel).add_child(frame);
}

#[allow(clippy::type_complexity)]
fn clicks(
    open: Query<&Interaction, (Changed<Interaction>, With<OpenJournal>)>,
    letter: Query<(&Interaction, &LetterButton), Changed<Interaction>>,
    close: Query<&Interaction, (Changed<Interaction>, With<CloseJournal>)>,
    ok: Query<&Interaction, (Changed<Interaction>, With<CloseVoice>)>,
    keys: Res<ButtonInput<KeyCode>>,
    mut journal: ResMut<Journal>,
    mut game: ResMut<Match>,
) {
    let hit = |mut i: std::slice::Iter<'_, Interaction>| i.any(|i| *i == Interaction::Pressed);
    let (open, close, ok): (Vec<_>, Vec<_>, Vec<_>) = (
        open.iter().copied().collect(),
        close.iter().copied().collect(),
        ok.iter().copied().collect(),
    );
    if hit(open.iter()) {
        journal.set_if_neq(Journal { open: true });
    }
    if hit(close.iter()) || (journal.open && keys.just_pressed(KeyCode::Escape)) {
        journal.set_if_neq(Journal { open: false });
    }
    if hit(ok.iter()) {
        game.told = None;
    }
    for (i, b) in &letter {
        if *i == Interaction::Pressed {
            let human = game.human;
            let intent = match b.0 {
                Some(index) => necromy_rules::Intent::TakeLetter { index },
                None => necromy_rules::Intent::DeclineLetters,
            };
            if let Err(err) = game.act(human, intent) {
                warn!("letter: {err}");
            }
        }
    }
    // Reading the journal covers what the voice said.
    if journal.open && game.told.is_some() {
        game.told = None;
    }
}

fn expire_voice(time: Res<Time>, mut shown: Local<(u32, f32)>, mut game: ResMut<Match>) {
    if game.told.is_none() {
        return;
    }
    let now = time.elapsed_secs();
    if shown.0 != game.told_serial {
        *shown = (game.told_serial, now);
    }
    if now - shown.1 > VOICE_SECS {
        game.told = None;
    }
}

/// Tells one told line's words from another's, and from wish replies.
fn speech_key(serial: u32, words: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    ("told", serial, words).hash(&mut h);
    h.finish()
}

/// The quests stand just above the human's sheet, however tall it grows
/// (curses, items, an oath).
fn above_the_sheet(
    sheet: Query<&ComputedNode, With<stats::MyPanel>>,
    mut panel: Query<&mut Node, With<StoryPanel>>,
) {
    let (Ok(sheet), Ok(mut panel)) = (sheet.single(), panel.single_mut()) else {
        return;
    };
    let bottom = px(sheet.size().y * sheet.inverse_scale_factor() + 18.0);
    if panel.bottom != bottom {
        panel.bottom = bottom;
    }
}
