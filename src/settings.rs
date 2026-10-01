//! The game's settings: music and effects volume, windowed or fullscreen.
//! Kept in `$XDG_CONFIG_HOME/necromy-table/settings` (one `key=value` a
//! line), applied at start and whenever they change. The window opens
//! maximized, windowed; fullscreen is borderless on the current monitor.
//!
//! The panel opens from the menu («Настройки»), in a match from the gear
//! beside the status panel or F10; Esc or «Закрыть» closes it.

use bevy::prelude::*;
use bevy::window::{MonitorSelection, PrimaryWindow, WindowMode};

use crate::hud::{INK, UiFont};
use crate::stats;
use crate::ui_skin::{Accent, Frame};

const GOLD: Color = Color::srgb(0.95, 0.78, 0.35);
const DIM: Color = Color::srgb(0.72, 0.70, 0.64);
/// Volume steps: tenths.
const STEPS: u8 = 10;

pub struct SettingsPlugin;

impl Plugin for SettingsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Settings::load())
            // Dev aid: `NECROMY_SETTINGS=1` opens the panel at start.
            .insert_resource(SettingsOpen(std::env::var_os("NECROMY_SETTINGS").is_some()))
            .add_systems(Startup, (spawn, first_window))
            .add_systems(
                Update,
                (
                    keys,
                    buttons,
                    gear_shown,
                    apply_window.run_if(resource_changed::<Settings>),
                    save.run_if(resource_changed::<Settings>),
                    rebuild.run_if(
                        resource_changed::<Settings>.or_else(resource_changed::<SettingsOpen>),
                    ),
                )
                    .chain(),
            );
    }
}

#[derive(Resource, Clone, Debug, PartialEq)]
pub struct Settings {
    /// Music volume, 0..=STEPS tenths.
    pub music: u8,
    /// Sound effects and blips, 0..=STEPS tenths.
    pub effects: u8,
    pub fullscreen: bool,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            music: 8,
            effects: 8,
            fullscreen: false,
        }
    }
}

impl Settings {
    /// Music volume as a factor, 0..=1.
    pub fn music_gain(&self) -> f32 {
        f32::from(self.music) / f32::from(STEPS)
    }

    pub fn effects_gain(&self) -> f32 {
        f32::from(self.effects) / f32::from(STEPS)
    }

    fn path() -> Option<std::path::PathBuf> {
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .map(std::path::PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|h| std::path::Path::new(&h).join(".config"))
            })?;
        Some(config.join("necromy-table").join("settings"))
    }

    fn load() -> Settings {
        Settings::path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map_or_else(Settings::default, |text| Settings::parse(&text))
    }

    /// Settings from the file's text; unknown keys and bad values are skipped.
    fn parse(text: &str) -> Settings {
        let mut s = Settings::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim();
            match key.trim() {
                "music" => s.music = value.parse::<u8>().unwrap_or(s.music).min(STEPS),
                "effects" => s.effects = value.parse::<u8>().unwrap_or(s.effects).min(STEPS),
                "fullscreen" => s.fullscreen = value == "true",
                _ => {}
            }
        }
        s
    }

    fn text(&self) -> String {
        format!(
            "music={}\neffects={}\nfullscreen={}\n",
            self.music, self.effects, self.fullscreen
        )
    }
}

/// Whether the settings panel is up.
#[derive(Resource, Default, PartialEq)]
pub struct SettingsOpen(pub bool);

#[derive(Component)]
struct SettingsPanel;

/// The gear that opens the panel during a match.
#[derive(Component)]
struct Gear;

#[derive(Component, Clone, Copy)]
enum SettingsButton {
    Open,
    Music(i8),
    Effects(i8),
    Fullscreen(bool),
    /// Open the sound again on the present output (headphones plugged in).
    Reconnect,
    Close,
}

fn spawn(mut commands: Commands, font: Res<UiFont>) {
    commands.spawn((
        SettingsPanel,
        Node {
            position_type: PositionType::Absolute,
            width: percent(100.0),
            height: percent(100.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        // Over everything: the menu (50), the battle, the end of a match.
        GlobalZIndex(60),
        Visibility::Hidden,
    ));
    let gear = stats::label(&mut commands, &font, "⚙", 18.0, true);
    commands
        .spawn((
            Gear,
            SettingsButton::Open,
            Button,
            Frame::Button,
            Node {
                position_type: PositionType::Absolute,
                // In the gap right of the status panel, left of the action bar.
                left: px(344.0),
                top: px(14.0),
                padding: UiRect::axes(px(9.0), px(4.0)),
                ..default()
            },
            GlobalZIndex(9),
            Visibility::Hidden,
        ))
        .add_child(gear);
}

/// The window as the settings want it at start: maximized, or fullscreen.
/// Screenshot runs keep the default size, so their frames stay comparable.
fn first_window(settings: Res<Settings>, mut window: Single<&mut Window, With<PrimaryWindow>>) {
    set_window(&settings, &mut window);
}

fn apply_window(
    settings: Res<Settings>,
    mut window: Single<&mut Window, With<PrimaryWindow>>,
    mut first: Local<bool>,
) {
    // The first run is the start, which `first_window` handled.
    if !*first {
        *first = true;
        return;
    }
    set_window(&settings, &mut window);
}

fn set_window(settings: &Settings, window: &mut Window) {
    if settings.fullscreen {
        window.mode = WindowMode::BorderlessFullscreen(MonitorSelection::Current);
    } else {
        window.mode = WindowMode::Windowed;
        window.set_maximized(true);
    }
}

fn save(settings: Res<Settings>, mut first: Local<bool>) {
    // Nothing changed yet at start: loaded, not chosen.
    if !*first {
        *first = true;
        return;
    }
    let Some(path) = Settings::path() else {
        return;
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Err(e) = std::fs::write(&path, settings.text()) {
        warn!("settings not saved to {}: {e}", path.display());
    }
}

fn keys(keyboard: Res<ButtonInput<KeyCode>>, mut open: ResMut<SettingsOpen>) {
    if keyboard.just_pressed(KeyCode::F10) {
        open.0 = !open.0;
    } else if open.0 && keyboard.just_pressed(KeyCode::Escape) {
        open.0 = false;
    }
}

fn buttons(
    pressed: Query<(&Interaction, &SettingsButton), Changed<Interaction>>,
    mut settings: ResMut<Settings>,
    mut open: ResMut<SettingsOpen>,
    mut reopen: MessageWriter<bevy::audio::ReopenAudioOutput>,
) {
    let step = |v: u8, d: i8| (v as i8 + d).clamp(0, STEPS as i8) as u8;
    for (interaction, button) in &pressed {
        if *interaction != Interaction::Pressed {
            continue;
        }
        match *button {
            SettingsButton::Open => open.0 = true,
            SettingsButton::Close => open.0 = false,
            SettingsButton::Music(d) => settings.music = step(settings.music, d),
            SettingsButton::Effects(d) => settings.effects = step(settings.effects, d),
            SettingsButton::Fullscreen(on) => settings.fullscreen = on,
            SettingsButton::Reconnect => {
                reopen.write(bevy::audio::ReopenAudioOutput);
            }
        }
    }
}

/// The gear stands during a match, while the panel is closed.
fn gear_shown(
    game: Option<Res<crate::play::Match>>,
    open: Res<SettingsOpen>,
    mut gear: Single<&mut Visibility, With<Gear>>,
) {
    let shown = if game.is_some() && !open.0 {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    gear.set_if_neq(shown);
}

/// Opens the settings from elsewhere (the menu's button).
pub fn open_button(commands: &mut Commands, font: &UiFont, label: &str) -> Entity {
    button(commands, font, SettingsButton::Open, label, true)
}

fn button(
    commands: &mut Commands,
    font: &UiFont,
    what: SettingsButton,
    label: &str,
    lit: bool,
) -> Entity {
    let text = stats::label(commands, font, label, 14.0, true);
    commands
        .spawn((
            what,
            Button,
            Frame::Button,
            Accent(if lit {
                GOLD
            } else {
                Color::srgb(0.55, 0.45, 0.3)
            }),
            Node {
                padding: UiRect::axes(px(12.0), px(6.0)),
                justify_content: JustifyContent::Center,
                ..default()
            },
        ))
        .add_child(text)
        .id()
}

fn rebuild(
    mut commands: Commands,
    settings: Res<Settings>,
    open: Res<SettingsOpen>,
    font: Res<UiFont>,
    panel: Single<(Entity, &mut Visibility), With<SettingsPanel>>,
) {
    let (panel, mut visibility) = panel.into_inner();
    commands.entity(panel).despawn_related::<Children>();
    if !open.0 {
        visibility.set_if_neq(Visibility::Hidden);
        return;
    }
    visibility.set_if_neq(Visibility::Inherited);
    let frame = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(14.0),
                padding: UiRect::all(px(24.0)),
                width: px(440.0),
                ..default()
            },
            Frame::Plate,
            Accent(GOLD),
        ))
        .id();
    let title = stats::label(&mut commands, &font, "Настройки", 22.0, true);
    commands.entity(frame).add_child(title);

    for (name, value, less, more) in [
        (
            "Музыка",
            settings.music,
            SettingsButton::Music(-1),
            SettingsButton::Music(1),
        ),
        (
            "Эффекты",
            settings.effects,
            SettingsButton::Effects(-1),
            SettingsButton::Effects(1),
        ),
    ] {
        let row = row(&mut commands);
        let label = fixed_label(&mut commands, &font, name, 100.0);
        let minus = button(&mut commands, &font, less, "−", value > 0);
        let bar = stats::bar(&mut commands, value, STEPS, GOLD, None, 12.0, 14.0);
        let plus = button(&mut commands, &font, more, "+", value < STEPS);
        let percent = fixed_label(&mut commands, &font, &format!("{}%", value * 10), 44.0);
        commands
            .entity(row)
            .add_children(&[label, minus, bar, plus, percent]);
        commands.entity(frame).add_child(row);
    }

    let row = row(&mut commands);
    let label = fixed_label(&mut commands, &font, "Экран", 100.0);
    let windowed = button(
        &mut commands,
        &font,
        SettingsButton::Fullscreen(false),
        "Окно",
        !settings.fullscreen,
    );
    let full = button(
        &mut commands,
        &font,
        SettingsButton::Fullscreen(true),
        "Весь экран",
        settings.fullscreen,
    );
    commands.entity(row).add_children(&[label, windowed, full]);
    commands.entity(frame).add_child(row);

    // The sound follows a new output by itself; this is for when it does not.
    let output = self::row(&mut commands);
    let label = fixed_label(&mut commands, &font, "Вывод", 100.0);
    let reconnect = button(
        &mut commands,
        &font,
        SettingsButton::Reconnect,
        "Переподключить звук",
        false,
    );
    commands.entity(output).add_children(&[label, reconnect]);
    commands.entity(frame).add_child(output);

    let hint = commands
        .spawn((
            Text::new("F10 — открыть и закрыть, Esc — закрыть. Сохраняется само."),
            font.text(11.0),
            TextColor(DIM),
        ))
        .id();
    let close = button(&mut commands, &font, SettingsButton::Close, "Закрыть", true);
    commands.entity(close).insert(Node {
        padding: UiRect::axes(px(16.0), px(8.0)),
        align_self: AlignSelf::FlexEnd,
        ..default()
    });
    commands.entity(frame).add_children(&[hint, close]);
    commands.entity(panel).add_child(frame);
}

fn row(commands: &mut Commands) -> Entity {
    commands
        .spawn(Node {
            align_items: AlignItems::Center,
            column_gap: px(8.0),
            ..default()
        })
        .id()
}

fn fixed_label(commands: &mut Commands, font: &UiFont, text: &str, width: f32) -> Entity {
    commands
        .spawn((
            Text::new(text.to_string()),
            font.text(14.0),
            TextColor(INK),
            TextLayout::no_wrap(),
            Node {
                width: px(width),
                ..default()
            },
        ))
        .id()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_come_back_as_saved_and_bad_lines_are_skipped() {
        let s = Settings {
            music: 3,
            effects: 10,
            fullscreen: true,
        };
        assert_eq!(Settings::parse(&s.text()), s);
        let odd = Settings::parse("music=99\neffects=x\nnonsense\nfullscreen=false\n");
        assert_eq!(odd.music, STEPS, "clamped");
        assert_eq!(odd.effects, Settings::default().effects, "kept");
        assert!(!odd.fullscreen);
    }
}
