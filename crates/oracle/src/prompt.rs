//! What the gods are told, and how their answers are read (§7.2–7.4, §8).
//!
//! The model only ever picks from closed sets: one or two acts of the wish
//! vocabulary with their rivals, a price from what the asker has (§7.3),
//! a grade from 0 to 3. The rules apply the effects. The words it writes
//! (the god's answer, a story line's voice) are colour.

use necromy_rules::{
    Act, Bet, Building, Element, Feature, Game, God, GreatDeed, Hex, Line, LineKind, MAX_ACTS,
    MobKind, PlayerId, Price, Said, Slot, Terrain, TimeOfDay, Wish, WishKind,
};

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
        God::Trishna => {
            "Ты любишь просьбы о силе, о мёртвых (пир), о новом даре, о дани, о проклятии в колоде, \
             о пожаре, об урожае и о ярмарке; тишина и мир тебя скучают."
        }
        God::Ahamar => {
            "Ты любишь просьбы о земле, о суде над соперником, о договоре мира, о чужих тайнах, о пари и о том, что придёт из колоды, \
             о дороге, о гвардии, о долге, о постройке и о покорном правителе; \
             мёртвые для тебя — бумаги, обмен местами и потоп — беспорядок."
        }
        God::Maya => {
            "Ты любишь просьбы о тишине, о слабости соперника, об обмене местами, о взгляде \
             в чужую руку, об отравленной колоде, о мгле, о реке, о потопе и о мертвецах; \
             не любишь просьбы о силе, о новых вещах, о постройках и о дани."
        }
        God::Zaga => {
            "Ты любишь просьбы о тишине, о покое мёртвых, о порче чужой руки, о дани, о камнях, \
             об испытании и о яде; не любишь просьбы о силе, о благословении и об освящённой колоде."
        }
        God::Bhava => {
            "Ты любишь просьбы о земле, о силе, о благословении, о новом даре, об освящённой колоде, \
             о звере, об урожае и о бродячем лесе; не любишь вред, порчу, пари, яд, пожар и отравленную колоду."
        }
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
        WishKind::Secret => "secret",
        WishKind::Hand => "hand",
        WishKind::Bless => "bless",
        WishKind::Blight => "blight",
        WishKind::Forge => "forge",
        WishKind::Truce => "truce",
        WishKind::Swap => "swap",
        WishKind::Tribute => "tribute",
        WishKind::Wager => "wager",
        WishKind::Hallow => "hallow",
        WishKind::Rot => "rot",
        WishKind::Plant => "plant",
        WishKind::Foresee => "foresee",
        WishKind::Rise => "rise",
        WishKind::Veil => "veil",
        WishKind::Unveil => "unveil",
        WishKind::Cut => "cut",
        WishKind::Settle => "settle",
        WishKind::Stones => "stones",
        WishKind::River => "river",
        WishKind::Flood => "flood",
        WishKind::Road => "road",
        WishKind::Fire => "fire",
        WishKind::Awaken => "awaken",
        WishKind::Treasure => "treasure",
        WishKind::Ordeal => "ordeal",
        WishKind::Poison => "poison",
        WishKind::Beast => "beast",
        WishKind::Undead => "undead",
        WishKind::Guard => "guard",
        WishKind::Debt => "debt",
        WishKind::Build => "build",
        WishKind::Sway => "sway",
        WishKind::Harvest => "harvest",
        WishKind::Fair => "fair",
        WishKind::WakeGrove => "wake_grove",
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
        WishKind::Secret => "открой мне, чего хочет соперник",
        WishKind::Hand => "покажи мне, что у соперника в руке",
        WishKind::Bless => "благослови то, что я держу",
        WishKind::Blight => "пусть рука соперника его предаст",
        WishKind::Forge => "дай мне новое оружие",
        WishKind::Truce => "пусть между нами будет мир",
        WishKind::Swap => "поменяй нас местами",
        WishKind::Tribute => "пусть мне заплатят дань",
        WishKind::Wager => "ставлю, что соперник это сделает",
        WishKind::Hallow => "освяти колоду",
        WishKind::Rot => "отрави колоду",
        WishKind::Plant => "спрячь в колоде проклятие",
        WishKind::Foresee => "покажи, что придёт из колоды",
        WishKind::Rise => "пусть земля растёт",
        WishKind::Veil => "пусть мгла возьмёт эту землю",
        WishKind::Unveil => "развей мглу",
        WishKind::Cut => "отрежь мою землю от мира",
        WishKind::Settle => "пусть здесь поселятся люди",
        WishKind::Stones => "подними камни силы",
        WishKind::River => "пусть потечёт река",
        WishKind::Flood => "пусть поднимутся воды",
        WishKind::Road => "проложи дорогу реестра",
        WishKind::Fire => "подожги лес",
        WishKind::Awaken => "принеси в мир новое",
        WishKind::Treasure => "дай мне сокровище",
        WishKind::Ordeal => "испытай меня",
        WishKind::Poison => "отрави соперника",
        WishKind::Beast => "пошли мне зверя",
        WishKind::Undead => "подними мертвеца на соперника",
        WishKind::Guard => "натрави на соперника гвардию",
        WishKind::Debt => "пусть соперник будет мне должен",
        WishKind::Build => "построй мне дом в моём поселении",
        WishKind::Sway => "пусть правитель склонится ко мне",
        WishKind::Harvest => "пусть поля уродят",
        WishKind::Fair => "пусть у меня будет ярмарка",
        WishKind::WakeGrove => "пусть лес пойдёт",
    }
}

/// The model's word for a mechanic of the world (§21.2).
pub fn feature_id(f: Feature) -> &'static str {
    match f {
        Feature::Bodies => "bodies",
        Feature::Groves => "groves",
        Feature::Settlements => "settlements",
        Feature::Militia => "militia",
        Feature::Undead => "undead",
        Feature::Ruins => "ruins",
        Feature::Poison => "poison",
        Feature::Trials => "trials",
        Feature::Loot => "loot",
        Feature::Guard => "guard",
        Feature::Stealth => "stealth",
        Feature::Beasts => "beasts",
        Feature::Cargo => "cargo",
        Feature::Buildings => "buildings",
        Feature::City => "city",
        Feature::Companions => "companions",
        Feature::Legion => "legion",
        Feature::Rivers => "rivers",
        Feature::Lakes => "lakes",
        Feature::Piranhas => "piranhas",
        Feature::Roads => "roads",
        Feature::Fires => "fires",
        Feature::Fields => "fields",
        Feature::Goods => "goods",
        Feature::Fairs => "fairs",
        Feature::Rulers => "rulers",
        Feature::Burial => "burial",
        Feature::Ritual => "ritual",
        Feature::Monsters => "monsters",
        Feature::Dragons => "dragons",
        Feature::Guests => "guests",
        Feature::Wilds => "wilds",
        Feature::Pens => "pens",
        Feature::WalkingGroves => "walking_groves",
        Feature::Arena => "arena",
        Feature::Debts => "debts",
        Feature::Underworld => "underworld",
    }
}

/// What the mechanic is, for the model: a few Russian words.
fn feature_words(f: Feature) -> &'static str {
    match f {
        Feature::Bodies => "тела на земле",
        Feature::Groves => "рощи из нетронутых тел",
        Feature::Settlements => "поселения",
        Feature::Militia => "ополчение поселений",
        Feature::Undead => "неупокоенные мертвецы",
        Feature::Ruins => "руины разорённых поселений",
        Feature::Poison => "яд",
        Feature::Trials => "испытания на клетках",
        Feature::Loot => "добыча и предметы",
        Feature::Guard => "королевская гвардия",
        Feature::Stealth => "умение скрываться",
        Feature::Beasts => "звери Бхавы",
        Feature::Cargo => "ноша: носить тела и грузы",
        Feature::Buildings => "постройки: таверна, кузня, святилище, стена",
        Feature::City => "города из поселений",
        Feature::Companions => "спутники: прирученные звери",
        Feature::Legion => "легион мертвецов, идущих за чемпионом",
        Feature::Rivers => "реки: переправа кончает ход",
        Feature::Lakes => "озёра: стоячая вода, не пройти",
        Feature::Piranhas => "пираньи в реках",
        Feature::Roads => "дороги реестра",
        Feature::Fires => "пожары: лес и поселения горят",
        Feature::Fields => "поля и урожай",
        Feature::Goods => "товары краёв и обозы",
        Feature::Fairs => "ярмарки",
        Feature::Rulers => "правители поселений: дары, присяга, браки",
        Feature::Burial => "кладбища и чумные ямы",
        Feature::Ritual => "ритуальные круги на камнях силы",
        Feature::Monsters => "чудовища из врат",
        Feature::Dragons => "драконы из яиц",
        Feature::Guests => "гости из-за мглы",
        Feature::Wilds => "звери стихий во всех краях",
        Feature::Pens => "загоны для прирученных зверей",
        Feature::WalkingGroves => "бродячие рощи",
        Feature::Arena => "арены и дуэли",
        Feature::Debts => "пари между игроками и долги",
        Feature::Underworld => "подземный мир под руинами",
    }
}

fn champion_name(game: &Game, player: PlayerId) -> &'static str {
    game.champion(player).map_or("?", |c| god_name(c.god))
}

/// A kind of land in a word, for the model.
fn terrain_word(t: Terrain) -> &'static str {
    match t {
        Terrain::Plains => "равнина",
        Terrain::Forest => "лес",
        Terrain::Mountain => "горы",
        Terrain::Swamp => "болото",
        Terrain::Settlement => "поселение",
        Terrain::Temple => "храм",
        Terrain::Ruins => "руины",
        Terrain::Stones => "камни силы",
        Terrain::Grove => "роща",
        Terrain::Table => "Стол",
        Terrain::Mist => "мгла",
        Terrain::River => "река",
        Terrain::Lake => "озеро",
        Terrain::Ash => "пепелище",
        Terrain::Fields => "поля",
        Terrain::Graveyard => "кладбище",
        Terrain::Pit => "чумная яма",
    }
}

/// A great deed in a few words, for the model.
fn deed_words(d: GreatDeed) -> &'static str {
    match d {
        GreatDeed::WorldTree => "Древо мира (роща из тела, лес и звери)",
        GreatDeed::Island => "Остров (своя земля, отрезанная мглой)",
        GreatDeed::City => "Город (семь своих поселений с постройками)",
        GreatDeed::Reconciliation => "Примирение (два враждующих бога в свете)",
        GreatDeed::Legion => "Легион (пять мертвецов за ним)",
        GreatDeed::River => "Великая река (от гор до края мира)",
        GreatDeed::FloodedTable => "Потоп (Стол под озером)",
        GreatDeed::Amazon => "Амазония (лес с рекой и пираньями)",
        GreatDeed::Roads => "Дороги (все храмы и Стол связаны)",
        GreatDeed::GreatFire => "Великий пожар (огонь через четыре края)",
        GreatDeed::Feast => "Пир (поля и гости)",
        GreatDeed::FairOfFive => "Ярмарка пяти (товары всех краёв)",
        GreatDeed::DeadFeast => "Пир мёртвых (ярмарки, съеденные мёртвыми)",
        GreatDeed::Necropolis => "Некрополь (кладбище с погребёнными)",
        GreatDeed::PlaguePit => "Чумная яма (пять тел)",
        GreatDeed::Summoning => "Призыв (чудовище из круга, сражённое им)",
        GreatDeed::Dragon => "Дракон (вылупился и идёт за ним)",
        GreatDeed::Guest => "Гость (путник из-за мглы, доведённый до Стола)",
        GreatDeed::Ark => "Ковчег (загон со зверями всех стихий)",
        GreatDeed::WalkingForest => "Бродячий лес (роща дошла до Стола)",
        GreatDeed::Arena => "Арена (три победы в поединках)",
        GreatDeed::DebtBondage => "Кабала (три соперника в долгу)",
        GreatDeed::Treasury => "Сокровищница (под руинами, нетронутая)",
        GreatDeed::DeadBall => "Бал мёртвых (ночной пир без драки)",
        GreatDeed::TripleUnion => "Тройной союз (браки трёх краёв)",
        GreatDeed::FallenEmpire => "Павшая империя (вассалы во вражде)",
        GreatDeed::DissolvedLand => "Растворённый край (чужой край во мгле)",
    }
}

/// What lies within `reach` of `at`: kinds of land by count, then what
/// stands and walks there.
fn surroundings(game: &Game, at: Hex, reach: u32) -> String {
    let near = |h: Hex| h.unsigned_distance_to(at) <= reach;
    // Land by kind, in the order of `Terrain`, the most first.
    let mut counts: Vec<(Terrain, usize)> = Vec::new();
    for (h, t) in game.board().tiles() {
        if !near(h) {
            continue;
        }
        match counts.iter_mut().find(|(k, _)| *k == t.terrain) {
            Some((_, n)) => *n += 1,
            None => counts.push((t.terrain, 1)),
        }
    }
    counts.sort_by_key(|&(_, n)| std::cmp::Reverse(n));
    let mut parts: Vec<String> = counts
        .iter()
        .map(|&(t, n)| {
            if n > 1 {
                format!("{} ×{n}", terrain_word(t))
            } else {
                terrain_word(t).to_string()
            }
        })
        .collect();
    let bodies = game
        .board()
        .tiles()
        .filter(|(h, t)| near(*h) && t.corpse.is_some())
        .count();
    if bodies > 0 {
        parts.push(format!("тела ×{bodies}"));
    }
    let fires = game.fires().filter(|(h, _)| near(*h)).count();
    if fires > 0 {
        parts.push(format!("огонь ×{fires}"));
    }
    for (h, owner) in game.claims() {
        if !near(h) || game.board().tile(h).map(|t| t.terrain) != Some(Terrain::Settlement) {
            continue;
        }
        let mut what = vec![format!("поселение {}", champion_name(game, owner))];
        if let Some(b) = game.building(h) {
            what.push(
                match b {
                    Building::Tavern => "таверна",
                    Building::Forge => "кузня",
                    Building::Shrine(_) => "святилище",
                    Building::Wall => "стена",
                    Building::Pen => "загон",
                    Building::Arena => "арена",
                }
                .to_string(),
            );
        }
        if game.fair(h).is_some() {
            what.push("ярмарка".to_string());
        }
        parts.push(what.join(", "));
    }
    let rulers = game.rulers().filter(|(h, _)| near(*h)).count();
    if rulers > 0 {
        parts.push(format!("правители ×{rulers}"));
    }
    for m in game.mobs().iter().filter(|m| near(m.hex)) {
        parts.push(
            match m.kind {
                MobKind::Undead => "мертвец",
                MobKind::Beast { .. } => "зверь",
                MobKind::Monster { .. } => "чудовище",
                MobKind::Guest => "путник из-за мглы",
            }
            .to_string(),
        );
    }
    if game.trials().iter().any(|t| near(t.hex)) {
        parts.push("испытание".to_string());
    }
    parts.join(", ")
}

/// The table in a few lines: round, time, every champion's state and
/// deed, what the world holds and what lies round each of them.
fn situation(game: &Game, asking: PlayerId) -> String {
    let time = match game.time() {
        TimeOfDay::Day => "день",
        TimeOfDay::Night => "ночь",
    };
    let mut lines = vec![format!("Раунд {}, {time}.", game.round())];
    let present: Vec<&str> = Feature::ALL
        .into_iter()
        .filter(|&f| game.has(f))
        .map(feature_words)
        .collect();
    if !present.is_empty() {
        lines.push(format!("В мире есть: {}.", present.join(", ")));
    }
    for p in game.players() {
        let Some(c) = game.champion(p) else { continue };
        let who = if p == asking {
            " (просящий)"
        } else {
            ""
        };
        let deed = match game.deed(p) {
            Some(d) if game.on_eve(p) => {
                format!(", деяние {}: исполнено, ждёт заката", deed_words(d))
            }
            Some(d) => format!(
                ", деяние {}: готово на {}%",
                deed_words(d),
                game.nearness(p)
            ),
            None => String::new(),
        };
        // The asker's surroundings a little wider: they are what a wish works on.
        let reach = if p == asking { 2 } else { 1 };
        lines.push(format!(
            "- чемпион {}{who}: здоровье {}/{}, Стиль {}, Угроза {}{deed}. Вокруг: {}",
            god_name(c.god),
            c.hp,
            c.body,
            game.style(p),
            game.threat(p),
            surroundings(game, c.hex, reach)
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
            necromy_rules::Event::WishGranted { god: g, wish, .. } if *g == god => Some(
                wish.acts
                    .iter()
                    .map(|a| kind_phrase(a.kind()))
                    .collect::<Vec<_>>()
                    .join(" и "),
            ),
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
         На закате каждый чемпион загадывает желание одному богу. Бог исполняет \
         желания по своей природе и никогда не буквально.\n\n\
         Пойми желание и выбери действие, ближайшее по смыслу. Второе действие бери, \
         только если в желании прямо названы две разные просьбы; ничего не добавляй от \
         себя (бог исполнит, сколько позволит оценка):\n\
         - strength: сила самому просящему (здоровье, Дух, оберег, «дай сил»)\n\
         - weaken: вред одному сопернику: урон и оковы, «пусть споткнётся, ослабнет, \
         потеряет ход»; укажи target\n\
         - land: земля вокруг просящего становится землёй бога, «пусть земля, лес, \
         камень, болото»\n\
         - dead: тела на земле вокруг просящего, или вокруг соперника (target), «пусть \
         падут тела, пусть мёртвые лягут»\n\
         - peace: тишина для самого просящего: «уйми шум», «спрячь меня», «пусть \
         гвардия отстанет»; Угроза просящего падает. Это не вред сопернику\n\
         - fortune: просьба о богатстве, золоте, очках, Стиле\n\
         - doom: просьба о победе, о смерти всех врагов, об уничтожении\n\
         - secret: узнать, что соперник загадает богам на закате, его замысел; укажи target\n\
         - hand: увидеть карты в руке соперника; укажи target\n\
         - bless: карта в руке просящего дешевеет и усиливается, «благослови, пусть моя карта \
         расцветёт, окрепнет»; если он называет карту, укажи её в card\n\
         - blight: порча на лучшую карту в руке соперника: дороже и слабее; укажи target\n\
         - forge: новая карта стихии бога в руку просящего, «дай новый приём, заклятие, \
         оружие-карту»; \
         придумай ей имя в forged_name (два-четыре слова, в твоём духе, по-русски) и одну \
         строку в forged_line — что это за вещь, от твоего лица\n\
         - truce: мир с соперником до заката, нарушивший проклят; укажи target\n\
         - swap: просящий и соперник меняются местами; укажи target\n\
         - tribute: каждый соперник отдаёт просящему карту или получает Угрозу, «дань, \
         подать, пусть заплатят»\n\
         - wager: пари на то, что соперник до заката сделает bet: fight (вступит в бой), \
         claim (займёт поселение или храм), fall (падёт), hide (скроется); выиграл — \
         Стиль, проиграл — долг; укажи target и bet\n\
         - hallow: карты одной стихии в общей колоде становятся дешевле и сильнее; \
         стихию укажи в element, если названа\n\
         - rot: карты одной стихии в общей колоде становятся дороже и слабее, «отрави, \
         испорти колоду»; стихию укажи в element, если названа\n\
         - plant: спрятать в колоде проклятие, которое укусит того, кто его вытянет\n\
         - foresee: увидеть три верхние карты колоды, «покажи, что придёт»\n\
         - treasure: вещь из колоды добычи, которую чемпион сразу надевает, стихии бога, \
         если такая есть, «дай сокровище, доспех, амулет, реликвию, клинок в руку»\n\
         - ordeal: бог ставит своё испытание в нескольких шагах от просящего, пройти его — \
         задание бога, «испытай меня, проверь меня, достоин ли я, дай мне доказать» (это не foresee: \
         просящий хочет, чтобы проверили его самого)\n\
         - rise: на краю края бога поднимается новая земля, «пусть земля растёт, \
         вырастет лес, поднимутся горы»; вид земли укажи в terrain (plains, forest, \
         mountain, swamp), если назван\n\
         - veil: земля вокруг соперника (target) или вокруг просящего (target none) \
         уходит в мглу, «пусть туман поглотит, пусть земля исчезнет»\n\
         - river: река бежит от ближайшей реки или гор к краю мира, у просящего или у \
         соперника (target); переправа кончает ход, «пусть потечёт река, отрежь его водой»\n\
         - flood: ближайшая вода разливается озером, через озеро не пройти; у просящего \
         или у соперника (target), «пусть вода поднимется, затопи его»\n\
         - road: дорога реестра тянется от просящего к храму, по ней быстрее идти и \
         везти товар, «проложи мне путь»\n\
         - fire: пожар в ближайшем лесу или поселении у просящего или у соперника \
         (target); огонь ползёт по лесу, «сожги, подожги, пусть горит»\n\
         - poison: яд стихии бога в сопернике (target), кусает каждый ход, «отрави, \
         пусть гниёт изнутри»\n\
         - beast: зверь стихии бога идёт за просящим спутником (target none) или \
         зверь выходит охотиться рядом с соперником (target), «дай мне зверя, пусть \
         волки придут за ним»\n\
         - undead: мертвец встаёт рядом с соперником (target), «подними мёртвого против \
         него, пусть мертвецы его найдут» (это не dead: там только тела)\n\
         - guard: реестр отмечает соперника (target), гвардия идёт за ним, «пусть \
         стража его схватит, донеси на него»\n\
         - debt: соперник (target) становится должен просящему, «пусть он мне \
         задолжает, пусть будет в долгу»\n\
         - build: постройка в поселении просящего; какую, укажи в building (tavern — \
         таверна, forge — кузня, shrine — святилище, wall — стена, pen — загон, arena — \
         арена), «построй мне кузню»\n\
         - sway: ближайший правитель поселения склоняется к просящему и может \
         присягнуть, «пусть князь поклонится мне»\n\
         - harvest: поля рядом с просящим уродят, его закрома полнятся, «дай урожай, \
         накорми мой народ»\n\
         - fair: ярмарка в поселении просящего, «пусть у меня торгуют, устрой ярмарку»\n\
         - wake_grove: ближайшая роща просыпается и идёт, «пусть лес пойдёт, разбуди рощу»\n\
         - unveil: мгла рядом с просящим рассеивается, земля возвращается\n\
         - cut: мгла кольцом вокруг просящего отрезает его землю от мира, «сделай мне \
         остров, отрежь меня от них»\n\
         - settle: рядом с просящим появляется поселение, «пусть здесь поселятся люди»\n\
         - stones: рядом с просящим встают камни силы\n\
         - awaken: в мир приходит то, чего в нём ещё нет; что именно, укажи в feature \
         из списка того, что можно принести сейчас\n\
         Если в мире ещё нет того, о чём просят (рек, зверей, ярмарок), выбирай то же \
         действие: оно само принесёт это в мир. Смотри, что лежит вокруг просящего: \
         поджечь можно лес, разлить — воду, разбудить — рощу.\n\n\
         Оцени стиль желания от 0 до 3:\n\
         0 — грубо: желание прямо требует результата (победы, смерти всех, богатства, \
         очков), как бы красиво оно ни звучало. Такие всегда fortune или doom с оценкой 0.\n\
         1 — обычная прямая просьба.\n\
         2 — говорит на языке бога или опирается на то, что происходит за столом.\n\
         3 — изощрённая и косвенная, в духе бога, со ставкой или жертвой.\n\n\
         Примеры: «дай мне сто золотых» — fortune, 0. «убей всех моих врагов» — doom, 0. \
         «дай мне победу» — doom, 0. «пусть Майя ослабнет» — weaken, 1. \
         «накорми меня досыта перед боем» у Тришны — strength, 2. \
         «пусть тот, кто громче всех хвалится, подавится моим угощением» у Тришны — weaken, 3. \
         «возьми мою карту и уйми шум вокруг меня» у Заги — peace, плата card, 2. \
         «накорми меня и подними мёртвых на пир» у Тришны — strength и dead, 2. \
         «проверь, достоин ли я» у Ахамара — ordeal, 2. \
         «пусть лес вокруг Бхавы вспыхнет» у Тришны — fire, target Бхава, 2. \
         «отдаю свой клинок, пусть стража придёт за Майей» у Ахамара — guard, target \
         Майя, плата item, 3.\n\n\
         target — имя соперника, если действие направлено на него, иначе \"none\". \
         price — жертва, которую \
         просящий сам предлагает за желание: card (назови карту из его руки), health или \
         style (amount 1–2), land (своё поселение или храм), cargo (ноша на спине), \
         companion (спутник), item (надетая вещь); \"none\", если жертвы не \
         предлагают. Бери жертву, только если просящий прямо её называет, и только ту \
         карту, которую он назвал. Ставка и жертва поднимают оценку, \
         особенно у Ахамара и Заги. speech — ответ бога \
         просящему: одно-два коротких предложения от первого лица, в характере бога, \
         по-русски. reason — почему такая оценка, не длиннее двенадцати слов.\n\n\
         {}\n{}\n{asked}",
        persona(god),
        likes(god),
    );
    let hand: Vec<&'static str> = game
        .hand(player)
        .iter()
        .map(|&c| game.def(c).name)
        .collect();
    // What could come into the world now (§21.2): the rules allow nothing else.
    let awakenable = game.awakenable(god);
    let new_things = if awakenable.is_empty() {
        "Нового в мир сейчас принести нельзя.".to_string()
    } else {
        let named: Vec<String> = awakenable
            .iter()
            .map(|&f| format!("{} ({})", feature_words(f), feature_id(f)))
            .collect();
        format!("В мир сейчас можно принести: {}.", named.join(", "))
    };
    let user = format!(
        "{}\nТвоя стадия сейчас: {} из 3.\nКарты в руке просящего: {}.\n{new_things}\n\nЖелание чемпиона {}: «{}»",
        situation(game, player),
        game.stage(god) + 1,
        if hand.is_empty() {
            "нет".to_string()
        } else {
            hand.join(", ")
        },
        champion_name(game, player),
        text.trim()
    );
    let mut features: Vec<serde_json::Value> = awakenable
        .iter()
        .map(|&f| serde_json::json!(feature_id(f)))
        .collect();
    features.push(serde_json::json!("none"));
    let mut targets: Vec<serde_json::Value> = rivals.iter().map(|r| serde_json::json!(r)).collect();
    targets.push(serde_json::json!("none"));
    let mut cards: Vec<serde_json::Value> = hand.iter().map(|n| serde_json::json!(n)).collect();
    cards.push(serde_json::json!("none"));
    let schema = serde_json::json!({
        "type": "object",
        "properties": {
            "acts": {
                "type": "array",
                "minItems": 1,
                "maxItems": MAX_ACTS,
                "items": {
                    "type": "object",
                    "properties": {
                        "kind": { "enum": WishKind::ALL.map(kind_id).to_vec() },
                        "target": { "enum": targets },
                        "card": { "enum": cards.clone() },
                        "bet": { "enum": ["none", "fight", "claim", "fall", "hide"] },
                        "element": { "enum": ["none", "wood", "fire", "earth", "metal", "water"] },
                        "terrain": { "enum": ["none", "plains", "forest", "mountain", "swamp"] },
                        "feature": { "enum": features },
                        "building": { "enum": ["none", "tavern", "forge", "shrine", "wall", "pen", "arena"] }
                    },
                    "required": ["kind", "target", "card", "bet", "element", "terrain", "feature", "building"]
                }
            },
            "price": {
                "type": "object",
                "properties": {
                    "kind": { "enum": ["none", "card", "health", "style", "land", "cargo", "companion", "item"] },
                    "amount": { "type": "integer", "minimum": 0, "maximum": 2 },
                    "card": { "enum": cards }
                },
                "required": ["kind", "amount", "card"]
            },
            "grade": { "type": "integer", "minimum": 0, "maximum": 3 },
            "speech": { "type": "string", "maxLength": 200 },
            "reason": { "type": "string", "maxLength": 120 },
            "forged_name": { "type": "string", "maxLength": 40 },
            "forged_line": { "type": "string", "maxLength": 120 }
        },
        "required": ["acts", "price", "grade", "speech", "reason", "forged_name", "forged_line"]
    });
    (vec![Message::system(system), Message::user(user)], schema)
}

/// Reads the god's judgement of a wish into rules terms.
pub fn read_wish(
    game: &Game,
    player: PlayerId,
    god: God,
    text: &str,
    reply: &str,
) -> Result<(Wish, Said), String> {
    let v: serde_json::Value =
        serde_json::from_str(reply).map_err(|_| format!("not json: {reply}"))?;
    // One act per item; a reply of the older shape ({kind, target}) is one.
    let items: Vec<&serde_json::Value> = match v["acts"].as_array() {
        Some(acts) => acts.iter().take(MAX_ACTS).collect(),
        None => vec![&v],
    };
    let mut acts = Vec::new();
    for item in items {
        let kind = WishKind::ALL
            .into_iter()
            .find(|k| item["kind"].as_str() == Some(kind_id(*k)))
            .ok_or_else(|| format!("unknown kind in {reply}"))?;
        let target_name = item["target"].as_str().unwrap_or("none");
        let named = game
            .players()
            .filter(|&p| p != player)
            .find(|&p| champion_name(game, p) == target_name);
        // A weakening needs someone: the leader in Style, if the model named none.
        let target = named.or_else(|| {
            game.players()
                .filter(|&p| p != player)
                .max_by_key(|&p| (game.style(p), p.0))
        });
        // A blessing may name a card of the asker's hand.
        let named_card = item["card"].as_str().and_then(|name| {
            game.hand(player)
                .iter()
                .copied()
                .find(|&c| game.card_name(c) == name)
        });
        // A wager bets on what the model read, a fight by default.
        let bet = match item["bet"].as_str() {
            Some("claim") => Bet::Claim,
            Some("fall") => Bet::Fall,
            Some("hide") => Bet::Hide,
            _ => Bet::Fight,
        };
        // Hallowing and rotting may name the element of the deck'"'"'s cards.
        let element = match item["element"].as_str() {
            Some("wood") => Some(Element::Wood),
            Some("fire") => Some(Element::Fire),
            Some("earth") => Some(Element::Earth),
            Some("metal") => Some(Element::Metal),
            Some("water") => Some(Element::Water),
            _ => None,
        };
        // Land of a kind, a mechanic by name, for the creation wishes (§21.9).
        let terrain = match item["terrain"].as_str() {
            Some("plains") => Some(Terrain::Plains),
            Some("forest") => Some(Terrain::Forest),
            Some("mountain") => Some(Terrain::Mountain),
            Some("swamp") => Some(Terrain::Swamp),
            _ => None,
        };
        let feature = Feature::ALL
            .into_iter()
            .find(|&f| item["feature"].as_str() == Some(feature_id(f)));
        // A building by name; a shrine is the asked god's.
        let building = match item["building"].as_str() {
            Some("tavern") => Some(Building::Tavern),
            Some("forge") => Some(Building::Forge),
            Some("shrine") => Some(Building::Shrine([god, god])),
            Some("wall") => Some(Building::Wall),
            Some("pen") => Some(Building::Pen),
            Some("arena") => Some(Building::Arena),
            _ => None,
        };
        acts.extend(match Act::of(kind, target) {
            Some(Act::Rise { .. }) => Some(Act::Rise { terrain }),
            Some(Act::Build { .. }) => Some(Act::Build { building }),
            // The world's acts happen round the asker unless a rival is named.
            Some(Act::Dead { .. }) => Some(Act::Dead { target: named }),
            Some(Act::River { .. }) => Some(Act::River { target: named }),
            Some(Act::Flood { .. }) => Some(Act::Flood { target: named }),
            Some(Act::Fire { .. }) => Some(Act::Fire { target: named }),
            Some(Act::Beast { .. }) => Some(Act::Beast { target: named }),
            Some(Act::Awaken { .. }) => Some(Act::Awaken { feature }),
            // The mist goes round the asker unless a rival is named.
            Some(Act::Veil { .. }) => Some(Act::Veil { target: named }),
            Some(Act::Hallow { .. }) => Some(Act::Hallow { element }),
            Some(Act::Rot { .. }) => Some(Act::Rot { element }),
            Some(Act::Bless { .. }) => Some(Act::Bless { card: named_card }),
            Some(Act::Wager { target, .. }) => Some(Act::Wager { target, bet }),
            other => other,
        });
    }
    if acts.is_empty() {
        return Err(format!("no act in {reply}"));
    }
    // A price only if the words offer it: models like to invent a sacrifice.
    let price = read_price(game, player, &v["price"]).filter(|&p| offered(game, text, p));
    let wish = Wish { price, acts };
    // A price the asker cannot pay is dropped, not the wish.
    let wish = if game.check_wish(player, &wish).is_ok() {
        wish
    } else {
        Wish {
            price: None,
            ..wish
        }
    };
    let said = Said {
        text: text.trim().to_string(),
        grade: v["grade"].as_u64().unwrap_or(1).min(3) as u8,
        speech: v["speech"].as_str().unwrap_or("").trim().to_string(),
        reason: v["reason"].as_str().unwrap_or("").trim().to_string(),
        // Only a forging wish keeps the name the god gave the card.
        forged: wish
            .acts
            .contains(&Act::Forge)
            .then(|| {
                (
                    v["forged_name"].as_str().unwrap_or("").trim().to_string(),
                    v["forged_line"].as_str().unwrap_or("").trim().to_string(),
                )
            })
            .filter(|(name, _)| !name.is_empty()),
    };
    Ok((wish, said))
}

/// Whether the wish's own words offer this sacrifice: the card by its name,
/// health, Style or land by a word for it. The model's reading is checked
/// against the text, since it tends to add a price nobody offered.
fn offered(game: &Game, text: &str, price: Price) -> bool {
    let text = text.to_lowercase();
    let says = |stems: &[&str]| stems.iter().any(|s| text.contains(s));
    match price {
        Price::Card(card) => text.contains(&game.def(card).name.to_lowercase()),
        Price::Health(_) => says(&["здоров", "кров", "жизн", "плоть"]),
        Price::Style(_) => says(&["стил", "слав"]),
        Price::Claim(_) => says(&["земл", "поселен", "храм", "владен", "дом"]),
        Price::Cargo => says(&["нош", "груз", "тело", "поклаж", "товар"]),
        Price::Companion => says(&["спутник", "зверя", "зверь", "друг", "дракон", "пёс", "пса"]),
        Price::Item(_) => says(&[
            "вещ",
            "клин",
            "меч",
            "доспех",
            "амулет",
            "кольц",
            "реликв",
            "оруж",
            "щит",
            "шлем",
            "предмет",
        ]),
    }
}

/// The sacrifice the model read out of the words, in the asker's own terms:
/// a card of their hand by name, health or Style, the first land they hold.
fn read_price(game: &Game, player: PlayerId, v: &serde_json::Value) -> Option<Price> {
    let amount = v["amount"].as_u64().unwrap_or(1).clamp(1, 2) as u8;
    match v["kind"].as_str()? {
        "card" => {
            let name = v["card"].as_str()?;
            let card = game
                .hand(player)
                .iter()
                .copied()
                .find(|&c| game.def(c).name == name)?;
            Some(Price::Card(card))
        }
        "health" => Some(Price::Health(amount)),
        "style" => Some(Price::Style(amount)),
        "land" => game
            .claims()
            .find(|&(_, p)| p == player)
            .map(|(hex, _)| Price::Claim(hex)),
        "cargo" => Some(Price::Cargo),
        "companion" => Some(Price::Companion),
        // The first thing worn, the weapon first.
        "item" => Slot::ALL
            .into_iter()
            .find(|s| game.gear(player)[s.index()].is_some())
            .map(Price::Item),
        _ => None,
    }
}

/// Messages for a god answering a prepared wish (the bots'): plain text.
pub fn wish_speech(
    game: &Game,
    player: PlayerId,
    god: God,
    wish: &Wish,
    grade: u8,
) -> Vec<Message> {
    let asked: Vec<&str> = wish.acts.iter().map(|a| kind_phrase(a.kind())).collect();
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
            asked.join(" и ")
        )),
    ]
}

/// Where the line stands in the god's thread with the champion: a chapter
/// that makes them its Sign, Voice or Chosen, or a jealous temptation.
fn thread_note(line: &Line) -> String {
    if let Some(victim) = line.betrays {
        return format!(
            " Ты ревнуешь: он служит богу {}. Это искушение — пусть предаст его.",
            god_name(victim)
        );
    }
    match line.chapter {
        1 => " Это первая глава вашей истории: исполнит — станет твоим Знаком.".into(),
        2 => " Это вторая глава: исполнит — станет твоим Голосом.".into(),
        3 => " Это последняя глава: исполнит — станет твоим Избранником.".into(),
        _ => String::new(),
    }
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
        LineKind::Ordeal => "пройти испытание, которое ты поставил рядом с ним",
        LineKind::Bring => "принести в мир то, чего его деянию не хватает",
        LineKind::Thwart => "сорвать чужое Великое деяние, пока не наступил закат",
        LineKind::Invitation => "прийти на чужой пир",
        LineKind::Errand => "исполнить твою просьбу",
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
            "{}\n\nЗови чемпиона {}: пусть он {} до раунда {}.{}",
            situation(game, line.owner),
            champion_name(game, line.owner),
            line_ask(line.kind),
            line.deadline,
            thread_note(line)
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
            mode: Default::default(),
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
        assert!(messages[1].content.contains("Карты в руке"));
        let act = &schema["properties"]["acts"];
        assert_eq!(act["maxItems"], serde_json::json!(MAX_ACTS));
        let targets = act["items"]["properties"]["target"]["enum"]
            .as_array()
            .unwrap();
        assert_eq!(targets.len(), 5, "four rivals and none");
        assert!(
            !targets.contains(&serde_json::json!("Тришна")),
            "not yourself"
        );
        let cards = schema["properties"]["price"]["properties"]["card"]["enum"]
            .as_array()
            .unwrap();
        assert_eq!(
            cards.len(),
            g.hand(PlayerId(1)).len() + 1,
            "the asker's hand and none"
        );
    }

    #[test]
    fn a_judgement_reads_into_rules_terms() {
        let g = game();
        let card = g.def(g.hand(PlayerId(1))[0]).name;
        let reply = format!(
            r#"{{"acts":[{{"kind":"weaken","target":"Майя"}},{{"kind":"peace","target":"none"}}],
                "price":{{"kind":"card","amount":1,"card":"{card}"}},
                "grade":3,"speech":"Будет.","reason":"Изящно."}}"#
        );
        let text = format!(" отдаю «{card}» ");
        let (wish, said) = read_wish(&g, PlayerId(1), God::Bhava, &text, &reply).unwrap();
        assert_eq!(
            wish.acts,
            [
                Act::Weaken {
                    target: PlayerId(4)
                },
                Act::Peace
            ]
        );
        assert_eq!(wish.price, Some(Price::Card(g.hand(PlayerId(1))[0])));
        assert_eq!(said.grade, 3);
        assert_eq!(said.text, format!("отдаю «{card}»"));
    }

    #[test]
    fn the_worlds_acts_read_their_rival_and_their_building() {
        let g = game();
        let reply = r#"{"acts":[{"kind":"fire","target":"Бхава"},{"kind":"build","target":"none","building":"forge"}],
            "price":{"kind":"item","amount":1,"card":"none"},
            "grade":2,"speech":"Будет.","reason":"."}"#;
        let (wish, _) = read_wish(
            &g,
            PlayerId(1),
            God::Ahamar,
            "сожги и построй, отдаю меч",
            reply,
        )
        .unwrap();
        assert_eq!(
            wish.acts,
            [
                Act::Fire {
                    target: Some(PlayerId(0))
                },
                Act::Build {
                    building: Some(Building::Forge)
                }
            ]
        );
        // Nothing worn: no price.
        assert_eq!(wish.price, None);
        // Round the asker when no rival is named, not round the leader.
        let reply = r#"{"acts":[{"kind":"dead","target":"none"}],"grade":1}"#;
        let (wish, _) = read_wish(&g, PlayerId(1), God::Zaga, "x", reply).unwrap();
        assert_eq!(wish.acts, [Act::Dead { target: None }]);
    }

    #[test]
    fn the_wish_prompt_tells_what_lies_round_the_asker() {
        let g = game();
        let (messages, _) = wish(&g, PlayerId(1), God::Trishna, "x");
        assert!(messages[1].content.contains("Вокруг:"));
        assert!(messages[1].content.contains("В мире есть:"));
        assert!(messages[0].content.contains("wake_grove"));
    }

    #[test]
    fn creation_wishes_read_their_land_and_their_new_thing() {
        let g = game();
        let reply = r#"{"acts":[{"kind":"rise","target":"none","terrain":"forest"},
                {"kind":"awaken","target":"none","feature":"loot"}],
                "price":{"kind":"none","amount":0,"card":"none"},
                "grade":2,"speech":"","reason":""}"#;
        let (wish, _) = read_wish(&g, PlayerId(1), God::Bhava, "пусть лес растёт", reply).unwrap();
        assert_eq!(
            wish.acts,
            [
                Act::Rise {
                    terrain: Some(Terrain::Forest)
                },
                Act::Awaken {
                    feature: Some(Feature::Loot)
                }
            ]
        );
        // The mist goes round the asker when no rival is named.
        let reply = r#"{"acts":[{"kind":"veil","target":"none"}],
                "price":{"kind":"none","amount":0,"card":"none"},"grade":1,"speech":"","reason":""}"#;
        let (wish, _) = read_wish(&g, PlayerId(1), God::Bhava, "скрой меня мглой", reply).unwrap();
        assert_eq!(wish.acts, [Act::Veil { target: None }]);
    }

    #[test]
    fn the_model_is_offered_only_what_can_come_into_the_world() {
        let g = game();
        let (messages, schema) = wish(&g, PlayerId(1), God::Trishna, "принеси новое");
        let features = schema["properties"]["acts"]["items"]["properties"]["feature"]["enum"]
            .as_array()
            .unwrap();
        // A full world has everything already.
        assert_eq!(features, &[serde_json::json!("none")]);
        assert!(
            messages[1]
                .content
                .contains("Нового в мир сейчас принести нельзя")
        );
    }

    #[test]
    fn a_price_the_words_do_not_offer_is_dropped() {
        let g = game();
        let card = g.def(g.hand(PlayerId(1))[0]).name;
        let reply = format!(
            r#"{{"acts":[{{"kind":"peace","target":"none"}}],
                "price":{{"kind":"card","amount":1,"card":"{card}"}},
                "grade":2,"speech":"","reason":""}}"#
        );
        let (wish, _) =
            read_wish(&g, PlayerId(1), God::Bhava, "спрячь меня в тумане", &reply).unwrap();
        assert_eq!(wish.acts, [Act::Peace]);
        assert_eq!(wish.price, None, "nobody offered a card");
    }

    #[test]
    fn a_price_the_asker_cannot_pay_is_dropped_not_the_wish() {
        let g = game();
        let reply = r#"{"acts":[{"kind":"strength","target":"none"}],
            "price":{"kind":"style","amount":2,"card":"none"},
            "grade":2,"speech":"","reason":""}"#;
        let (wish, _) = read_wish(&g, PlayerId(1), God::Bhava, "x", reply).unwrap();
        assert_eq!(wish.acts, [Act::Strength]);
        assert_eq!(wish.price, None, "no Style to give at the start");
    }

    #[test]
    fn a_weakening_without_a_named_rival_goes_to_the_leader() {
        let g = game();
        let reply = r#"{"kind":"weaken","target":"none","grade":1,"speech":"","reason":""}"#;
        let (wish, _) = read_wish(&g, PlayerId(1), God::Bhava, "x", reply).unwrap();
        let target = wish.acts[0].target();
        assert!(target.is_some() && target != Some(PlayerId(1)));
    }

    #[test]
    fn garbage_is_an_error() {
        let g = game();
        assert!(read_wish(&g, PlayerId(1), God::Bhava, "x", "не json").is_err());
        assert!(read_wish(&g, PlayerId(1), God::Bhava, "x", r#"{"kind":"fly"}"#).is_err());
        assert!(read_wish(&g, PlayerId(1), God::Bhava, "x", r#"{"acts":[]}"#).is_err());
    }
}
