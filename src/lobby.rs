//! The front of the game: the menu and the lobby (docs/design.md §17).
//!
//! Before a match there is no `Match`: this screen owns the window. From
//! the menu one plays alone (a table in this process) or with friends: open
//! a table on a server and tell them its code, or sit at theirs by code. In
//! the lobby everyone picks a god; the one who opened the table starts, and
//! bots take the free seats. When the server sends the first view, the
//! `Match` is inserted and the board takes over.
//!
//! Dev aids: `NECROMY_PLAY=local|menu|create|join:CODE` skips the clicks,
//! `NECROMY_START_AT=n` makes the opener start once n people sit, and
//! `NECROMY_SERVER`, `NECROMY_NAME` fill the fields.

use bevy::input::ButtonState;
use bevy::input::keyboard::{Key, KeyboardInput};
use bevy::prelude::*;
use necromy_host::FromTable;
use necromy_net::{ClientConn, ClientMsg, LobbyInfo, ServerMsg, normalize_code, spoken_code};
use necromy_rules::{God, PlayerId};

use crate::god_pick::{self, GodPickArt, Holder};
use crate::hud::{INK, UiFont};
use crate::names;
use crate::play::Match;
use crate::ui_skin::{Accent, BRONZE_RIM, Frame};

const GOLD: Color = Color::srgb(1.0, 0.82, 0.3);
const DIM: Color = Color::srgb(0.72, 0.70, 0.64);
const BAD: Color = Color::srgb(0.95, 0.5, 0.4);
const MAX_FIELD: usize = 40;

pub struct LobbyPlugin;

impl Plugin for LobbyPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Front::from_env())
            .add_systems(Startup, spawn)
            .add_systems(
                Update,
                (
                    dev_play,
                    poll_server,
                    type_field,
                    buttons,
                    rebuild.run_if(resource_changed::<Front>),
                )
                    .chain()
                    .run_if(not(resource_exists::<Match>)),
            )
            .add_systems(Update, hide.run_if(resource_added::<Match>));
    }
}

/// With a dev variable set and no `NECROMY_PLAY`, go straight to a single
/// player match, as dev runs always did.
pub fn skip_menu() -> bool {
    match std::env::var("NECROMY_PLAY").as_deref() {
        Ok("local") => true,
        Ok(_) => false,
        Err(_) => [
            "NECROMY_SEED",
            "NECROMY_AUTOPLAY",
            "NECROMY_SCREENSHOT",
            "NECROMY_WISH",
        ]
        .iter()
        .any(|v| std::env::var_os(v).is_some()),
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Field {
    Name,
    Server,
    Code,
}

#[derive(Resource)]
pub struct Front {
    name: String,
    server: String,
    code: String,
    focus: Field,
    conn: Option<ClientConn>,
    lobby: Option<LobbyInfo>,
    error: Option<String>,
    /// Our seat, once the match began; the first view follows it.
    seat: Option<PlayerId>,
    first: Vec<FromTable>,
    /// `NECROMY_START_AT`: the opener starts once this many sit.
    start_at: Option<usize>,
    asked_start: bool,
    /// A saved way back to a running match.
    ticket: Option<Ticket>,
    /// The ticket the server handed out with our seat.
    ticket_no: u64,
    /// We dialled to sit back down by ticket.
    returning: bool,
    /// Setting up a single player match, and the god taken for it.
    alone: bool,
    alone_god: God,
    /// The world the single player match begins with (§21).
    alone_mode: necromy_rules::Mode,
    /// The tutorial's chapters are listed.
    tutorial: bool,
    /// The newest single player match on disk and a line about it.
    saved: Option<(std::path::PathBuf, String)>,
}

impl Front {
    fn from_env() -> Front {
        let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        Front {
            name: env("NECROMY_NAME")
                .or_else(|| env("USER"))
                .or_else(|| env("USERNAME"))
                .unwrap_or_else(|| "Игрок".into()),
            server: env("NECROMY_SERVER").unwrap_or_else(|| necromy_net::PUBLIC_SERVER.into()),
            code: String::new(),
            focus: Field::Name,
            conn: None,
            lobby: None,
            error: None,
            seat: None,
            first: Vec::new(),
            start_at: env("NECROMY_START_AT").and_then(|n| n.parse().ok()),
            asked_start: false,
            ticket: Ticket::load(),
            ticket_no: 0,
            returning: false,
            alone: std::env::var_os("NECROMY_PLAY").is_some_and(|v| v == "alone"),
            alone_god: crate::play::default_god(),
            alone_mode: match std::env::var("NECROMY_MODE").as_deref() {
                Ok("full") => necromy_rules::Mode::Full,
                _ => necromy_rules::Mode::Creation,
            },
            tutorial: std::env::var_os("NECROMY_PLAY").is_some_and(|v| v == "tutorial"),
            saved: crate::saves::latest(),
        }
    }

    /// Dev aid for screenshots: the lobby holds everyone expected.
    pub fn lobby_full(&self) -> bool {
        self.lobby
            .as_ref()
            .is_some_and(|l| l.people.len() >= self.start_at.unwrap_or(1))
    }

    fn field(&mut self, field: Field) -> &mut String {
        match field {
            Field::Name => &mut self.name,
            Field::Server => &mut self.server,
            Field::Code => &mut self.code,
        }
    }

    /// Dial the server and ask for a table: a new one, or the one by code.
    fn dial(&mut self, then: ClientMsg) {
        self.error = None;
        self.conn = None;
        self.lobby = None;
        match necromy_net::connect(&self.server, &self.name) {
            Ok(conn) => {
                conn.send(then);
                self.conn = Some(conn);
            }
            Err(err) => {
                self.error = Some(format!(
                    "Сервер {} не отвечает: {err}",
                    necromy_net::with_port(&self.server)
                ));
            }
        }
    }

    /// Take up the single player match kept on disk (§17.5).
    fn resume(&mut self, commands: &mut Commands) {
        let Some((dir, _)) = self.saved.clone() else {
            return;
        };
        match Match::resume(dir) {
            Ok(m) => commands.insert_resource(m),
            Err(why) => {
                self.saved = crate::saves::latest();
                self.error = Some(format!("Не вышло продолжить: {why}."));
            }
        }
    }

    /// Sit back down at the match the saved ticket names.
    fn go_back(&mut self) {
        let Some(t) = self.ticket.clone() else {
            return;
        };
        self.server = t.server;
        self.name = t.name;
        self.code = t.code.clone();
        self.returning = true;
        self.dial(ClientMsg::Rejoin {
            code: t.code,
            ticket: t.ticket,
        });
    }

    fn leave(&mut self) {
        self.conn = None;
        self.lobby = None;
        self.seat = None;
        self.first.clear();
        self.asked_start = false;
    }
}

#[derive(Component)]
struct FrontRoot;

#[derive(Component, Clone, Copy)]
enum FrontButton {
    Alone,
    /// Single player: take this god.
    Solo(God),
    /// Single player: begin with the god taken.
    Begin,
    /// Single player: the world to begin with.
    Mode(necromy_rules::Mode),
    /// Back from the single player setup to the menu.
    Back,
    Open,
    Sit,
    Focus(Field),
    Pick(Option<God>),
    Start,
    Leave,
    /// Sit back down at the match the saved ticket names.
    Return,
    /// Take up the single player match saved on disk.
    Continue,
    /// The tutorial's list of chapters.
    Tutorial,
    /// Play tutorial chapter `n` (0-based).
    Chapter(usize),
}

fn spawn(mut commands: Commands) {
    commands.spawn((
        FrontRoot,
        Node {
            position_type: PositionType::Absolute,
            width: percent(100.0),
            height: percent(100.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        // No fill: the menu's world (`menu_world.rs`) paints behind it.
        GlobalZIndex(50),
    ));
}

fn hide(mut root: Query<&mut Visibility, With<FrontRoot>>) {
    for mut v in &mut root {
        *v = Visibility::Hidden;
    }
}

/// `NECROMY_PLAY`: skip the clicks, once.
fn dev_play(mut done: Local<bool>, mut front: ResMut<Front>, mut commands: Commands) {
    if *done {
        return;
    }
    *done = true;
    match std::env::var("NECROMY_PLAY").as_deref() {
        Ok("create") => front.dial(ClientMsg::Create),
        Ok(join) if join.starts_with("join:") => {
            let code = normalize_code(&join["join:".len()..]);
            front.code = code.clone();
            front.dial(ClientMsg::Join { code });
        }
        Ok("local") => commands.insert_resource(Match::local(
            crate::play::default_god(),
            crate::play::default_mode(),
        )),
        Ok("continue") => front.resume(&mut commands),
        Ok("return") => {
            front.go_back();
        }
        Ok(play) if play.starts_with("tutorial:") => {
            let n: usize = play["tutorial:".len()..].parse().unwrap_or(1);
            let n = n.clamp(1, crate::tutorial::chapters().len()) - 1;
            let (game, lesson) = crate::tutorial::start(n);
            commands.insert_resource(game);
            commands.insert_resource(lesson);
        }
        _ => {}
    }
}

/// What the server says, and the hand-over to the match once it begins.
fn poll_server(mut front: ResMut<Front>, mut commands: Commands) {
    // Polling is not a change; only what arrives redraws the screen.
    let f = front.bypass_change_detection();
    let Some(conn) = f.conn.as_ref() else {
        return;
    };
    let mut changed = false;
    while let Some(message) = conn.poll() {
        changed = true;
        match message {
            ServerMsg::Lobby(info) => {
                if f.lobby.as_ref().is_none_or(|l| l.code != info.code) {
                    info!("at table {} ({} people)", info.code, info.people.len());
                }
                f.error = None;
                f.code = info.code.clone();
                f.lobby = Some(info);
            }
            ServerMsg::Error(e) => {
                // The seat to return to is gone: forget it.
                if f.returning {
                    f.returning = false;
                    f.ticket = None;
                    Ticket::forget();
                }
                f.error = Some(e);
            }
            ServerMsg::Started { seat, ticket } => {
                f.seat = Some(seat);
                f.ticket_no = ticket;
            }
            ServerMsg::Table(m) => f.first.push(m),
        }
    }
    if !conn.is_open() {
        changed = true;
        let was_in = f.lobby.is_some();
        f.leave();
        if f.error.is_none() {
            f.error = Some(if was_in {
                "Связь с сервером потеряна.".into()
            } else {
                "Сервер закрыл соединение.".into()
            });
        }
    }
    // Dev aid: the opener starts once everyone expected sits.
    if let (Some(n), Some(lobby), Some(conn)) = (f.start_at, f.lobby.as_ref(), f.conn.as_ref())
        && lobby.you == lobby.owner
        && lobby.people.len() >= n
        && !f.asked_start
    {
        f.asked_start = true;
        conn.send(ClientMsg::Start);
    }
    if let Some(seat) = f.seat
        && f.first
            .iter()
            .any(|m| matches!(m, FromTable::Update { .. }))
        && let Some(conn) = f.conn.take()
    {
        let first = std::mem::take(&mut f.first);
        let ticket = Ticket {
            server: f.server.clone(),
            name: f.name.clone(),
            code: f.code.clone(),
            ticket: f.ticket_no,
        };
        ticket.save();
        let mut m = Match::remote(conn, seat, first, ticket);
        if f.returning {
            f.returning = false;
            m.feed
                .push("Ты снова за столом: пока тебя не было, играл бот.".into());
        }
        commands.insert_resource(m);
    }
    if changed {
        front.set_changed();
    }
}

/// Keys go into the focused field.
fn type_field(mut keys: MessageReader<KeyboardInput>, mut front: ResMut<Front>) {
    if front.lobby.is_some() {
        keys.clear();
        return;
    }
    for key in keys.read() {
        if key.state != ButtonState::Pressed {
            continue;
        }
        let focus = front.focus;
        match &key.logical_key {
            Key::Backspace => {
                front.field(focus).pop();
            }
            Key::Tab => {
                front.focus = match focus {
                    Field::Name => Field::Server,
                    Field::Server => Field::Code,
                    Field::Code => Field::Name,
                };
            }
            Key::Enter => match focus {
                Field::Code if !front.code.trim().is_empty() => {
                    let code = normalize_code(&front.code);
                    front.dial(ClientMsg::Join { code });
                }
                _ => {}
            },
            _ => {
                if let Some(text) = &key.text
                    && text.chars().all(|c| !c.is_control())
                    && front.field(focus).chars().count() < MAX_FIELD
                {
                    front.field(focus).push_str(text);
                }
            }
        }
    }
}

fn buttons(
    pressed: Query<(&Interaction, &FrontButton), Changed<Interaction>>,
    mut front: ResMut<Front>,
    mut commands: Commands,
) {
    for (interaction, button) in &pressed {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match *button {
            FrontButton::Alone => front.alone = true,
            FrontButton::Solo(god) => front.alone_god = god,
            FrontButton::Begin => {
                commands.insert_resource(Match::local(front.alone_god, front.alone_mode))
            }
            FrontButton::Mode(mode) => match &front.conn {
                // At a table the owner asks the server; alone, it is ours.
                Some(conn) if front.lobby.is_some() => conn.send(ClientMsg::Mode(mode)),
                _ => front.alone_mode = mode,
            },
            FrontButton::Back => {
                front.alone = false;
                front.tutorial = false;
            }
            FrontButton::Tutorial => front.tutorial = true,
            FrontButton::Chapter(n) => {
                let (game, lesson) = crate::tutorial::start(n);
                commands.insert_resource(game);
                commands.insert_resource(lesson);
            }
            FrontButton::Open => front.dial(ClientMsg::Create),
            FrontButton::Sit => {
                let code = normalize_code(&front.code);
                if code.is_empty() {
                    front.error = Some("Введи код стола.".into());
                    front.focus = Field::Code;
                } else {
                    front.dial(ClientMsg::Join { code });
                }
            }
            FrontButton::Focus(field) => front.focus = field,
            FrontButton::Pick(god) => {
                if let Some(conn) = &front.conn {
                    conn.send(ClientMsg::Pick(god));
                }
            }
            FrontButton::Start => {
                if let Some(conn) = &front.conn {
                    conn.send(ClientMsg::Start);
                }
            }
            FrontButton::Leave => front.leave(),
            FrontButton::Return => {
                front.go_back();
            }
            FrontButton::Continue => front.resume(&mut commands),
        }
    }
}

fn button(
    commands: &mut Commands,
    font: &UiFont,
    action: FrontButton,
    text: &str,
    on: bool,
) -> Entity {
    let b = commands
        .spawn((
            action,
            Button,
            Node {
                padding: UiRect::axes(px(16.0), px(9.0)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            Frame::Button,
            Accent(if on { GOLD } else { BRONZE_RIM }),
        ))
        .id();
    let t = commands
        .spawn((Text::new(text.to_string()), font.bold(15.0), TextColor(INK)))
        .id();
    commands.entity(b).add_child(t);
    b
}

fn text(commands: &mut Commands, font: &UiFont, text: &str, size: f32, color: Color) -> Entity {
    commands
        .spawn((
            Text::new(text.to_string()),
            font.text(size),
            TextColor(color),
            Node {
                max_width: px(520.0),
                ..default()
            },
        ))
        .id()
}

fn field(
    commands: &mut Commands,
    font: &UiFont,
    front: &Front,
    which: Field,
    label: &str,
) -> Entity {
    let row = commands
        .spawn(Node {
            column_gap: px(10.0),
            align_items: AlignItems::Center,
            ..default()
        })
        .id();
    let caption = commands
        .spawn((
            Text::new(label.to_string()),
            font.text(14.0),
            TextColor(DIM),
            Node {
                width: px(70.0),
                ..default()
            },
        ))
        .id();
    let focused = front.focus == which;
    let value = match which {
        Field::Name => &front.name,
        Field::Server => &front.server,
        Field::Code => &front.code,
    };
    let shown = if focused {
        format!("{value}▏")
    } else {
        value.clone()
    };
    let b = commands
        .spawn((
            FrontButton::Focus(which),
            Button,
            Node {
                width: px(300.0),
                min_height: px(30.0),
                padding: UiRect::axes(px(10.0), px(7.0)),
                ..default()
            },
            Frame::Inset,
            Accent(if focused { GOLD } else { BRONZE_RIM }),
        ))
        .id();
    let t = commands
        .spawn((Text::new(shown), font.text(15.0), TextColor(INK)))
        .id();
    commands.entity(b).add_child(t);
    commands.entity(row).add_children(&[caption, b]);
    row
}

fn rebuild(
    mut commands: Commands,
    front: Res<Front>,
    font: Res<UiFont>,
    art: Res<GodPickArt>,
    root: Single<Entity, With<FrontRoot>>,
) {
    let root = *root;
    commands.entity(root).despawn_related::<Children>();
    let frame = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(if front.alone || front.lobby.is_some() {
                    7.0
                } else {
                    12.0
                }),
                padding: UiRect::all(px(if front.alone || front.lobby.is_some() {
                    22.0
                } else {
                    30.0
                })),
                // The god cards need the room; the menu keeps a column.
                width: if front.alone || front.lobby.is_some() {
                    Val::Auto
                } else {
                    px(560.0)
                },
                ..default()
            },
            Frame::Plate,
            crate::menu_stage::MenuPanel,
        ))
        .id();
    let mut rows = Vec::new();
    let title = commands
        .spawn((Text::new("Necromy Table"), font.bold(30.0), TextColor(GOLD)))
        .id();
    rows.push(title);

    match &front.lobby {
        None if front.alone => alone_rows(&mut commands, &font, &art, &front, &mut rows),
        None if front.tutorial => tutorial_rows(&mut commands, &font, &mut rows),
        None => menu(&mut commands, &font, &front, &mut rows),
        Some(lobby) => lobby_rows(&mut commands, &font, &art, lobby, &mut rows),
    }
    if let Some(e) = &front.error {
        rows.push(text(&mut commands, &font, e, 13.0, BAD));
    }
    commands.entity(frame).add_children(&rows);
    commands.entity(root).add_child(frame);
}

fn menu(commands: &mut Commands, font: &UiFont, front: &Front, rows: &mut Vec<Entity>) {
    rows.push(text(commands, font, "Стол пяти богов.", 14.0, DIM));
    rows.push(field(commands, font, front, Field::Name, "Имя"));
    if let Some(t) = &front.ticket {
        let back = button(
            commands,
            font,
            FrontButton::Return,
            &format!("Вернуться за стол {}", spoken_code(&t.code)),
            true,
        );
        rows.push(back);
        rows.push(text(
            commands,
            font,
            &format!(
                "Партия на {} ещё может идти: твоё место держит бот.",
                t.server
            ),
            12.0,
            DIM,
        ));
    }
    if let Some((_, line)) = &front.saved {
        let resume = button(commands, font, FrontButton::Continue, "Продолжить", true);
        rows.push(resume);
        rows.push(text(
            commands,
            font,
            &format!("Одиночная партия: {line}."),
            12.0,
            DIM,
        ));
    }
    let learn = button(commands, font, FrontButton::Tutorial, "Обучение", true);
    rows.push(learn);
    rows.push(text(
        commands,
        font,
        "Девять коротких глав: от первого шага до победы.",
        12.0,
        DIM,
    ));
    let alone = button(commands, font, FrontButton::Alone, "Одиночная игра", true);
    rows.push(alone);
    rows.push(text(
        commands,
        font,
        "Ты и четыре бота, стол в этой игре.",
        12.0,
        DIM,
    ));

    let head = commands
        .spawn((
            Text::new("Игра с друзьями"),
            font.bold(18.0),
            TextColor(GOLD),
            Node {
                margin: UiRect::top(px(10.0)),
                ..default()
            },
        ))
        .id();
    rows.push(head);
    rows.push(field(commands, font, front, Field::Server, "Сервер"));
    let open = button(commands, font, FrontButton::Open, "Открыть стол", true);
    rows.push(open);
    rows.push(text(
        commands,
        font,
        "Сервер даст код стола из шести цифр: продиктуй его друзьям.",
        12.0,
        DIM,
    ));
    rows.push(field(commands, font, front, Field::Code, "Код"));
    let sit = button(
        commands,
        font,
        FrontButton::Sit,
        "Сесть по коду",
        !front.code.is_empty(),
    );
    rows.push(sit);
    rows.push(text(
        commands,
        font,
        "Tab — следующее поле. Сервер по умолчанию — наш; свой: scripts/table-server.sh.",
        11.0,
        DIM,
    ));
    // Volume and the window; also F10 anywhere.
    let settings = crate::settings::open_button(commands, font, "Настройки");
    commands.entity(settings).insert(Node {
        margin: UiRect::top(px(10.0)),
        padding: UiRect::axes(px(16.0), px(9.0)),
        justify_content: JustifyContent::Center,
        ..default()
    });
    rows.push(settings);
}

/// The tutorial's chapters, the finished ones marked.
fn tutorial_rows(commands: &mut Commands, font: &UiFont, rows: &mut Vec<Entity>) {
    rows.push(text(
        commands,
        font,
        "Обучение: каждая глава — маленькая сцена с подсказками шаг за шагом.",
        14.0,
        DIM,
    ));
    let done = crate::tutorial::done_chapters();
    let first_new = (0..crate::tutorial::chapters().len()).find(|n| !done.contains(n));
    for (n, chapter) in crate::tutorial::chapters().iter().enumerate() {
        let mark = if done.contains(&n) { "  ✓" } else { "" };
        let b = button(
            commands,
            font,
            FrontButton::Chapter(n),
            &format!("{}. {}{mark}", n + 1, chapter.title),
            first_new == Some(n),
        );
        rows.push(b);
        rows.push(text(commands, font, chapter.blurb, 12.0, DIM));
    }
    let back = button(commands, font, FrontButton::Back, "Назад", false);
    rows.push(back);
}

/// Single player: pick a god, the bots take the other four.
fn alone_rows(
    commands: &mut Commands,
    font: &UiFont,
    art: &GodPickArt,
    front: &Front,
    rows: &mut Vec<Entity>,
) {
    rows.push(text(
        commands,
        font,
        "Одиночная игра: выбери бога. Остальных четверых сыграют боты.",
        14.0,
        DIM,
    ));
    let cards = god_pick::row(commands);
    for god in God::ALL {
        let holder = if god == front.alone_god {
            Holder::Mine
        } else {
            Holder::Free
        };
        let card = god_pick::god_card(
            commands,
            font,
            art,
            god,
            holder,
            Some(FrontButton::Solo(god)),
        );
        commands.entity(cards).add_child(card);
    }
    rows.push(cards);
    rows.push(mode_row(commands, font, front.alone_mode, true));
    let actions = commands
        .spawn(Node {
            column_gap: px(10.0),
            ..default()
        })
        .id();
    let begin = button(commands, font, FrontButton::Begin, "Начать", true);
    let back = button(commands, font, FrontButton::Back, "Назад", false);
    commands.entity(actions).add_children(&[begin, back]);
    rows.push(actions);
}

fn lobby_rows(
    commands: &mut Commands,
    font: &UiFont,
    art: &GodPickArt,
    lobby: &LobbyInfo,
    rows: &mut Vec<Entity>,
) {
    let code = commands
        .spawn((
            Text::new(format!("Стол {}", spoken_code(&lobby.code))),
            font.bold(24.0),
            TextColor(INK),
        ))
        .id();
    rows.push(code);
    rows.push(text(
        commands,
        font,
        "Продиктуй друзьям этот код.",
        13.0,
        DIM,
    ));
    rows.push(mode_row(
        commands,
        font,
        lobby.mode,
        lobby.you == lobby.owner,
    ));

    for (i, person) in lobby.people.iter().enumerate() {
        let mut who = person.name.clone();
        if i == lobby.you {
            who.push_str(" (ты)");
        }
        if i == lobby.owner {
            who.push_str(" · начинает партию");
        }
        let god = person.god.map_or("бог не выбран".to_string(), |g| {
            names::god(g).to_string()
        });
        let color = person.god.map_or(DIM, god_color);
        let row = commands
            .spawn(Node {
                column_gap: px(12.0),
                ..default()
            })
            .id();
        let a = commands
            .spawn((Text::new(who), font.bold(15.0), TextColor(INK)))
            .id();
        let b = commands
            .spawn((Text::new(god), font.text(15.0), TextColor(color)))
            .id();
        commands.entity(row).add_children(&[a, b]);
        rows.push(row);
    }

    rows.push(text(
        commands,
        font,
        "Выбери бога. Щелчок по своему снимает выбор.",
        13.0,
        DIM,
    ));
    let gods = god_pick::row(commands);
    let mine = lobby.people.get(lobby.you).and_then(|p| p.god);
    for god in God::ALL {
        let taken_by = lobby
            .people
            .iter()
            .enumerate()
            .find(|(i, p)| p.god == Some(god) && *i != lobby.you);
        let (holder, action) = match taken_by {
            Some((_, p)) => (Holder::Taken(p.name.clone()), None),
            None if mine == Some(god) => (Holder::Mine, Some(FrontButton::Pick(None))),
            None => (Holder::Free, Some(FrontButton::Pick(Some(god)))),
        };
        let card = god_pick::god_card(commands, font, art, god, holder, action);
        commands.entity(gods).add_child(card);
    }
    rows.push(gods);

    let bots = 5usize.saturating_sub(lobby.people.len());
    let voice = lobby
        .oracle
        .as_deref()
        .map_or("шаблоны".to_string(), str::to_string);
    rows.push(text(
        commands,
        font,
        &format!("Свободные места займут боты: {bots}. Голос богов: {voice}."),
        12.0,
        DIM,
    ));

    let actions = commands
        .spawn(Node {
            column_gap: px(10.0),
            align_items: AlignItems::Center,
            ..default()
        })
        .id();
    if lobby.you == lobby.owner {
        let start = button(commands, font, FrontButton::Start, "Начать", true);
        commands.entity(actions).add_child(start);
    } else {
        let wait = text(commands, font, "Ждём, пока начнут партию.", 13.0, DIM);
        commands.entity(actions).add_child(wait);
    }
    let leave = button(commands, font, FrontButton::Leave, "Выйти", false);
    commands.entity(actions).add_child(leave);
    rows.push(actions);
}

fn god_color(god: God) -> Color {
    let [r, g, b] = god.accent();
    Color::srgb_u8(r, g, b)
}

/// The way back to a seat at a running match on a server, kept on disk so
/// that even a restarted game can sit back down (`NECROMY_TICKET` moves it).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ticket {
    pub server: String,
    pub name: String,
    pub code: String,
    pub ticket: u64,
}

impl Ticket {
    fn path() -> Option<std::path::PathBuf> {
        if let Some(p) = std::env::var_os("NECROMY_TICKET") {
            return Some(p.into());
        }
        Some(crate::state_dir()?.join("ticket"))
    }

    pub fn save(&self) {
        let Some(path) = Ticket::path() else {
            return;
        };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let text = format!(
            "{}\n{}\n{}\n{}\n",
            self.server, self.code, self.ticket, self.name
        );
        if let Err(err) = std::fs::write(&path, text) {
            warn!("could not keep the ticket at {}: {err}", path.display());
        }
    }

    pub fn load() -> Option<Ticket> {
        let text = std::fs::read_to_string(Ticket::path()?).ok()?;
        let mut lines = text.lines();
        Some(Ticket {
            server: lines.next()?.to_string(),
            code: lines.next()?.to_string(),
            ticket: lines.next()?.parse().ok()?,
            name: lines.next().unwrap_or("Игрок").to_string(),
        })
    }

    /// The match is over or the seat is gone: nothing to come back to.
    pub fn forget() {
        if let Some(path) = Ticket::path() {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// The world a match begins with (§21): two buttons and what each means;
/// only words for someone who may not choose.
fn mode_row(
    commands: &mut Commands,
    font: &UiFont,
    mode: necromy_rules::Mode,
    choose: bool,
) -> Entity {
    use necromy_rules::Mode;
    let column = commands
        .spawn(Node {
            flex_direction: FlexDirection::Column,
            row_gap: px(4.0),
            ..default()
        })
        .id();
    let what = match mode {
        Mode::Creation => {
            "Сотворение мира: маленький мир и одна механика; остальное игроки приносят желаниями."
        }
        Mode::Full => "Полный мир: вся доска и все механики с первого хода.",
    };
    if choose {
        let row = commands
            .spawn(Node {
                column_gap: px(10.0),
                ..default()
            })
            .id();
        let creation = button(
            commands,
            font,
            FrontButton::Mode(Mode::Creation),
            "Сотворение мира",
            mode == Mode::Creation,
        );
        let full = button(
            commands,
            font,
            FrontButton::Mode(Mode::Full),
            "Полный мир",
            mode == Mode::Full,
        );
        commands.entity(row).add_children(&[creation, full]);
        commands.entity(column).add_child(row);
    }
    let note = text(commands, font, what, 12.0, DIM);
    commands.entity(column).add_child(note);
    column
}
