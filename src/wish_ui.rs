//! Wishes (docs/design.md §7, §21.4).
//!
//! Everyone wishes once a day, for dusk. All day the human may open the
//! panel («Загадать желание» under the action bar) and seal a wish; at
//! dusk, if they still have not, it opens by itself and the table waits. With the gods' voice up (`oracle.rs`), the human picks a god and
//! writes the wish in their own words; the model reads it and the god
//! answers. Without it, or on request, the prepared wishes are offered as
//! buttons. The grade is not shown in advance: the god's answer tells it.
//!
//! Every wish, the bots' too, is answered in a panel for a few seconds: who
//! asked whom for what, the grade in stars, the god's words and what came of
//! it; at dusk the answers come one after another. Under the action bar a
//! strip tells where the human's own wish stands and shows the rivals'
//! wishes as they write them.

use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;
use necromy_rules::{God, Intent, PlayerId, Seal, WishKind};

use crate::audio::{Speech, Tone};
use crate::hud::{INK, UiFont};
use crate::names;
use crate::play::Match;
use crate::stats::{self, StatArt};
use crate::ui_skin::{Accent, BRONZE_RIM, Frame};

const GOLD: Color = Color::srgb(1.0, 0.82, 0.3);
const DIM: Color = Color::srgb(0.72, 0.70, 0.64);
const VOICE: Color = Color::srgb(0.85, 0.80, 0.95);
/// How long a god's answer stays on screen.
const REPLY_SECS: f32 = 7.0;
/// Longest wish the field accepts, in characters.
const MAX_WISH: usize = 200;
/// How fast `NECROMY_WISH` types the dev wish.
const DEV_LETTERS_PER_SEC: f32 = 12.0;

pub struct WishUiPlugin;

impl Plugin for WishUiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WishDraft>()
            .init_resource::<Typing>()
            .add_systems(Startup, spawn)
            .add_systems(crate::InGame, (dev_panel, dev_wish, type_wish, buttons))
            .add_systems(
                crate::InGame,
                rebuild_panel.after(type_wish).after(buttons).run_if(
                    resource_changed::<Match>
                        .or_else(resource_changed::<WishDraft>)
                        .or_else(resource_changed::<crate::tutorial::Focus>),
                ),
            )
            .add_systems(
                crate::InGame,
                rebuild_reply.run_if(resource_changed::<Match>),
            )
            .add_systems(
                crate::InGame,
                (
                    share_draft.after(type_wish).after(buttons),
                    watch_draft,
                    rebuild_watch
                        .run_if(resource_changed::<Match>.or_else(resource_changed::<WishDraft>)),
                ),
            )
            .add_systems(
                crate::InGame,
                (expire_reply, reply_buttons, listening_dots, place_watch),
            );
    }
}

/// The wish being put together in the panel.
#[derive(Resource, Default)]
struct WishDraft {
    god: Option<God>,
    kind: Option<WishKind>,
    target: Option<PlayerId>,
    /// Free words, when the gods' voice is up.
    text: String,
    /// The human chose the prepared wishes even with the voice up.
    prepared: bool,
    /// The prepared wishes' tab (`Theme`); `None` picks one.
    theme: Option<Theme>,
    /// Opened by day, before dusk asks for it.
    open: bool,
}

/// The keyboard is the wish's: `play::keys` leaves it alone.
#[derive(Resource, Default)]
pub struct Typing(pub bool);

#[derive(Component)]
struct WishPanel;

#[derive(Component)]
struct ReplyPanel;

/// A button under the gods' answers: leaf back or on, or put them away.
#[derive(Component, Clone, Copy)]
enum ReplyButton {
    Back,
    On,
    Close,
}

/// Another seat's wish as they write it.
#[derive(Component)]
struct WatchPanel;

/// "Тришна слушает…", animated while the model thinks.
#[derive(Component)]
struct Listening(God);

#[derive(Component, Clone, Copy)]
enum WishButton {
    God(God),
    Kind(WishKind),
    Target(PlayerId),
    Make,
    Refuse,
    /// A tab of the prepared wishes.
    Theme(Theme),
    /// Switch between free words and the prepared wishes.
    Prepared(bool),
    /// Open the panel by day.
    Open,
    /// Close it again: the wish can wait until dusk.
    Later,
}

/// The prepared wishes come in tabs: what the chosen god likes first, then
/// by what they are about.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Theme {
    /// What the chosen god likes, the best grade first.
    Liked,
    Me,
    Rivals,
    Deals,
    Deck,
    Land,
    People,
}

impl Theme {
    const ALL: [Theme; 7] = [
        Theme::Liked,
        Theme::Me,
        Theme::Rivals,
        Theme::Deals,
        Theme::Deck,
        Theme::Land,
        Theme::People,
    ];

    fn label(self) -> &'static str {
        match self {
            Theme::Liked => "По вкусу богу",
            Theme::Me => "Себе",
            Theme::Rivals => "Соперникам",
            Theme::Deals => "Сделки",
            Theme::Deck => "Колода",
            Theme::Land => "Земля",
            Theme::People => "Люди",
        }
    }

    /// The tab a wish sits in (besides `Liked`).
    fn of(kind: WishKind) -> Theme {
        use WishKind::*;
        match kind {
            Strength | Peace | Bless | Forge | Treasure | Ordeal | Beast | Fortune | Doom => {
                Theme::Me
            }
            Weaken | Secret | Hand | Blight | Poison | Undead | Guard => Theme::Rivals,
            Truce | Swap | Tribute | Wager | Debt => Theme::Deals,
            Hallow | Rot | Plant | Foresee => Theme::Deck,
            Land | Dead | Rise | Veil | Unveil | Cut | Stones | River | Flood | Road | Fire
            | Awaken | WakeGrove => Theme::Land,
            Settle | Build | Sway | Harvest | Fair => Theme::People,
        }
    }

    /// The wishes on this tab; `Liked` needs the god.
    fn kinds(self, game: &necromy_rules::Game, god: Option<God>) -> Vec<WishKind> {
        match self {
            Theme::Liked => {
                let Some(god) = god else { return Vec::new() };
                let mut liked: Vec<WishKind> = WishKind::ALL
                    .into_iter()
                    .filter(|&k| necromy_rules::taste_for(god, k) > 0)
                    .collect();
                // Stable: the best grade first, else the usual order.
                liked.sort_by_key(|&k| std::cmp::Reverse(game.wish_grade(god, k)));
                liked
            }
            theme => WishKind::ALL
                .into_iter()
                .filter(|&k| Theme::of(k) == theme)
                .collect(),
        }
    }
}

/// How the god would grade a wish (§7.4), as stars out of three.
fn stars(grade: u8) -> String {
    (0..3).map(|i| if i < grade { '★' } else { '☆' }).collect()
}

fn spawn(mut commands: Commands) {
    for (panel, z) in [(0, 15), (1, 12), (2, 12)] {
        let node = Node {
            position_type: PositionType::Absolute,
            top: px(90.0),
            left: px(0.0),
            right: px(0.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::FlexStart,
            ..default()
        };
        let mut e = commands.spawn((node, GlobalZIndex(z), Visibility::Hidden));
        match panel {
            0 => e.insert(WishPanel),
            1 => e.insert(ReplyPanel),
            _ => e.insert(WatchPanel),
        };
    }
}

fn button(
    commands: &mut Commands,
    font: &UiFont,
    action: WishButton,
    text: &str,
    on: bool,
) -> Entity {
    let b = commands
        .spawn((
            action,
            Button,
            Node {
                padding: UiRect::axes(px(12.0), px(7.0)),
                align_items: AlignItems::Center,
                column_gap: px(4.0),
                ..default()
            },
            Frame::Button,
            Accent(if on { GOLD } else { BRONZE_RIM }),
        ))
        .id();
    let t = stats::label(commands, font, text, 13.0, on);
    commands.entity(b).add_child(t);
    b
}

fn text_block(
    commands: &mut Commands,
    font: &UiFont,
    text: &str,
    size: f32,
    color: Color,
) -> Entity {
    commands
        .spawn((
            Text::new(text.to_string()),
            font.text(size),
            TextColor(color),
            Node {
                width: px(610.0),
                ..default()
            },
        ))
        .id()
}

/// Whether the wish panel is up: the human may still wish, and opened it or
/// dusk waits for them.
fn panel_up(game: &Match, draft: &WishDraft) -> bool {
    let g = &game.game;
    g.may_wish(game.human)
        && !(game.autoplay && !game.paused_for_wish_panel())
        && (draft.open || g.wishing().contains(&game.human))
}

/// Is the human writing a wish right now (panel up, free words, not sent)?
fn writing(game: &Match, draft: &WishDraft) -> bool {
    panel_up(game, draft)
        && game.oracle.online
        && !draft.prepared
        && game.oracle.listening.is_none()
}

#[allow(clippy::too_many_arguments)]
fn rebuild_panel(
    mut commands: Commands,
    game: Res<Match>,
    draft: Res<WishDraft>,
    art: Res<StatArt>,
    font: Res<UiFont>,
    panel: Single<(Entity, &mut Visibility), With<WishPanel>>,
    focus: Res<crate::tutorial::Focus>,
) {
    let (panel, mut visibility) = panel.into_inner();
    commands.entity(panel).despawn_related::<Children>();
    let g = &game.game;
    if !panel_up(&game, &draft) {
        visibility.set_if_neq(Visibility::Hidden);
        return;
    }
    let crowned = g.dominant() == Some(game.human);
    let dusk = g.wishing().contains(&game.human);
    visibility.set_if_neq(Visibility::Inherited);
    // The tutorial may point at a prepared wish: then the prepared ones.
    let pointed = focus.wish;
    let free = game.oracle.online && !draft.prepared && pointed.is_none();

    let frame = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(8.0),
                padding: UiRect::all(px(22.0)),
                width: px(640.0),
                ..default()
            },
            Frame::Plate,
        ))
        .id();
    let mut rows = Vec::new();

    let title = stats::row(&mut commands);
    if crowned {
        let crown = stats::icon_node(
            &mut commands,
            art.icon(crate::icons::StatIcon::Crown),
            28.0,
            true,
        );
        commands.entity(title).add_child(crown);
    }
    let heading = match (dusk, crowned) {
        (true, true) => "Закат: Венец у тебя, боги ждут твоего желания",
        (true, false) => "Закат: боги ждут твоего желания",
        (false, true) => "Венец у тебя: желание на закате",
        (false, false) => "Желание на закате",
    };
    let text = stats::label(&mut commands, &font, heading, 18.0, true);
    commands.entity(title).add_child(text);
    rows.push(title);

    // The god is thinking: show the words and wait.
    if let Some(god) = game.oracle.listening {
        let words = text_block(
            &mut commands,
            &font,
            &format!("«{}»", draft.text),
            14.0,
            INK,
        );
        let listening = commands
            .spawn((
                Listening(god),
                Text::new(format!("{} слушает…", names::god(god))),
                font.bold(14.0),
                TextColor(VOICE),
            ))
            .id();
        rows.extend([words, listening]);
        commands.entity(frame).add_children(&rows);
        commands.entity(panel).add_child(frame);
        return;
    }

    let hint = if free {
        "Выбери бога и напиши желание своими словами. Бог исполнит по-своему: изощрённое вознаградит Стилем, грубое исполнит урезанно и с проклятием."
    } else {
        "Бог исполнит по-своему. Изощрённое желание вознаграждается Стилем, грубое исполнится урезанно и с проклятием."
    };
    rows.push(text_block(&mut commands, &font, hint, 12.0, DIM));
    let when = if crowned {
        "Боги ответят на закате: сначала тем, у кого меньше Стиля, Венцу последним. Венцу бог даёт больше, но за грубое берёт вдвое."
    } else {
        "Боги ответят на закате: сначала тем, у кого меньше Стиля, Венцу последним."
    };
    rows.push(text_block(&mut commands, &font, when, 12.0, DIM));
    if let Some(why) = &game.oracle.failed {
        rows.push(text_block(
            &mut commands,
            &font,
            &format!("Прошлое желание не услышано: {why}. Попробуй ещё раз или выбери заготовку."),
            12.0,
            Color::srgb(0.95, 0.5, 0.4),
        ));
    }

    // Gods.
    let gods = commands
        .spawn(Node {
            flex_wrap: FlexWrap::Wrap,
            column_gap: px(6.0),
            row_gap: px(6.0),
            ..default()
        })
        .id();
    for god in God::ALL {
        let b = button(
            &mut commands,
            &font,
            WishButton::God(god),
            &format!(
                "{} · {}{}",
                names::god(god),
                names::stage(god, g.stage(god)),
                if pointed.is_some_and(|(p, _)| p == god) && draft.god != Some(god) {
                    " ←"
                } else {
                    ""
                }
            ),
            draft.god == Some(god),
        );
        let icon = stats::icon_node(&mut commands, art.gods[god.index()].clone(), 20.0, true);
        commands.entity(b).insert_children(0, &[icon]);
        commands.entity(gods).add_child(b);
    }
    rows.push(gods);
    if let Some(god) = draft.god {
        rows.push(text_block(
            &mut commands,
            &font,
            names::god_likes(god),
            12.0,
            VOICE,
        ));
    }

    if free {
        // The words, typed straight in.
        let field = commands
            .spawn((
                Node {
                    border: UiRect::all(px(4.0)),
                    // Room for the longest wish (`MAX_WISH`), four lines.
                    height: px(84.0),
                    width: px(610.0),
                    overflow: Overflow::clip(),
                    ..default()
                },
                Frame::Inset,
            ))
            .id();
        let (shown, color) = if draft.text.is_empty() {
            (
                "Например: «накорми меня досыта перед боем»▏".to_string(),
                DIM,
            )
        } else {
            (format!("{}▏", draft.text), INK)
        };
        let words = commands
            .spawn((
                Text::new(shown),
                font.text(15.0),
                TextColor(color),
                Node {
                    // Out of the field's layout: measured at min-content
                    // width, wrapped text would grow the field by a line
                    // per key (CLAUDE.md, `TextLayout::no_wrap`).
                    position_type: PositionType::Absolute,
                    left: px(8.0),
                    top: px(6.0),
                    width: px(590.0),
                    ..default()
                },
            ))
            .id();
        commands.entity(field).add_child(words);
        rows.push(field);
    } else {
        // Prepared wishes, in tabs. The tutorial may point at one.

        let theme = draft.theme.unwrap_or_else(|| match (pointed, draft.god) {
            (Some((_, kind)), _) => Theme::of(kind),
            (None, Some(_)) => Theme::Liked,
            (None, None) => Theme::Me,
        });
        let tabs = commands
            .spawn(Node {
                flex_wrap: FlexWrap::Wrap,
                column_gap: px(4.0),
                row_gap: px(4.0),
                ..default()
            })
            .id();
        for t in Theme::ALL {
            let count = t.kinds(g, draft.god).len();
            if count == 0 {
                continue;
            }
            let here = pointed.is_some_and(|(_, k)| Theme::of(k) == t) && t != theme;
            let label = if here {
                format!("{} · {count} ←", t.label())
            } else {
                format!("{} · {count}", t.label())
            };
            let b = button(
                &mut commands,
                &font,
                WishButton::Theme(t),
                &label,
                t == theme,
            );
            commands.entity(tabs).add_child(b);
        }
        rows.push(tabs);
        let kinds = commands
            .spawn((
                Node {
                    flex_wrap: FlexWrap::Wrap,
                    column_gap: px(6.0),
                    row_gap: px(6.0),
                    padding: UiRect::all(px(8.0)),
                    width: px(610.0),
                    ..default()
                },
                Frame::Inset,
            ))
            .id();
        for kind in theme.kinds(g, draft.god) {
            let mut text = format!("«{}»", names::wish(kind));
            if let Some(god) = draft.god {
                text.push(' ');
                text.push_str(&stars(g.wish_grade(god, kind)));
            }
            if kind.is_crude() {
                text.push_str(" · грубое");
            }
            if pointed.is_some_and(|(_, k)| k == kind) {
                text.push_str(" ←");
            }
            let b = button(
                &mut commands,
                &font,
                WishButton::Kind(kind),
                &text,
                draft.kind == Some(kind),
            );
            commands.entity(kinds).add_child(b);
        }
        rows.push(kinds);
        let about = match (draft.kind, draft.god) {
            (Some(kind), Some(god)) => {
                let taste = match necromy_rules::taste_for(god, kind) {
                    1.. => format!(" {} это по вкусу.", names::god(god)),
                    ..=-1 => format!(" {} такое не любит.", names::god(god)),
                    0 => String::new(),
                };
                format!("{}{taste}", names::wish_effect(kind))
            }
            (Some(kind), None) => names::wish_effect(kind).to_string(),
            (None, Some(_)) => {
                "Звёзды — как бог оценит желание: по вкусу и впервые — ★★★, повтор или нелюбимое — меньше. Чем выше оценка, тем полнее исполнение и больше Стиля."
                    .to_string()
            }
            (None, None) => "Сначала выбери бога: у каждого свой вкус.".to_string(),
        };
        rows.push(text_block(&mut commands, &font, &about, 12.0, VOICE));

        if draft.kind.is_some_and(WishKind::needs_target) {
            let rivals = stats::row(&mut commands);
            let label = stats::label(&mut commands, &font, "Кого:", 13.0, false);
            commands.entity(rivals).add_child(label);
            for p in g.players().filter(|&p| p != game.human) {
                let b = button(
                    &mut commands,
                    &font,
                    WishButton::Target(p),
                    &game.name(p),
                    draft.target == Some(p),
                );
                let face = commands
                    .spawn((
                        ImageNode::new(art.portraits[p.0 as usize].clone()),
                        Node {
                            width: px(12.0),
                            height: px(18.0),
                            ..default()
                        },
                    ))
                    .id();
                commands.entity(b).insert_children(0, &[face]);
                commands.entity(rivals).add_child(b);
            }
            rows.push(rivals);
        }
    }

    // Make, switch, refuse.
    let ready = draft.god.is_some()
        && if free {
            !draft.text.trim().is_empty()
        } else {
            draft.kind.is_some()
                && (draft.target.is_some() || !draft.kind.is_some_and(WishKind::needs_target))
        };
    let actions = stats::row(&mut commands);
    let make_label = if free {
        "Загадать (Enter)"
    } else {
        "Загадать"
    };
    let make = button(&mut commands, &font, WishButton::Make, make_label, ready);
    commands.entity(actions).add_child(make);
    if game.oracle.online {
        let (switch, label) = if free {
            (true, "Заготовки")
        } else {
            (false, "Своими словами")
        };
        let b = button(
            &mut commands,
            &font,
            WishButton::Prepared(switch),
            label,
            false,
        );
        commands.entity(actions).add_child(b);
    }
    let refuse = button(
        &mut commands,
        &font,
        WishButton::Refuse,
        if crowned {
            "Ничего не просить (Венцу +2 Угрозы)"
        } else {
            "Ничего не просить"
        },
        false,
    );
    commands.entity(actions).add_child(refuse);
    if !dusk {
        let later = button(&mut commands, &font, WishButton::Later, "Позже", false);
        commands.entity(actions).add_child(later);
    }
    rows.push(actions);
    if !game.oracle.online {
        rows.push(text_block(
            &mut commands,
            &font,
            "Голос богов не отвечает (scripts/oracle-server.sh): только заготовки.",
            11.0,
            DIM,
        ));
    } else if game.oracle.spare {
        rows.push(text_block(
            &mut commands,
            &font,
            "Голос богов слабеет: говорит запасная модель, она медленнее и понимает хуже.",
            11.0,
            DIM,
        ));
    }

    commands.entity(frame).add_children(&rows);
    commands.entity(panel).add_child(frame);
}

/// Dev aid: `NECROMY_WISH_PANEL=1` opens the wish panel once the human may
/// wish, `=1:<god index>` also picks that god (for screenshots).
fn dev_panel(game: Res<Match>, mut draft: ResMut<WishDraft>, mut done: Local<bool>) {
    if *done || !game.game.may_wish(game.human) {
        return;
    }
    *done = true;
    let Ok(want) = std::env::var("NECROMY_WISH_PANEL") else {
        return;
    };
    draft.open = true;
    draft.prepared = true;
    draft.god = want
        .split_once(':')
        .and_then(|(_, g)| God::ALL.get(g.parse::<usize>().ok()?).copied());
}

/// Dev aid: `NECROMY_WISH=<god index>:<words>` writes the human's first wish
/// once the gods' voice is up, e.g. `NECROMY_WISH="1:накорми меня перед боем"`
/// with `NECROMY_AUTOPLAY=1 NECROMY_SCREENSHOT_WHEN=reply`. It is typed out
/// letter by letter, as a person would (the others watch it being written),
/// then sent.
fn dev_wish(
    time: Res<Time>,
    mut typing: Local<Option<(f32, bool)>>,
    mut draft: ResMut<WishDraft>,
    mut game: ResMut<Match>,
) {
    if typing.is_some_and(|(_, sent)| sent) || !writing(&game, &draft) {
        return;
    }
    let Some((god, text)) = std::env::var("NECROMY_WISH").ok().and_then(|w| {
        let (g, t) = w.split_once(':')?;
        let god = God::ALL.get(g.parse::<usize>().ok()?).copied()?;
        Some((god, t.to_string()))
    }) else {
        return;
    };
    let now = time.elapsed_secs();
    let (start, _) = *typing.get_or_insert((now, false));
    let letters = ((now - start) * DEV_LETTERS_PER_SEC) as usize;
    let shown: String = text.chars().take(letters).collect();
    if draft.god != Some(god) {
        draft.god = Some(god);
    }
    if draft.text != shown {
        draft.text = shown;
    }
    let done_at = text.chars().count() as f32 / DEV_LETTERS_PER_SEC + 0.5;
    if now - start >= done_at {
        *typing = Some((start, true));
        game.wish_in_words(god, &text);
    }
}

/// Keys go into the wish while the human writes one; Enter sends it.
fn type_wish(
    mut keys: MessageReader<KeyboardInput>,
    mut draft: ResMut<WishDraft>,
    mut game: ResMut<Match>,
    mut tones: MessageWriter<Tone>,
    mut typing: ResMut<Typing>,
) {
    let now = writing(&game, &draft);
    if typing.0 != now {
        typing.0 = now;
    }
    if !now {
        keys.clear();
        return;
    }
    for key in keys.read() {
        if key.state != ButtonState::Pressed {
            continue;
        }
        match &key.logical_key {
            Key::Backspace => {
                if draft.text.pop().is_some() {
                    tones.write(Tone::key(true));
                }
            }
            Key::Enter => {
                if let Some(god) = draft.god
                    && !draft.text.trim().is_empty()
                {
                    let text = draft.text.clone();
                    game.wish_in_words(god, &text);
                }
            }
            _ => {
                if let Some(text) = &key.text
                    && text.chars().all(|c| !c.is_control())
                    && draft.text.chars().count() < MAX_WISH
                {
                    draft.text.push_str(text);
                    tones.write(Tone::key(false));
                }
            }
        }
    }
}

fn buttons(
    pressed: Query<(&Interaction, &WishButton), Changed<Interaction>>,
    mut draft: ResMut<WishDraft>,
    mut game: ResMut<Match>,
) {
    for (interaction, button) in &pressed {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let human = game.human;
        match *button {
            WishButton::God(god) => draft.god = Some(god),
            WishButton::Kind(kind) => {
                draft.kind = Some(kind);
                if !kind.needs_target() {
                    draft.target = None;
                }
            }
            WishButton::Target(p) => draft.target = Some(p),
            WishButton::Theme(theme) => draft.theme = Some(theme),
            WishButton::Prepared(on) => draft.prepared = on,
            WishButton::Open => draft.open = true,
            WishButton::Later => draft.open = false,
            WishButton::Make => {
                let Some(god) = draft.god else { continue };
                if game.oracle.online && !draft.prepared {
                    if !draft.text.trim().is_empty() && game.oracle.listening.is_none() {
                        let text = draft.text.clone();
                        game.wish_in_words(god, &text);
                    }
                    continue;
                }
                let Some(kind) = draft.kind else { continue };
                // A prepared wish: one act, no price (§7.7).
                let Some(act) = necromy_rules::Act::of(kind, draft.target) else {
                    continue;
                };
                if game
                    .act(
                        human,
                        Intent::Wish {
                            god,
                            wish: necromy_rules::Wish::one(act),
                            said: None,
                        },
                    )
                    .is_ok()
                {
                    *draft = WishDraft::default();
                }
            }
            WishButton::Refuse => {
                if game.act(human, Intent::RefuseWish).is_ok() {
                    *draft = WishDraft::default();
                }
            }
        }
    }
    // A wish sealed (or the night come) clears the draft for the next day.
    let done = !game.game.may_wish(game.human)
        && game.oracle.listening.is_none()
        && (!draft.text.is_empty() || draft.god.is_some() || draft.open);
    if done {
        *draft = WishDraft::default();
    }
}

fn listening_dots(time: Res<Time>, mut texts: Query<(&Listening, &mut Text)>) {
    let dots = ".".repeat(1 + (time.elapsed_secs() * 2.0) as usize % 3);
    for (listening, mut text) in &mut texts {
        let wanted = format!("{} слушает{dots}", names::god(listening.0));
        if text.0 != wanted {
            text.0 = wanted;
        }
    }
}

/// Who asked whom for what, the grade, the god's words, what came of it.
fn rebuild_reply(
    mut commands: Commands,
    game: Res<Match>,
    art: Res<StatArt>,
    font: Res<UiFont>,
    panel: Single<(Entity, &mut Visibility), With<ReplyPanel>>,
) {
    let (panel, mut visibility) = panel.into_inner();
    commands.entity(panel).despawn_related::<Children>();
    // A fight or a trial on screen owns the centre: the answers wait.
    let Some(reply) = game
        .wish_reply
        .as_ref()
        .filter(|_| game.battle.is_none() && game.trial.is_none())
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
                padding: UiRect::all(px(20.0)),
                width: px(540.0),
                ..default()
            },
            Frame::Plate,
        ))
        .id();
    let mut rows = Vec::new();
    let block = |commands: &mut Commands, text: String, size: f32, color: Color| {
        commands
            .spawn((
                Text::new(text),
                font.text(size),
                TextColor(color),
                Node {
                    width: px(510.0),
                    ..default()
                },
            ))
            .id()
    };
    match reply.wish {
        Some((god, ref wish, grade, dropped)) => {
            let head = stats::row(&mut commands);
            let icon = stats::icon_node(&mut commands, art.gods[god.index()].clone(), 28.0, true);
            let who = stats::label(
                &mut commands,
                &font,
                &format!(
                    "{} просит {}",
                    game.name(reply.player),
                    names::god_accusative(god)
                ),
                15.0,
                true,
            );
            let stars = stats::label(
                &mut commands,
                &font,
                &format!(
                    "{}{}",
                    "★".repeat(grade as usize),
                    "☆".repeat(3 - grade as usize)
                ),
                16.0,
                true,
            );
            commands.entity(head).add_children(&[icon, who, stars]);
            rows.push(head);
            // The words: the player's own, or the prepared phrase.
            let phrase = names::wish_phrase(wish);
            let asked = match &reply.said {
                Some(said) => format!("«{}» — понято как «{phrase}»", said.text),
                None => format!("«{phrase}»"),
            };
            rows.push(block(&mut commands, asked, 14.0, INK));
            // The sacrifice, and what the grade could not cover.
            if let Some(price) = wish.price {
                let paid = format!(
                    "Отдано богу: {}.",
                    names::price(price, |c| game.game.def(c).name.to_string())
                );
                rows.push(block(&mut commands, paid, 12.0, INK));
            }
            if dropped > 0 {
                rows.push(block(
                    &mut commands,
                    "Всего бог не дал: оценка не покрыла второе.".into(),
                    12.0,
                    INK,
                ));
            }
            // The god's answer: the model's, then any written later, then the template.
            let speech = reply
                .said
                .as_ref()
                .map(|s| s.speech.clone())
                .filter(|s| !s.is_empty())
                .or_else(|| {
                    let key = (reply.serial, reply.player);
                    game.oracle.wish_voices.get(&key).cloned()
                })
                .unwrap_or_else(|| names::god_speech(god, grade).to_string());
            // Typed out in the god's voice.
            let head = format!("{}: «", names::god(god));
            let spoken = block(&mut commands, head.clone(), 13.0, VOICE);
            commands.entity(spoken).insert(Speech {
                key: u64::from(reply.serial),
                god,
                from: head.chars().count(),
                text: format!("{head}{speech}»"),
            });
            rows.push(spoken);
            if let Some(said) = &reply.said
                && !said.reason.is_empty()
            {
                rows.push(block(
                    &mut commands,
                    format!("Оценка: {}", said.reason),
                    12.0,
                    DIM,
                ));
            }
        }
        None => {
            let text = stats::label(
                &mut commands,
                &font,
                &format!("{} ничего не просит.", game.name(reply.player)),
                15.0,
                true,
            );
            rows.push(text);
        }
    }
    for line in &reply.lines {
        rows.push(block(&mut commands, format!("→ {line}"), 12.0, INK));
    }
    // Leaf through this dusk's answers, one's own first; put them away.
    let nav = commands
        .spawn(Node {
            column_gap: px(8.0),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::FlexEnd,
            margin: UiRect::top(px(6.0)),
            ..default()
        })
        .id();
    let count = game.wish_replies.len();
    let small = |commands: &mut Commands, what: ReplyButton, label: &str, on: bool| {
        let b = commands
            .spawn((
                what,
                Button,
                Node {
                    padding: UiRect::axes(px(12.0), px(5.0)),
                    ..default()
                },
                if on { Frame::Button } else { Frame::ButtonOff },
            ))
            .id();
        let t = commands
            .spawn((Text::new(label), font.bold(13.0), TextColor(INK)))
            .id();
        commands.entity(b).add_child(t);
        b
    };
    if count > 1 {
        let back = small(&mut commands, ReplyButton::Back, "◀", game.wish_page > 0);
        let page = stats::label(
            &mut commands,
            &font,
            &format!("{} из {count}", game.wish_page + 1),
            13.0,
            true,
        );
        let on = small(
            &mut commands,
            ReplyButton::On,
            "▶",
            game.wish_page + 1 < count,
        );
        commands.entity(nav).add_children(&[back, page, on]);
    }
    let close = small(&mut commands, ReplyButton::Close, "Понятно", true);
    commands.entity(nav).add_child(close);
    rows.push(nav);
    commands.entity(frame).add_children(&rows);
    commands.entity(panel).add_child(frame);
}

/// Autoplay leafs through the answers by itself; a person does it by hand.
fn expire_reply(time: Res<Time>, mut shown: Local<(u32, f32)>, mut game: ResMut<Match>) {
    if game.wish_reply.is_none() || !game.autoplay {
        return;
    }
    let now = time.elapsed_secs();
    if shown.0 != game.wish_serial {
        *shown = (game.wish_serial, now);
    }
    if now - shown.1 > REPLY_SECS {
        game.next_wish_reply();
    }
}

/// Sends the human's wish to the table as it changes, for the others to
/// watch.
fn share_draft(
    draft: Res<WishDraft>,
    mut game: ResMut<Match>,
    mut sent: Local<Option<(Option<God>, String)>>,
) {
    if !writing(&game, &draft) {
        *sent = None;
        return;
    }
    let now = (draft.god, draft.text.clone());
    if sent.as_ref() != Some(&now) {
        // Sending changes nothing on screen: no redraw for it.
        game.bypass_change_detection().draft_wish(now.0, &now.1);
        *sent = Some(now);
    }
}

/// Rivals' wishes as they are written: a key for every new letter, a
/// softer one for every deletion.
fn watch_draft(game: Res<Match>, mut heard: Local<String>, mut tones: MessageWriter<Tone>) {
    let text: String = game
        .drafting
        .iter()
        .filter(|(p, _)| **p != game.human)
        .map(|(_, (_, t))| t.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    if text == *heard {
        return;
    }
    let (now, before) = (text.chars().count(), heard.chars().count());
    let common = text
        .chars()
        .zip(heard.chars())
        .take_while(|(a, b)| a == b)
        .count();
    // A burst (a paste, a late message) is heard as a few keys, not all.
    for _ in 0..(before - common).min(3) {
        tones.write(Tone::key(true));
    }
    for _ in 0..(now - common).min(3) {
        tones.write(Tone::key(false));
    }
    *heard = text;
}

/// Under the action bar: where the human's own wish stands, and the rivals'
/// wishes as they write them (§21.4). Hidden while the panel or an answer
/// has the place.
fn rebuild_watch(
    mut commands: Commands,
    game: Res<Match>,
    draft: Res<WishDraft>,
    art: Res<StatArt>,
    font: Res<UiFont>,
    panel: Single<(Entity, &mut Visibility), With<WatchPanel>>,
) {
    let (panel, mut visibility) = panel.into_inner();
    commands.entity(panel).despawn_related::<Children>();
    let g = &game.game;
    let me = game.human;
    let writers: Vec<(PlayerId, Option<God>, &str)> = game
        .drafting
        .iter()
        .filter(|(p, (_, t))| **p != me && !t.trim().is_empty())
        .map(|(p, (god, t))| (*p, *god, t.as_str()))
        .collect();
    let own = g.may_wish(me) || matches!(g.seal(me), Seal::Wish(_) | Seal::Refused);
    let busy = game.wish_reply.is_some() || panel_up(&game, &draft) || game.autoplay;
    if busy || (!own && writers.is_empty()) || g.winner().is_some() {
        visibility.set_if_neq(Visibility::Hidden);
        return;
    }
    visibility.set_if_neq(Visibility::Inherited);
    let frame = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(4.0),
                padding: UiRect::axes(px(14.0), px(10.0)),
                width: px(540.0),
                ..default()
            },
            Frame::Panel,
        ))
        .id();
    let mut rows = Vec::new();
    if own {
        let row = stats::row(&mut commands);
        let (text, open) = match g.seal(me) {
            Seal::Open if g.wishing().contains(&me) => ("Закат ждёт твоего желания.", true),
            Seal::Open => ("Желание на закате ещё не загадано.", true),
            Seal::Refused => ("На закате ты ничего не просишь.", false),
            Seal::Wish(_) => ("Желание запечатано: бог ответит на закате.", false),
        };
        let label = stats::label(&mut commands, &font, text, 13.0, open);
        commands.entity(row).add_child(label);
        if open {
            let b = button(
                &mut commands,
                &font,
                WishButton::Open,
                "Загадать желание",
                true,
            );
            commands.entity(row).add_child(b);
        }
        rows.push(row);
    }
    for (writer, god, text) in writers {
        let row = stats::row(&mut commands);
        let face = commands
            .spawn((
                ImageNode::new(art.portraits[writer.0 as usize].clone()),
                Node {
                    width: px(12.0),
                    height: px(18.0),
                    ..default()
                },
            ))
            .id();
        let who = match god {
            Some(god) => format!("{} → {}:", game.name(writer), names::god_dative(god)),
            None => format!("{}:", game.name(writer)),
        };
        let who = stats::label(&mut commands, &font, &who, 12.0, true);
        let words = commands
            .spawn((
                Text::new(format!("«{text}▌»")),
                font.text(12.0),
                TextColor(INK),
                Node {
                    max_width: px(400.0),
                    ..default()
                },
            ))
            .id();
        commands.entity(row).add_children(&[face, who, words]);
        rows.push(row);
    }
    commands.entity(frame).add_children(&rows);
    commands.entity(panel).add_child(frame);
}

/// The buttons under the gods' answers.
fn reply_buttons(
    pressed: Query<(&Interaction, &ReplyButton), Changed<Interaction>>,
    mut game: ResMut<Match>,
) {
    for (interaction, button) in &pressed {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match button {
            ReplyButton::Back if game.wish_page > 0 => {
                let page = game.wish_page - 1;
                game.show_wish_page(page);
            }
            ReplyButton::On if game.wish_page + 1 < game.wish_replies.len() => {
                let page = game.wish_page + 1;
                game.show_wish_page(page);
            }
            ReplyButton::Close => game.close_wish_replies(),
            _ => {}
        }
    }
}

/// The strip of wishes stands just under the action bar and the clock,
/// however tall they are (a waiting line, a clock in a network match).
#[allow(clippy::type_complexity)]
fn place_watch(
    above: Query<
        (&ComputedNode, &UiGlobalTransform, &InheritedVisibility),
        Or<(
            With<crate::turn_ui::ActionBar>,
            With<crate::turn_ui::ClockChip>,
        )>,
    >,
    mut watch: Single<&mut Node, With<WatchPanel>>,
) {
    let bottom = above
        .iter()
        .filter(|(c, _, v)| v.get() && c.size().y > 0.0)
        .map(|(c, at, _)| {
            let k = c.inverse_scale_factor();
            (at.affine().translation.y + c.size().y / 2.0) * k
        })
        .fold(84.0_f32, f32::max);
    let top = px((bottom + 6.0).round());
    if watch.top != top {
        watch.top = top;
    }
}
