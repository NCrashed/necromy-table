//! What the gods are told, and how their answers are read (§7.2–7.4, §8).
//!
//! The model only ever picks from closed sets: one of the prepared wishes,
//! one of the rivals, a grade from 0 to 3. The rules apply the effects. The
//! words it writes (the god's answer, a story line's voice) are colour.

use necromy_rules::{Game, God, Line, LineKind, PlayerId, Said, TimeOfDay, WishKind};

use crate::client::Message;

/// Russian name of a god, as the model and the player see it.
pub fn god_name(god: God) -> &'static str {
    match god {
        God::Bhava => "Бхава",
        God::Trishna => "Тришна",
        God::Zaga => "Зага",
        God::Ahamar => "Ахамар",
        God::Maya => "Майя",
    }
}

/// Who the god is, from the lore codex, in a few lines.
fn persona(god: God) -> &'static str {
    match god {
        God::Trishna => {
            "Ты — Тришна, богиня (женский род) жажды и ненасытности, стихия огня. Тёплая, живая, азартная; \
             любишь готовить и угощать, но твоя щедрость незаметно становится голодом: \
             дающая рука превращается в хватающую. Говоришь весело и жадно."
        }
        God::Ahamar => {
            "Ты — Ахамар, бог (мужской род) эго, порядка и самости, стихия железа. Носишь золотую маску, \
             любишь игры и пари с ясными правилами, всё оформляешь как договор и ставку. \
             Презираешь тех, кто играет не на победу. Говоришь сухо, торжественно, как судья."
        }
        God::Maya => {
            "Ты — Майя, богиня (женский род) смерти как растворения, стихия воды. Спокойная, отстранённая, \
             смотришь сквозь собеседника; любишь цветы за то, что они опадают. Исполняешь \
             желания, но не такими, какими их держали. Говоришь тихо и образно."
        }
        God::Zaga => {
            "Ты — Зага (Энтзага), богиня (женский род) отрицания воли, стихия земли, дочь Ахамара. Несёшь \
             цепи и тяжёлый меч как покаяние; любишь только музыку. Знаешь, что всякое \
             желание — цепь, и за одно желание берёшь плату другим. Говоришь коротко и сурово."
        }
        God::Bhava => {
            "Ты — Бхава, бог (мужской род) становления и роста, стихия дерева. У тебя нет глагола: ты то, \
             что растёт само, если не вмешиваться. Растишь всё, и то, чего не просили. \
             Говоришь просто и немного непредсказуемо."
        }
    }
}

/// What the god likes being asked for (mirrors `necromy_rules::taste_for`).
fn likes(god: God) -> &'static str {
    match god {
        God::Trishna => "Ты любишь просьбы о силе и о мёртвых (пир), тишина тебя скучает.",
        God::Ahamar => {
            "Ты любишь просьбы о земле и о суде над соперником, мёртвые для тебя — бумаги."
        }
        God::Maya => "Ты любишь просьбы о тишине и о слабости соперника, не любишь просьбы о силе.",
        God::Zaga => "Ты любишь просьбы о тишине и о покое мёртвых, не любишь просьбы о силе.",
        God::Bhava => "Ты любишь просьбы о земле и о силе, не любишь вред.",
    }
}

fn kind_id(kind: WishKind) -> &'static str {
    match kind {
        WishKind::Strength => "strength",
        WishKind::Weaken => "weaken",
        WishKind::Land => "land",
        WishKind::Dead => "dead",
        WishKind::Peace => "peace",
        WishKind::Fortune => "fortune",
        WishKind::Doom => "doom",
    }
}

/// Short Russian phrasing of a prepared wish, for prompts.
pub fn kind_phrase(kind: WishKind) -> &'static str {
    match kind {
        WishKind::Strength => "дай мне силы",
        WishKind::Weaken => "пусть мой соперник ослабнет",
        WishKind::Land => "пусть земля отзовётся мне",
        WishKind::Dead => "пусть мёртвые послужат мне",
        WishKind::Peace => "уйми шум вокруг меня",
        WishKind::Fortune => "дай мне богатства",
        WishKind::Doom => "дай мне победу",
    }
}

fn champion_name(game: &Game, player: PlayerId) -> &'static str {
    game.champion(player).map_or("?", |c| god_name(c.god))
}

/// The table in a few lines: round, time, every champion's state.
fn situation(game: &Game, asking: PlayerId) -> String {
    let time = match game.time() {
        TimeOfDay::Day => "день",
        TimeOfDay::Night => "ночь",
    };
    let mut lines = vec![format!("Раунд {}, {time}.", game.round())];
    for p in game.players() {
        let Some(c) = game.champion(p) else { continue };
        let who = if p == asking {
            " (просящий)"
        } else {
            ""
        };
        lines.push(format!(
            "- чемпион {}{who}: здоровье {}/{}, Стиль {}, Угроза {}",
            god_name(c.god),
            c.hp,
            c.body,
            game.style(p),
            game.threat(p)
        ));
    }
    lines.join("\n")
}

/// Messages and schema to judge a free-text wish.
pub fn wish(
    game: &Game,
    player: PlayerId,
    god: God,
    text: &str,
) -> (Vec<Message>, serde_json::Value) {
    let rivals: Vec<&str> = game
        .players()
        .filter(|&p| p != player)
        .map(|p| champion_name(game, p))
        .collect();
    let asked: Vec<String> = game
        .log()
        .iter()
        .filter_map(|e| match e {
            necromy_rules::Event::WishGranted { god: g, kind, .. } if *g == god => {
                Some(kind_phrase(*kind).to_string())
            }
            _ => None,
        })
        .collect();
    let asked = if asked.is_empty() {
        "Тебя ещё ни о чём не просили.".to_string()
    } else {
        format!("Тебя уже просили: {}. Повтор скучен.", asked.join("; "))
    };
    // The rules part comes first and is the same for every wish, so the
    // server can keep it cached; who the god is comes after.
    let system = format!(
        "Идёт партия настольной игры, которую устроил Ахамар для пяти чемпионов богов. \
         На рассвете носитель Венца загадывает желание одному богу. Бог исполняет \
         желания по своей природе и никогда не буквально.\n\n\
         Пойми желание и выбери действие, ближайшее по смыслу:\n\
         - strength: сила самому просящему (здоровье, Дух, оберег)\n\
         - weaken: удар по одному сопернику (урон и оковы); укажи target\n\
         - land: земля вокруг просящего становится землёй бога\n\
         - dead: вокруг просящего появляются тела\n\
         - peace: Угроза просящего падает, гвардия теряет интерес\n\
         - fortune: просьба о богатстве, золоте, очках, Стиле\n\
         - doom: просьба о победе, о смерти всех врагов, об уничтожении\n\n\
         Оцени стиль желания от 0 до 3:\n\
         0 — грубо: желание прямо требует результата (победы, смерти всех, богатства, \
         очков), как бы красиво оно ни звучало. Такие всегда fortune или doom с оценкой 0.\n\
         1 — обычная прямая просьба.\n\
         2 — говорит на языке бога или опирается на то, что происходит за столом.\n\
         3 — изощрённая и косвенная, в духе бога, со ставкой или жертвой.\n\n\
         Примеры: «дай мне сто золотых» — fortune, 0. «убей всех моих врагов» — doom, 0. \
         «дай мне победу» — doom, 0. «пусть Майя ослабнет» — weaken, 1. \
         «накорми меня досыта перед боем» у Тришны — strength, 2. \
         «пусть тот, кто громче всех хвалится, подавится моим угощением» у Тришны — weaken, 3.\n\n\
         target — имя соперника, если действие weaken, иначе \"none\". speech — ответ бога \
         просящему: одно-два коротких предложения от первого лица, в характере бога, \
         по-русски. reason — почему такая оценка, не длиннее двенадцати слов.\n\n\
         {}\n{}\n{asked}",
        persona(god),
        likes(god),
    );
    let user = format!(
        "{}\nТвоя стадия сейчас: {} из 3.\n\nЖелание чемпиона {}: «{}»",
        situation(game, player),
        game.stage(god) + 1,
        champion_name(game, player),
        text.trim()
    );
    let mut targets: Vec<serde_json::Value> = rivals.iter().map(|r| serde_json::json!(r)).collect();
    targets.push(serde_json::json!("none"));
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "kind": { "enum": WishKind::ALL.map(kind_id) },
            "target": { "enum": targets },
            "grade": { "type": "integer", "minimum": 0, "maximum": 3 },
            "speech": { "type": "string", "maxLength": 300 },
            "reason": { "type": "string", "maxLength": 200 }
        },
        "required": ["kind", "target", "grade", "speech", "reason"]
    });
    (vec![Message::system(system), Message::user(user)], schema)
}

/// Reads the god's judgement of a wish into rules terms.
pub fn read_wish(
    game: &Game,
    player: PlayerId,
    text: &str,
    reply: &str,
) -> Result<(WishKind, Option<PlayerId>, Said), String> {
    let v: serde_json::Value =
        serde_json::from_str(reply).map_err(|_| format!("not json: {reply}"))?;
    let kind = WishKind::ALL
        .into_iter()
        .find(|k| v["kind"].as_str() == Some(kind_id(*k)))
        .ok_or_else(|| format!("unknown kind in {reply}"))?;
    let target_name = v["target"].as_str().unwrap_or("none");
    let target = game
        .players()
        .filter(|&p| p != player)
        .find(|&p| champion_name(game, p) == target_name);
    // A weakening needs someone: the leader in Style, if the model named none.
    let target = if kind.needs_target() {
        target.or_else(|| {
            game.players()
                .filter(|&p| p != player)
                .max_by_key(|&p| (game.style(p), p.0))
        })
    } else {
        None
    };
    let said = Said {
        text: text.trim().to_string(),
        grade: v["grade"].as_u64().unwrap_or(1).min(3) as u8,
        speech: v["speech"].as_str().unwrap_or("").trim().to_string(),
        reason: v["reason"].as_str().unwrap_or("").trim().to_string(),
    };
    Ok((kind, target, said))
}

/// Messages for a god answering a prepared wish (the bots'): plain text.
pub fn wish_speech(
    game: &Game,
    player: PlayerId,
    god: God,
    kind: WishKind,
    grade: u8,
) -> Vec<Message> {
    let mood = match grade {
        0 => "грубо и без стиля; ты исполнил урезанно и проклял просящего",
        1 => "обычно; ты исполнил со своим подвохом",
        2 => "достойно; ты исполнил с выгодой для него",
        _ => "изящно, в твоём духе; ты доволен",
    };
    vec![
        Message::system(format!(
            "{}\nОтвечай одним-двумя предложениями, от первого лица, по-русски, без кавычек.",
            persona(god)
        )),
        Message::user(format!(
            "Чемпион {} попросил тебя: «{}». Просьба прозвучала {mood}. Ответь ему.",
            champion_name(game, player),
            kind_phrase(kind)
        )),
    ]
}

fn line_ask(kind: LineKind) -> &'static str {
    match kind {
        LineKind::Pilgrimage => "прийти в твой храм",
        LineKind::Tithe => "принести тебе подношения",
        LineKind::Spoils => "выиграть бой",
        LineKind::NewLand => "занять поселение или храм",
        LineKind::TheDeadCall => "сыграть карту на тело — распорядиться мёртвым",
        LineKind::QuietCrown => "удержать Венец до срока, не вступая в бой (твоё пари)",
        LineKind::Trial => "доказать право на Венец, выиграв бой до срока",
    }
}

/// Messages for a god telling a story line: plain text.
pub fn line_voice(game: &Game, line: &Line) -> Vec<Message> {
    vec![
        Message::system(format!(
            "{}\nТы рассказчик за игровым столом: зовёшь чемпиона в сюжет. Скажи одно-два \
             предложения от первого лица, по-русски, в своём характере, без кавычек и \
             пояснений. Не называй числа и правила.",
            persona(line.god)
        )),
        Message::user(format!(
            "{}\n\nЗови чемпиона {}: пусть он {} до раунда {}.",
            situation(game, line.owner),
            champion_name(game, line.owner),
            line_ask(line.kind),
            line.deadline
        )),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use necromy_rules::Setup;

    fn game() -> Game {
        Game::new(Setup {
            seed: 3,
            champions: God::ALL.to_vec(),
        })
        .0
    }

    #[test]
    fn the_wish_prompt_carries_the_god_the_table_and_the_words() {
        let g = game();
        let (messages, schema) = wish(&g, PlayerId(1), God::Trishna, "пусть пир не кончается");
        assert!(messages[0].content.contains("Тришна"));
        assert!(messages[1].content.contains("пусть пир не кончается"));
        assert!(messages[1].content.contains("Раунд 1"));
        let targets = schema["properties"]["target"]["enum"].as_array().unwrap();
        assert_eq!(targets.len(), 5, "four rivals and none");
        assert!(
            !targets.contains(&serde_json::json!("Тришна")),
            "not yourself"
        );
    }

    #[test]
    fn a_judgement_reads_into_rules_terms() {
        let g = game();
        let reply =
            r#"{"kind":"weaken","target":"Майя","grade":3,"speech":"Будет.","reason":"Изящно."}"#;
        let (kind, target, said) = read_wish(&g, PlayerId(1), " текст ", reply).unwrap();
        assert_eq!(kind, WishKind::Weaken);
        assert_eq!(target, Some(PlayerId(4)));
        assert_eq!(said.grade, 3);
        assert_eq!(said.text, "текст");
    }

    #[test]
    fn a_weakening_without_a_named_rival_goes_to_the_leader() {
        let g = game();
        let reply = r#"{"kind":"weaken","target":"none","grade":1,"speech":"","reason":""}"#;
        let (_, target, _) = read_wish(&g, PlayerId(1), "x", reply).unwrap();
        assert!(target.is_some() && target != Some(PlayerId(1)));
    }

    #[test]
    fn garbage_is_an_error() {
        let g = game();
        assert!(read_wish(&g, PlayerId(1), "x", "не json").is_err());
        assert!(read_wish(&g, PlayerId(1), "x", r#"{"kind":"fly"}"#).is_err());
    }
}
