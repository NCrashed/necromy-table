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
use necromy_net::{ClientConn, ClientMsg, LobbyInfo, ServerMsg, normalize_code};
use necromy_rules::{God, PlayerId};

use crate::hud::{INK, UiFont};
use crate::names;
use crate::play::Match;

const GOLD: Color = Color::srgb(1.0, 0.82, 0.3);
const DIM: Color = Color::srgb(0.72, 0.70, 0.64);
const BAD: Color = Color::srgb(0.95, 0.5, 0.4);
const GROUND: Color = Color::srgb(0.09, 0.08, 0.11);
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
}

impl Front {
    fn from_env() -> Front {
        let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
        Front {
            name: env("NECROMY_NAME")
                .or_else(|| env("USER"))
                .unwrap_or_else(|| "Игрок".into()),
            server: env("NECROMY_SERVER").unwrap_or_else(|| "127.0.0.1".into()),
            code: String::new(),
            focus: Field::Name,
            conn: None,
            lobby: None,
            error: None,
            seat: None,
            first: Vec::new(),
            start_at: env("NECROMY_START_AT").and_then(|n| n.parse().ok()),
            asked_start: false,
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
    Open,
    Sit,
    Focus(Field),
    Pick(Option<God>),
    Start,
    Leave,
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
        BackgroundColor(GROUND),
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
        Ok("local") => commands.insert_resource(Match::local()),
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
            ServerMsg::Error(e) => f.error = Some(e),
            ServerMsg::Started { seat } => f.seat = Some(seat),
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
        commands.insert_resource(Match::remote(conn, seat, first));
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
            FrontButton::Alone => commands.insert_resource(Match::local()),
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
                padding: UiRect::axes(px(12.0), px(6.0)),
                border: UiRect::all(px(2.0)),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BorderColor::all(if on { GOLD } else { GOLD.with_alpha(0.3) }),
            BackgroundColor(if on {
                Color::srgba(0.35, 0.26, 0.1, 0.95)
            } else {
                Color::srgba(0.15, 0.12, 0.15, 0.9)
            }),
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
                padding: UiRect::axes(px(8.0), px(5.0)),
                border: UiRect::all(px(2.0)),
                ..default()
            },
            BorderColor::all(if focused { GOLD } else { GOLD.with_alpha(0.3) }),
            BackgroundColor(Color::srgba(0.04, 0.03, 0.06, 1.0)),
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
    root: Single<Entity, With<FrontRoot>>,
) {
    let root = *root;
    commands.entity(root).despawn_related::<Children>();
    let frame = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(12.0),
                padding: UiRect::all(px(24.0)),
                border: UiRect::all(px(2.0)),
                width: px(560.0),
                ..default()
            },
            BorderColor::all(GOLD),
            BackgroundColor(Color::srgba(0.13, 0.11, 0.15, 1.0)),
        ))
        .id();
    let mut rows = Vec::new();
    let title = commands
        .spawn((Text::new("Necromy Table"), font.bold(30.0), TextColor(GOLD)))
        .id();
    rows.push(title);

    match &front.lobby {
        None => menu(&mut commands, &font, &front, &mut rows),
        Some(lobby) => lobby_rows(&mut commands, &font, lobby, &mut rows),
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
        "Сервер даст код стола: назови его друзьям.",
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
        "Tab — следующее поле. Сервер: scripts/table-server.sh или адрес друга.",
        11.0,
        DIM,
    ));
}

fn lobby_rows(commands: &mut Commands, font: &UiFont, lobby: &LobbyInfo, rows: &mut Vec<Entity>) {
    let code = commands
        .spawn((
            Text::new(format!("Стол {}", lobby.code)),
            font.bold(24.0),
            TextColor(INK),
        ))
        .id();
    rows.push(code);
    rows.push(text(commands, font, "Назови друзьям этот код.", 13.0, DIM));

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

    rows.push(text(commands, font, "Выбери бога:", 13.0, DIM));
    let gods = commands
        .spawn(Node {
            flex_wrap: FlexWrap::Wrap,
            column_gap: px(6.0),
            row_gap: px(6.0),
            ..default()
        })
        .id();
    let mine = lobby.people.get(lobby.you).and_then(|p| p.god);
    for god in God::ALL {
        let taken_by = lobby
            .people
            .iter()
            .enumerate()
            .find(|(i, p)| p.god == Some(god) && *i != lobby.you);
        let label = match taken_by {
            Some((_, p)) => format!("{} · {}", names::god(god), p.name),
            None => names::god(god).to_string(),
        };
        let action = if mine == Some(god) {
            FrontButton::Pick(None)
        } else {
            FrontButton::Pick(Some(god))
        };
        let b = button(commands, font, action, &label, mine == Some(god));
        let swatch = commands
            .spawn((
                Node {
                    width: px(10.0),
                    height: px(10.0),
                    margin: UiRect::right(px(6.0)),
                    ..default()
                },
                BackgroundColor(god_color(god)),
            ))
            .id();
        commands.entity(b).insert_children(0, &[swatch]);
        commands.entity(gods).add_child(b);
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
