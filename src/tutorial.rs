//! The tutorial: chapters, each a small scripted scene (`Game::scenario`)
//! that teaches one thing step by step. A step says what to do, lights the
//! hex or the card it is about (`Focus`), lets the human do only that
//! (`Match::gate`) and waits for the event that shows it done; or it only
//! explains and waits for «Далее». Rivals are dummies (`Seat::Dummy`) that
//! stand still and pass; a step may make one play a card as it begins.
//!
//! The menu lists the chapters (`chapters`); a finished chapter is marked
//! in `$XDG_STATE_HOME/necromy-table/tutorial`. `NECROMY_PLAY=tutorial`
//! opens the list, `tutorial:N` chapter N; with `NECROMY_AUTOPLAY` the
//! steps play themselves (each step's `demo`), for screenshots.

use std::sync::Arc;

use bevy::prelude::*;
use necromy_rules::{
    Event, Game, God, Hex, Intent, PlayerId, Scenario, SceneSeat, Target, Terrain,
};

use crate::hud::{INK, UiFont};
use crate::play::{Gate, Match};
use crate::stats;
use crate::ui_skin::{Accent, Frame};

const GOLD: Color = Color::srgb(0.95, 0.78, 0.35);
const VIOLET: Color = Color::srgb(0.75, 0.55, 1.0);
const BAD: Color = Color::srgb(1.0, 0.55, 0.45);
/// Seconds the "not now" hint stays after a refused action.
const HINT_SECS: f32 = 4.0;
/// Autoplay: seconds between a step's moves, and before «Далее».
const DEMO_SECS: f32 = 1.2;
const READ_SECS: f32 = 2.5;

pub struct TutorialPlugin;

impl Plugin for TutorialPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Focus>()
            .add_systems(Startup, spawn)
            .add_systems(
                crate::InGame,
                (run, demo, buttons, rebuild, place)
                    .chain()
                    .run_if(resource_exists::<Lesson>),
            );
    }
}

/// What the current step points at: hexes lit and marked on the board, a
/// card outlined in hand.
#[derive(Resource, Default, PartialEq)]
pub struct Focus {
    pub hexes: Vec<Hex>,
    pub card: Option<&'static str>,
}

type EventTest = Arc<dyn Fn(&Event, &Game) -> bool + Send + Sync>;
type Demo = Arc<dyn Fn(&Game, PlayerId) -> Option<Intent> + Send + Sync>;

/// When a step is done.
#[derive(Clone)]
enum Until {
    /// The human reads and presses «Далее».
    Next,
    /// An event the table tells; with `settle`, also the battle on screen
    /// is over.
    Event { test: EventTest, settle: bool },
}

/// A rival's scripted move as a step begins: play `card` at the human.
#[derive(Clone)]
struct Rival {
    seat: PlayerId,
    card: &'static str,
}

#[derive(Clone)]
pub struct Step {
    text: &'static str,
    until: Until,
    gate: Gate,
    /// Said when the human tries something else.
    refuse: &'static str,
    focus_hexes: Vec<Hex>,
    focus_card: Option<&'static str>,
    rival: Option<Rival>,
    /// What the step asks for, as an intent: autoplay and tests make it.
    demo: Option<Demo>,
}

impl Step {
    /// Only words: «Далее» goes on. Windows may still be passed.
    fn read(text: &'static str) -> Step {
        Step {
            text,
            until: Until::Next,
            gate: Arc::new(|_, _, i| *i == Intent::Pass),
            refuse: "Сначала дочитай и нажми «Далее».",
            focus_hexes: Vec::new(),
            focus_card: None,
            rival: None,
            demo: None,
        }
    }

    fn act(
        text: &'static str,
        until: impl Fn(&Event, &Game) -> bool + Send + Sync + 'static,
        gate: impl Fn(&Game, PlayerId, &Intent) -> bool + Send + Sync + 'static,
        refuse: &'static str,
        demo: impl Fn(&Game, PlayerId) -> Option<Intent> + Send + Sync + 'static,
    ) -> Step {
        Step {
            text,
            until: Until::Event {
                test: Arc::new(until),
                settle: false,
            },
            gate: Arc::new(gate),
            refuse,
            focus_hexes: Vec::new(),
            focus_card: None,
            rival: None,
            demo: Some(Arc::new(demo)),
        }
    }

    fn hexes(mut self, hexes: &[Hex]) -> Step {
        self.focus_hexes = hexes.to_vec();
        self
    }

    fn card(mut self, name: &'static str) -> Step {
        self.focus_card = Some(name);
        self
    }

    fn rival(mut self, seat: u8, card: &'static str) -> Step {
        self.rival = Some(Rival {
            seat: PlayerId(seat),
            card,
        });
        self
    }

    /// Wait also for the battle on screen to end.
    fn settle(mut self) -> Step {
        if let Until::Event { settle, .. } = &mut self.until {
            *settle = true;
        }
        self
    }
}

pub struct Chapter {
    pub title: &'static str,
    pub blurb: &'static str,
    pub scene: fn() -> Scenario,
    pub steps: fn() -> Vec<Step>,
}

pub fn chapters() -> [Chapter; 4] {
    [
        Chapter {
            title: "Шаги по столу",
            blurb: "Очки движения, цена местности, конец хода.",
            scene: walk_scene,
            steps: walk_steps,
        },
        Chapter {
            title: "Карты и Дух",
            blurb: "Цена карты, свой ход и мгновенные карты.",
            scene: cards_scene,
            steps: cards_steps,
        },
        Chapter {
            title: "Кольцо пяти",
            blurb: "Кто кого гасит: ответы и обереги.",
            scene: ring_scene,
            steps: ring_steps,
        },
        Chapter {
            title: "Бой",
            blurb: "Нападение, сожжённые карты, кубики.",
            scene: fight_scene,
            steps: fight_steps,
        },
    ]
}

// ---- Gates, tests and demos the chapters share ----

const ME: PlayerId = PlayerId(0);
const RIVAL: PlayerId = PlayerId(1);

fn is_move_to(hex: Hex) -> impl Fn(&Game, PlayerId, &Intent) -> bool {
    move |_, _, i| matches!(i, Intent::Move { to } if *to == hex) || *i == Intent::Pass
}

fn is_play(name: &'static str) -> impl Fn(&Game, PlayerId, &Intent) -> bool {
    move |g, _, i| matches!(i, Intent::Play { card, .. } if g.def(*card).name == name)
}

fn card_in_hand(g: &Game, p: PlayerId, name: &str) -> Option<necromy_rules::CardId> {
    g.hand(p).iter().copied().find(|&c| g.def(c).name == name)
}

/// Play `name` at its first legal target, a rival's champion first.
fn play_demo(name: &'static str) -> impl Fn(&Game, PlayerId) -> Option<Intent> {
    move |g, p| {
        let card = card_in_hand(g, p, name)?;
        let targets = g.targets(p, card);
        let target = targets
            .iter()
            .copied()
            .find(|t| matches!(t, Target::Champion(q) if *q != p))
            .or_else(|| targets.first().copied())
            .unwrap_or(Target::None);
        Some(Intent::Play { card, target })
    }
}

fn played(name: &'static str) -> impl Fn(&Event, &Game) -> bool {
    move |e, _| matches!(e, Event::CardPlayed { player, def, .. } if *player == ME && def.def().name == name)
}

/// A step towards `goal` over the cheapest ground, for the demo.
fn step_towards(g: &Game, p: PlayerId, goal: Hex) -> Option<Intent> {
    let at = g.champion(p)?.hex;
    at.all_neighbors()
        .into_iter()
        .filter(|h| g.board().contains(*h))
        .min_by_key(|h| {
            let cost = g.board().tile(*h).map_or(9, |t| t.terrain.move_cost());
            (h.unsigned_distance_to(goal), cost)
        })
        .map(|to| Intent::Move { to })
}

fn seat(god: God, x: i32, y: i32) -> SceneSeat {
    SceneSeat::new(god, Hex::new(x, y))
}

// ---- Chapter 1: walking ----

const WALK_A: Hex = Hex::new(0, 2);
const WALK_FOREST: Hex = Hex::new(0, 1);
const WALK_GOAL: Hex = Hex::new(1, -2);

fn walk_scene() -> Scenario {
    let mut s = Scenario::new(3, vec![seat(God::Trishna, 0, 3), seat(God::Zaga, -3, 0)]);
    s.terrain = vec![
        (WALK_FOREST, Terrain::Forest),
        (Hex::new(1, 1), Terrain::Forest),
        (Hex::new(-1, 2), Terrain::Mountain),
        (Hex::new(-1, 1), Terrain::Forest),
        (Hex::new(2, -1), Terrain::Settlement),
        (Hex::new(-2, -1), Terrain::Mountain),
    ];
    s
}

fn walk_steps() -> Vec<Step> {
    vec![
        Step::read(
            "Это твой чемпион — Тришна. Каждый ход даёт 3 очка движения: они видны \
             сверху, рядом с «Твой ход». Колесо мыши приближает стол, правая кнопка \
             двигает его, Q и E поворачивают.",
        ),
        Step::act(
            "Шаг по равнине стоит 1 очко. Кликни по отмеченной клетке.",
            |e, _| matches!(e, Event::Moved { player, to, .. } if *player == ME && *to == WALK_A),
            is_move_to(WALK_A),
            "Шагни на отмеченную клетку.",
            |_, _| Some(Intent::Move { to: WALK_A }),
        )
        .hexes(&[WALK_A]),
        Step::act(
            "Лес, горы и рощи стоят 2 очка. Шагни в лес.",
            |e, _| {
                matches!(e, Event::Moved { player, to, .. } if *player == ME && *to == WALK_FOREST)
            },
            is_move_to(WALK_FOREST),
            "Шагни в отмеченный лес.",
            |_, _| Some(Intent::Move { to: WALK_FOREST }),
        )
        .hexes(&[WALK_FOREST]),
        Step::read(
            "Очки кончились. Все чемпионы ходят одновременно, а раунд идёт дальше, \
             когда каждый закончил свой ход.",
        ),
        Step::act(
            "Нажми «Конец хода» наверху или пробел.",
            |e, _| matches!(e, Event::RoundStarted { round: 2, .. }),
            |_, _, i| matches!(i, Intent::EndTurn | Intent::Pass),
            "Сейчас закончи ход: кнопка наверху или пробел.",
            |_, _| Some(Intent::EndTurn),
        ),
        Step::act(
            "Новый раунд — снова 3 очка. Дойди до отмеченной клетки: кликни по ней, \
             путь проложится сам.",
            |e, _| {
                matches!(e, Event::Moved { player, to, .. } if *player == ME && *to == WALK_GOAL)
            },
            |_, _, i| matches!(i, Intent::Move { .. } | Intent::Pass),
            "Иди к отмеченной клетке.",
            |g, p| step_towards(g, p, WALK_GOAL),
        )
        .hexes(&[WALK_GOAL]),
        Step::read("Готово: ты умеешь ходить. Дальше — карты и Дух."),
    ]
}

// ---- Chapter 2: cards and Spirit ----

fn cards_scene() -> Scenario {
    let mut s = Scenario::new(
        3,
        vec![
            seat(God::Trishna, 0, 2)
                .hand(&["Жар в крови", "Искра", "Бинт"])
                .hp(2),
            seat(God::Zaga, 1, 0),
        ],
    );
    s.terrain = vec![
        (Hex::new(-1, 1), Terrain::Forest),
        (Hex::new(2, -2), Terrain::Mountain),
        (Hex::new(-2, 3), Terrain::Settlement),
    ];
    s
}

fn cards_steps() -> Vec<Step> {
    vec![
        Step::read(
            "Внизу — твоя рука. Наведи на карту, и она поднимется. Число в кристалле — \
             цена в Духе. Твой Дух — синие ячейки на листе слева внизу; в начале \
             каждого хода он растёт на 1.",
        ),
        Step::act(
            "Карты «свой ход» играют только в свой ход. Сыграй «Жар в крови»: \
             кликни по ней.",
            played("Жар в крови"),
            is_play("Жар в крови"),
            "Сыграй «Жар в крови».",
            play_demo("Жар в крови"),
        )
        .card("Жар в крови"),
        Step::read("+2 очка движения на этот ход, Духа стало на 1 меньше."),
        Step::act(
            "«Искра» — мгновенная карта: её можно играть и в чужой ход, в ответ. \
             Сейчас брось её в Загу: кликни по карте, потом по Заге.",
            |e, _| matches!(e, Event::Damaged { player, .. } if *player == RIVAL),
            is_play("Искра"),
            "Сыграй «Искру» в Загу.",
            play_demo("Искра"),
        )
        .card("Искра")
        .hexes(&[Hex::new(1, 0)]),
        Step::act(
            "Зага ранена. Ты тоже: у тебя 2 здоровья из 3. «Бинт» лечит на 1 — \
             сыграй его.",
            |e, _| matches!(e, Event::Healed { player, .. } if *player == ME),
            is_play("Бинт"),
            "Сыграй «Бинт».",
            play_demo("Бинт"),
        )
        .card("Бинт"),
        Step::read(
            "Сыгранные карты уходят в сброс, а в начале хода рука добирается из \
             колоды. Дальше — кольцо пяти стихий.",
        ),
    ]
}

// ---- Chapter 3: the ring of five ----

fn ring_scene() -> Scenario {
    let mut s = Scenario::new(
        3,
        vec![
            seat(God::Trishna, 0, 2).hand(&["Морок", "Тишь", "Власяница"]),
            seat(God::Zaga, 1, 0).hand(&["Бремя", "Шипы чащи"]),
        ],
    );
    s.terrain = vec![
        (Hex::new(-1, 1), Terrain::Forest),
        (Hex::new(2, -2), Terrain::Mountain),
        (Hex::new(-2, 3), Terrain::Settlement),
    ];
    s
}

/// End the turn if it is still running, else pass the window: a rival's
/// card on the human waits for their turn to end (§11.2).
fn end_then_pass(g: &Game, p: PlayerId) -> Option<Intent> {
    Some(if g.free_to_act(p) {
        Intent::EndTurn
    } else {
        Intent::Pass
    })
}

fn aimed_at_me(e: &Event, _: &Game) -> bool {
    matches!(e, Event::WindowOpened { kind: necromy_rules::WindowKind::Target { target, .. }, .. } if *target == ME)
}

fn ring_steps() -> Vec<Step> {
    vec![
        Step::read(
            "Пять стихий идут по кругу: дерево → огонь → земля → металл → вода → \
             дерево. Каждая гасит ту, что через одну от неё: вода — огонь, огонь — \
             металл, металл — дерево, дерево — землю, земля — воду. Схема — под \
             «кольцом пяти» справа вверху.",
        ),
        Step::act(
            "Зага играет в тебя «Бремя» (земля): ты потеряешь следующий ход. Пока ты \
             ходишь, чужие карты тебя не трогают — они ждут конца твоего хода. \
             Закончи ход (пробел), и ты сможешь ответить.",
            aimed_at_me,
            |_, _, i| matches!(i, Intent::EndTurn | Intent::Pass),
            "Закончи ход: пробел или кнопка наверху.",
            |_, _| Some(Intent::EndTurn),
        )
        .rival(1, "Бремя"),
        Step::act(
            "Окно ответа открыто. Смотри на значки карт: «Морок» (вода) не погасит — \
             земля гасит воду. Ответь «Тишью» (земля).",
            |e, _| matches!(e, Event::Canceled { .. }),
            is_play("Тишь"),
            "Ответь «Тишью»: «Морок» тут не поможет.",
            play_demo("Тишь"),
        )
        .card("Тишь"),
        Step::read(
            "«Бремя» погасло: ответ гасит карту, если её стихия не гасит стихию ответа. \
             Начался новый раунд.",
        ),
        Step::act(
            "Оберег держится до твоего следующего хода и останавливает вредные карты, \
             кроме стихии, что его гасит. Надень «Власяницу» — оберег земли.",
            |e, _| matches!(e, Event::WardRaised { player, .. } if *player == ME),
            is_play("Власяница"),
            "Надень «Власяницу».",
            play_demo("Власяница"),
        )
        .card("Власяница"),
        Step::act(
            "Зага целит в тебя «Шипы чащи» — это дерево, а дерево гасит землю: оберег \
             не удержит. «Морок» здесь погасил бы (дерево не гасит воду), но сейчас \
             закончи ход, а в окне ответа пропусти (пробел) — и посмотри.",
            |e, _| {
                matches!(e, Event::WardBroken { player, .. } | Event::Damaged { player, .. } if *player == ME)
            },
            |_, _, i| matches!(i, Intent::EndTurn | Intent::Pass),
            "Закончи ход, потом пропусти: пробел.",
            end_then_pass,
        )
        .rival(1, "Шипы чащи"),
        Step::read(
            "Ответ гасит карту, оберег её останавливает — если стихия карты не гасит \
             их стихию. Значки на картах («погасит», «не погасит») подскажут. Дальше — бой.",
        ),
    ]
}

// ---- Chapter 4: battle ----

const FIGHT_FOE: Hex = Hex::new(0, 1);

fn fight_scene() -> Scenario {
    let mut s = Scenario::new(
        3,
        vec![
            seat(God::Trishna, 0, 2).hand(&["Искра", "Сжечь как топливо"]),
            seat(God::Zaga, 0, 1).hp(2),
        ],
    );
    s.terrain = vec![
        (Hex::new(-1, 1), Terrain::Forest),
        (Hex::new(2, -2), Terrain::Mountain),
        (Hex::new(-2, 3), Terrain::Settlement),
    ];
    s
}

fn fight_steps() -> Vec<Step> {
    vec![
        Step::read(
            "Чтобы напасть, шагни на соперника. Бой решают кубики: у каждого их \
             столько, сколько Силы (меч на листе).",
        ),
        Step::act(
            "Нападай: кликни по Заге.",
            |e, _| matches!(e, Event::BattleStarted { attacker, .. } if *attacker == ME),
            is_move_to(FIGHT_FOE),
            "Нападай на Загу: кликни по ней.",
            |_, _| Some(Intent::Move { to: FIGHT_FOE }),
        )
        .hexes(&[FIGHT_FOE]),
        Step::act(
            "Перед броском каждый может сжечь карты: сожжённая карта — верная грань. \
             Приём даёт удар, карта тела — щит. Отметь «Искру» и брось кубики.",
            |e, _| matches!(e, Event::BattleResolved { .. }),
            |g, _, i| {
                matches!(i, Intent::Burn { cards } if cards.iter().any(|c| g.def(*c).name == "Искра"))
            },
            "Отметь «Искру», потом брось кубики.",
            |g, p| {
                card_in_hand(g, p, "Искра").map(|c| Intent::Burn { cards: vec![c] })
            },
        )
        .card("Искра")
        .settle(),
        Step::read(
            "Удары одной стороны бьются о щиты другой, лишние удары ранят. Победа в бою приносит Стиль, а павший оставляет тело и просыпается \
             дома. Обучение пройдено — можно садиться за стол!",
        ),
    ]
}

// ---- Running a chapter ----

#[derive(Resource)]
pub struct Lesson {
    pub chapter: usize,
    steps: Vec<Step>,
    step: usize,
    entered: bool,
    /// The step's event came; it waits for the battle to end.
    met: bool,
    /// `Match::refused` already answered with the hint, and when.
    refused_seen: u32,
    hint_until: f32,
    since: f32,
    last_demo: f32,
    autoplay: bool,
    finished: bool,
}

impl Lesson {
    pub fn new(chapter: usize) -> Lesson {
        Lesson {
            chapter,
            steps: (chapters()[chapter].steps)(),
            step: 0,
            entered: false,
            met: false,
            refused_seen: 0,
            hint_until: 0.0,
            since: 0.0,
            last_demo: 0.0,
            autoplay: std::env::var_os("NECROMY_AUTOPLAY").is_some(),
            finished: false,
        }
    }

    fn next(&mut self) {
        if self.step + 1 >= self.steps.len() {
            self.finished = true;
            mark_done(self.chapter);
        } else {
            self.step += 1;
        }
        self.entered = false;
        self.met = false;
    }
}

/// The match and lesson of chapter `i`.
pub fn start(chapter: usize) -> (Match, Lesson) {
    let scene = (chapters()[chapter].scene)();
    (Match::tutorial(&scene), Lesson::new(chapter))
}

fn run(
    time: Res<Time>,
    mut lesson: ResMut<Lesson>,
    mut game: ResMut<Match>,
    mut focus: ResMut<Focus>,
) {
    let now = time.elapsed_secs();
    let l = lesson.bypass_change_detection();
    if l.finished {
        game.gate = Some(Arc::new(|_, _, i| *i == Intent::Pass));
        focus.set_if_neq(Focus::default());
        return;
    }
    let step = l.steps[l.step].clone();
    if !l.entered {
        l.entered = true;
        l.since = now;
        game.gate = Some(step.gate.clone());
        focus.set_if_neq(Focus {
            hexes: step.focus_hexes.clone(),
            card: step.focus_card,
        });
        if let Some(events) = game.bypass_change_detection().lesson_events.as_mut() {
            events.clear();
        }
        if let Some(r) = &step.rival {
            game.script_play(r.seat, r.card, Target::Champion(ME));
        }
        lesson.set_changed();
        return;
    }
    if game.refused != l.refused_seen {
        l.refused_seen = game.refused;
        l.hint_until = now + HINT_SECS;
        lesson.set_changed();
        return;
    }
    let l = lesson.bypass_change_detection();
    if let Until::Event { test, settle } = &step.until {
        let events = game
            .bypass_change_detection()
            .lesson_events
            .as_mut()
            .map(std::mem::take)
            .unwrap_or_default();
        if !l.met && events.iter().any(|e| test(e, &game.game)) {
            l.met = true;
        }
        if l.met && (!settle || game.battle.is_none()) {
            l.next();
            lesson.set_changed();
        }
    }
}

/// Autoplay: the step's demo, and «Далее» after a moment to read.
fn demo(time: Res<Time>, mut lesson: ResMut<Lesson>, mut game: ResMut<Match>) {
    let now = time.elapsed_secs();
    let l = lesson.bypass_change_detection();
    if !l.autoplay || l.finished || !l.entered {
        return;
    }
    let step = l.steps[l.step].clone();
    match &step.until {
        Until::Next if now - l.since > READ_SECS => {
            l.next();
            lesson.set_changed();
        }
        Until::Event { .. } if now - l.last_demo > DEMO_SECS && !l.met => {
            l.last_demo = now;
            let human = game.human;
            if let Some(intent) = step.demo.as_ref().and_then(|d| d(&game.game, human)) {
                let _ = game.act(human, intent);
            }
        }
        _ => {}
    }
}

// ---- The panel ----

#[derive(Component)]
struct LessonPanel;

#[derive(Component, Clone, Copy)]
enum LessonButton {
    Next,
    NextChapter,
    Chapters,
}

fn spawn(mut commands: Commands) {
    commands.spawn((
        LessonPanel,
        Node {
            position_type: PositionType::Absolute,
            // In the gap between the status panel (left) and the gods (right).
            top: px(96.0),
            left: px(388.0),
            ..default()
        },
        // Over the battle panel: the step may be about the battle.
        GlobalZIndex(20),
        Visibility::Hidden,
    ));
}

/// Over the battle, the panel moves up to the screen's top edge: the fight
/// plays in the middle.
fn place(game: Res<Match>, mut panel: Single<&mut Node, With<LessonPanel>>) {
    let top = if game.battle.is_some() { 8.0 } else { 96.0 };
    if panel.top != px(top) {
        panel.top = px(top);
    }
}

fn rebuild(
    mut commands: Commands,
    time: Res<Time>,
    lesson: Res<Lesson>,
    font: Res<UiFont>,
    panel: Single<(Entity, &mut Visibility), With<LessonPanel>>,
    mut hinted: Local<bool>,
) {
    let hint = time.elapsed_secs() < lesson.hint_until;
    if !lesson.is_changed() && hint == *hinted {
        return;
    }
    *hinted = hint;
    let (panel, mut visibility) = panel.into_inner();
    visibility.set_if_neq(Visibility::Inherited);
    commands.entity(panel).despawn_related::<Children>();
    let all = chapters();
    let chapter = &all[lesson.chapter];
    let frame = commands
        .spawn((
            Node {
                flex_direction: FlexDirection::Column,
                row_gap: px(8.0),
                padding: UiRect::all(px(18.0)),
                width: px(456.0),
                ..default()
            },
            Frame::Plate,
            Accent(VIOLET),
        ))
        .id();
    let head = format!(
        "Обучение · глава {}: {}{}",
        lesson.chapter + 1,
        chapter.title,
        if lesson.finished {
            String::new()
        } else {
            format!("  ·  шаг {}/{}", lesson.step + 1, lesson.steps.len())
        }
    );
    let head = commands
        .spawn((Text::new(head), font.bold(13.0), TextColor(GOLD)))
        .id();
    commands.entity(frame).add_child(head);
    let words = if lesson.finished {
        "Глава пройдена!"
    } else {
        lesson.steps[lesson.step].text
    };
    let body = commands
        .spawn((Text::new(words), font.text(15.0), TextColor(INK)))
        .id();
    commands.entity(frame).add_child(body);
    if hint && !lesson.finished {
        let line = commands
            .spawn((
                Text::new(lesson.steps[lesson.step].refuse),
                font.bold(13.0),
                TextColor(BAD),
            ))
            .id();
        commands.entity(frame).add_child(line);
    }
    let row = commands
        .spawn(Node {
            column_gap: px(10.0),
            justify_content: JustifyContent::FlexEnd,
            ..default()
        })
        .id();
    let add = |commands: &mut Commands, what: LessonButton, label: &str, lit: bool| {
        let text = stats::label(commands, &font, label, 13.0, true);
        let b = commands
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
                    padding: UiRect::axes(px(14.0), px(7.0)),
                    ..default()
                },
            ))
            .add_child(text)
            .id();
        commands.entity(row).add_child(b);
    };
    if lesson.finished {
        if lesson.chapter + 1 < all.len() {
            add(
                &mut commands,
                LessonButton::NextChapter,
                "Следующая глава",
                true,
            );
        }
        add(
            &mut commands,
            LessonButton::Chapters,
            "К главам",
            lesson.chapter + 1 == all.len(),
        );
    } else {
        add(&mut commands, LessonButton::Chapters, "Выйти", false);
        if matches!(lesson.steps[lesson.step].until, Until::Next) {
            add(&mut commands, LessonButton::Next, "Далее", true);
        }
    }
    commands.entity(frame).add_child(row);
    commands.entity(panel).add_child(frame);
}

fn buttons(
    pressed: Query<(&Interaction, &LessonButton), Changed<Interaction>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut lesson: ResMut<Lesson>,
    mut exit: MessageWriter<AppExit>,
) {
    let mut chosen: Vec<LessonButton> = pressed
        .iter()
        .filter(|(i, _)| **i == Interaction::Pressed)
        .map(|(_, b)| *b)
        .collect();
    // Enter reads on, as «Далее» does.
    if keys.just_pressed(KeyCode::Enter)
        && !lesson.finished
        && matches!(lesson.steps[lesson.step].until, Until::Next)
    {
        chosen.push(LessonButton::Next);
    }
    for button in chosen {
        match button {
            LessonButton::Next => {
                if !lesson.finished && matches!(lesson.steps[lesson.step].until, Until::Next) {
                    lesson.next();
                }
            }
            LessonButton::NextChapter => {
                restart(&format!("tutorial:{}", lesson.chapter + 2), &mut exit);
            }
            LessonButton::Chapters => restart("tutorial", &mut exit),
        }
    }
}

/// A fresh process on the front screen `play` names (`NECROMY_PLAY`): the
/// simplest clean slate, as "Новая партия" does.
fn restart(play: &str, exit: &mut MessageWriter<AppExit>) {
    if let Ok(exe) = std::env::current_exe()
        && std::process::Command::new(exe)
            .args(std::env::args().skip(1))
            .env("NECROMY_PLAY", play)
            .spawn()
            .is_ok()
    {
        exit.write(AppExit::Success);
    }
}

// ---- Progress ----

fn progress_path() -> Option<std::path::PathBuf> {
    let state = std::env::var_os("XDG_STATE_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME").map(|h| std::path::Path::new(&h).join(".local/state"))
        })?;
    Some(state.join("necromy-table").join("tutorial"))
}

/// Chapters finished at least once.
pub fn done_chapters() -> Vec<usize> {
    progress_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|t| t.lines().filter_map(|l| l.trim().parse().ok()).collect())
        .unwrap_or_default()
}

fn mark_done(chapter: usize) {
    let mut done = done_chapters();
    if done.contains(&chapter) {
        return;
    }
    done.push(chapter);
    done.sort_unstable();
    if let Some(path) = progress_path() {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let text: String = done.iter().map(|c| format!("{c}\n")).collect();
        let _ = std::fs::write(path, text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use necromy_host::{FromTable, Seat, Table, ToTable};

    /// Every chapter can be played through as its steps ask: the demo of
    /// each step passes its gate and brings its event, dummies and scripts
    /// included, on a real table.
    #[test]
    fn every_chapter_plays_through() {
        for (n, chapter) in chapters().iter().enumerate() {
            let (game, events) = Game::scenario(&(chapter.scene)());
            let seats = (0..game.champions().len())
                .map(|i| if i == 0 { Seat::Human } else { Seat::Dummy })
                .collect();
            let mut table = Table::from_game(game, events, seats, 0, None, None);
            let mut view = table.game().clone();
            for (s, step) in (chapter.steps)().into_iter().enumerate() {
                let what = format!("chapter {} step {}", n + 1, s + 1);
                if let Some(r) = &step.rival {
                    let g = table.game();
                    let card = card_in_hand(g, r.seat, r.card)
                        .unwrap_or_else(|| panic!("{what}: rival lacks {}", r.card));
                    table
                        .act_as(
                            r.seat,
                            Intent::Play {
                                card,
                                target: Target::Champion(ME),
                            },
                        )
                        .unwrap_or_else(|e| panic!("{what}: rival's card refused: {e}"));
                }
                let Until::Event { test, .. } = &step.until else {
                    continue;
                };
                let mut met = false;
                for _ in 0..200 {
                    let mut heard = Vec::new();
                    for m in table.drain(ME) {
                        if let FromTable::Update {
                            serial,
                            events,
                            view: v,
                        } = m
                        {
                            heard.extend(events);
                            view = *v;
                            table.submit(ME, ToTable::Shown(serial));
                        }
                    }
                    if heard.iter().any(|e| test(e, &view)) {
                        met = true;
                        break;
                    }
                    if let Some(intent) = step.demo.as_ref().and_then(|d| d(&view, ME))
                        && (step.gate)(&view, ME, &intent)
                        && view.clone().apply(ME, intent.clone()).is_ok()
                    {
                        table.submit(ME, ToTable::Act(intent));
                    }
                    table.tick(0.4);
                }
                assert!(met, "{what} never came: {}", step.text);
            }
        }
    }

    #[test]
    fn focus_hexes_are_on_the_board() {
        for chapter in chapters() {
            let (game, _) = Game::scenario(&(chapter.scene)());
            for step in (chapter.steps)() {
                for h in step.focus_hexes {
                    assert!(game.board().contains(h), "{}: {h:?}", chapter.title);
                }
            }
        }
    }
}
