//! Russian display names and colours for rules types. The rules crate keeps
//! stable English identifiers; everything the player reads comes from here.

use bevy::prelude::*;
use necromy_rules::{CardKind, Element, God, Timing};

pub fn god(god: God) -> &'static str {
    match god {
        God::Bhava => "Бхава",
        God::Trishna => "Тришна",
        God::Zaga => "Зага",
        God::Ahamar => "Ахамар",
        God::Maya => "Майя",
    }
}

pub fn element(element: Element) -> &'static str {
    match element {
        Element::Wood => "дерево",
        Element::Fire => "огонь",
        Element::Earth => "земля",
        Element::Metal => "железо",
        Element::Water => "вода",
    }
}

/// "гасит воду", "гасит землю".
pub fn element_accusative(element: Element) -> &'static str {
    match element {
        Element::Earth => "землю",
        Element::Water => "воду",
        e => self::element(e),
    }
}

/// "обереги воды", "после дерева".
pub fn element_genitive(element: Element) -> &'static str {
    match element {
        Element::Wood => "дерева",
        Element::Fire => "огня",
        Element::Earth => "земли",
        Element::Metal => "железа",
        Element::Water => "воды",
    }
}

/// "гасится водой".
pub fn element_instrumental(element: Element) -> &'static str {
    match element {
        Element::Wood => "деревом",
        Element::Fire => "огнём",
        Element::Earth => "землёй",
        Element::Metal => "железом",
        Element::Water => "водой",
    }
}

pub fn kind(kind: CardKind) -> &'static str {
    match kind {
        CardKind::Rite => "обряд",
        CardKind::Trick => "уловка",
        CardKind::Body => "тело",
    }
}

pub fn timing(timing: Timing) -> &'static str {
    match timing {
        Timing::Own => "свой ход",
        Timing::Instant => "мгновенно",
        Timing::Response => "ответ",
    }
}

/// Element colour: the accent of the god who owns it; grey for neutral.
pub fn element_color(element: Option<Element>) -> Color {
    match element {
        Some(e) => {
            let [r, g, b] = God::ALL[e.index()].accent();
            Color::srgb_u8(r, g, b)
        }
        None => Color::srgb(0.62, 0.60, 0.56),
    }
}

pub fn face(face: necromy_rules::Face) -> &'static str {
    use necromy_rules::Face;
    match face {
        Face::Strike => "удар",
        Face::Shield => "щит",
        Face::Sun => "солнце",
        Face::Moon => "луна",
        Face::Element => "стихия",
        Face::Blank => "пусто",
    }
}

/// Stage names from light (0) to dark (2), docs/design.md §5.
pub fn stage(god: God, stage: u8) -> &'static str {
    let names = match god {
        God::Trishna => ["Щедрость", "Жажда", "Пожирание"],
        God::Ahamar => ["Маска", "Трещина", "Разоблачение"],
        God::Maya => ["Покой", "Манипуляция", "Гнев"],
        God::Zaga => ["Покой", "Бремя", "Приговор"],
        God::Bhava => ["Росток", "Чаща", "Заросль"],
    };
    names[(stage as usize).min(2)]
}

pub fn deed(deed: necromy_rules::Deed) -> String {
    use necromy_rules::{BodyVerb, Deed};
    match deed {
        Deed::Played(e) => format!("играть карты стихии «{}»", element(e)),
        Deed::Body(BodyVerb::Fuel) => "сжигать тела".into(),
        Deed::Body(BodyVerb::Legion) => "вписывать тела в легион".into(),
        Deed::Body(BodyVerb::Dissolve) => "растворять души".into(),
        Deed::Body(BodyVerb::Rest) => "упокаивать тела".into(),
        Deed::Body(BodyVerb::Seed) => "сеять в мёртвом".into(),
        Deed::Attacked => "нападать первым".into(),
        Deed::Fought => "драться".into(),
        Deed::Won => "побеждать в бою".into(),
        Deed::Prayed => "молиться в храмах".into(),
        Deed::Claimed => "занимать поселения и храмы".into(),
    }
}

pub fn taste(kind: necromy_rules::TasteKind) -> &'static str {
    use necromy_rules::TasteKind;
    match kind {
        TasteKind::Balance => "Равновесие: ценится всё понемногу",
        TasteKind::Land => "Стол: место за Столом Ахамара стоит втрое",
        TasteKind::Stage => "Сцена: верность характеру стоит вдвое",
        TasteKind::Arena => "Арена: победы стоят вдвое",
    }
}

pub fn style_reason(reason: necromy_rules::StyleReason) -> &'static str {
    use necromy_rules::StyleReason;
    match reason {
        StyleReason::Territory => "владения",
        StyleReason::Battle => "бой",
        StyleReason::Manner => "верность манере",
        StyleReason::Oath => "нарушенная клятва",
        StyleReason::Wish => "оценку желания",
        StyleReason::Story => "сюжет",
        StyleReason::Trial => "испытание",
        StyleReason::Item => "плату тёмному богу",
        StyleReason::First => "первенство",
        StyleReason::Variety => "разнообразие дел",
    }
}

/// What was done first at the table (§21.5), as the feed says it: «первым …».
pub fn novelty(n: necromy_rules::Novelty) -> String {
    use necromy_rules::Novelty;
    match n {
        Novelty::WonBattle => "выигрывает бой".into(),
        Novelty::FelledGuard => "повергает гвардию".into(),
        Novelty::LaidToRest => "упокаивает мертвеца".into(),
        Novelty::SlewBeast => "одолевает зверя".into(),
        Novelty::PassedTrial => "проходит испытание".into(),
        Novelty::TookSettlement => "занимает поселение".into(),
        Novelty::TookTable => "садится за Стол Ахамара".into(),
        Novelty::Rebuilt => "отстраивает руины".into(),
        Novelty::Sacrificed => "отдаёт предмет богу".into(),
        Novelty::Hid => "уходит в тень".into(),
        Novelty::FinishedLine => "доводит до конца свою историю".into(),
        Novelty::Wished(kind) => format!("просит «{}»", wish(kind).to_lowercase()),
        Novelty::Brought(f) => format!("приносит в мир: {}", feature(f).0.to_lowercase()),
        Novelty::PlayedFor(f) => format!("пускает в ход: {}", feature(f).0.to_lowercase()),
    }
}

/// Name and what the terrain does, for the hover tooltip.
pub fn terrain(terrain: necromy_rules::Terrain) -> (&'static str, &'static str) {
    use necromy_rules::Terrain;
    match terrain {
        Terrain::Plains => ("Равнина", "обычная земля"),
        Terrain::Forest => ("Лес", "идти дороже"),
        Terrain::Mountain => ("Горы", "идти дороже; защитнику +1 кубик"),
        Terrain::Swamp => ("Топь", "обычный проход"),
        Terrain::Settlement => ("Поселение", "войди — займёшь"),
        Terrain::Temple => (
            "Храм",
            "конец хода здесь — молитва богу края; здесь рука перебирается без потерь",
        ),
        Terrain::Ruins => ("Руины", "пока без особых свойств"),
        Terrain::Stones => ("Камни силы", "пока без особых свойств"),
        Terrain::Grove => ("Роща", "выросла из нетронутого тела; идти дороже"),
        Terrain::Table => ("Стол Ахамара", "центр; займи — больше всего Стиля"),
        Terrain::Mist => ("Мгла", "земли здесь нет: не пройти, пока мгла не развеется"),
    }
}

/// "край Бхавы": the god's name in the genitive.
pub fn god_genitive(god: God) -> &'static str {
    match god {
        God::Bhava => "Бхавы",
        God::Trishna => "Тришны",
        God::Zaga => "Заги",
        God::Ahamar => "Ахамара",
        God::Maya => "Майи",
    }
}

/// The taste's short name and its explanation, split for chip and tooltip.
pub fn taste_parts(kind: necromy_rules::TasteKind) -> (&'static str, &'static str) {
    let full = taste(kind);
    full.split_once(": ").unwrap_or((full, ""))
}

/// Name and what it asks of a Great Deed (§21.7).
pub fn great_deed(d: necromy_rules::GreatDeed) -> (&'static str, String) {
    use necromy_rules::GreatDeed;
    match d {
        GreatDeed::WorldTree => (
            "Мировое древо",
            "Роща, выросшая из тела чемпиона (своего тоже), вокруг шесть клеток леса или рощ, рядом звери — и так два заката подряд.".into(),
        ),
        GreatDeed::Island => (
            "Остров",
            format!(
                "Земля в {} клеток и больше, отрезанная мглой от Стола; на ней твоё поселение, и ты стоишь на ней.",
                necromy_rules::ISLAND
            ),
        ),
        GreatDeed::DissolvedLand => (
            "Растворение края",
            format!(
                "Отпусти чужой край: вся его земля, кроме храма и домов чемпионов, уходит во мглу — и не меньше {} клеток.",
                necromy_rules::DISSOLVED
            ),
        ),
    }
}

pub fn check(kind: necromy_rules::CheckKind) -> &'static str {
    use necromy_rules::CheckKind;
    match kind {
        CheckKind::HeroGrove => "роща из тела чемпиона",
        CheckKind::WoodsAround => "лес и рощи вокруг",
        CheckKind::BeastsNear => "звери рядом",
        CheckKind::Dusks => "закатов подряд",
        CheckKind::IslandSize => "клеток на отрезанной земле",
        CheckKind::IslandSettled => "твоё поселение на ней",
        CheckKind::RegionInMist => "клеток края во мгле",
    }
}

/// How the Dominant phrases a prepared wish.
pub fn wish(kind: necromy_rules::WishKind) -> &'static str {
    use necromy_rules::WishKind;
    match kind {
        WishKind::Strength => "Дай мне силы",
        WishKind::Weaken => "Пусть мой соперник ослабнет",
        WishKind::Land => "Пусть земля отзовётся мне",
        WishKind::Dead => "Пусть мёртвые послужат мне",
        WishKind::Peace => "Уйми шум вокруг меня",
        WishKind::Fortune => "Дай мне богатства",
        WishKind::Doom => "Дай мне победу",
        WishKind::Secret => "Открой мне, чего хочет соперник",
        WishKind::Hand => "Покажи, что у соперника в руке",
        WishKind::Bless => "Благослови то, что я держу",
        WishKind::Blight => "Пусть рука соперника его предаст",
        WishKind::Forge => "Дай мне новое оружие",
        WishKind::Truce => "Пусть между нами будет мир",
        WishKind::Swap => "Поменяй нас местами",
        WishKind::Tribute => "Пусть мне заплатят дань",
        WishKind::Wager => "Ставлю, что соперник вступит в бой",
        WishKind::Hallow => "Освяти колоду",
        WishKind::Rot => "Отрави колоду",
        WishKind::Plant => "Спрячь в колоде проклятие",
        WishKind::Foresee => "Покажи, что придёт из колоды",
        WishKind::Rise => "Пусть земля растёт",
        WishKind::Veil => "Пусть мгла возьмёт эту землю",
        WishKind::Unveil => "Развей мглу",
        WishKind::Cut => "Отрежь мою землю от мира",
        WishKind::Settle => "Пусть здесь поселятся люди",
        WishKind::Stones => "Подними камни силы",
        WishKind::Awaken => "Принеси в мир новое",
    }
}

/// A mechanic of the world (§21.2): its name and what it brings, as the
/// table tells it when it comes in.
pub fn feature(f: necromy_rules::Feature) -> (&'static str, &'static str) {
    use necromy_rules::Feature;
    match f {
        Feature::Bodies => (
            "Тела",
            "павшие и ночь оставляют тела; карты тел играют на них",
        ),
        Feature::Groves => ("Рощи", "нетронутое тело прорастает рощей"),
        Feature::Settlements => ("Поселения", "их занимают, и на рассвете они дают Стиль"),
        Feature::Militia => (
            "Ополчение",
            "у поселений свои отряды: друзей пропускают, прочих бьют",
        ),
        Feature::Undead => (
            "Неупокоенные",
            "тело, о котором никто не позаботился, встаёт мертвецом",
        ),
        Feature::Ruins => (
            "Руины",
            "поселение без ополчения мертвецы разоряют; руины можно отстроить",
        ),
        Feature::Poison => (
            "Яд",
            "яд стихии кусает каждый ход и лечится гасящей стихией",
        ),
        Feature::Trials => (
            "Испытания",
            "боги ставят на клетках испытания с наградой и ценой",
        ),
        Feature::Loot => ("Добыча", "предметы: оружие, облачение, реликвии"),
        Feature::Guard => (
            "Гвардия",
            "королевская гвардия выходит против самого шумного",
        ),
        Feature::Stealth => ("Скрытность", "в лесу и болоте ночью можно скрыться"),
        Feature::Beasts => ("Звери", "ночью из лесов Бхавы выходят звери"),
    }
}

/// A whole wish in the Dominant's words: its acts, one after the other.
pub fn wish_phrase(wish: &necromy_rules::Wish) -> String {
    let parts: Vec<String> = wish
        .acts
        .iter()
        .enumerate()
        .map(|(i, act)| {
            let phrase = self::wish(act.kind());
            if i == 0 {
                phrase.to_string()
            } else {
                // The second act joins the first in lower case.
                let mut lower = phrase.chars();
                lower
                    .next()
                    .map(|c| c.to_lowercase().chain(lower).collect())
                    .unwrap_or_default()
            }
        })
        .collect();
    parts.join(", и ")
}

/// What the Dominant gave for a wish, `card` naming a card of theirs.
pub fn price(
    price: necromy_rules::Price,
    card: impl Fn(necromy_rules::CardId) -> String,
) -> String {
    use necromy_rules::Price;
    match price {
        Price::Card(c) => format!("карту «{}»", card(c)),
        Price::Health(1) => "1 здоровья".into(),
        Price::Health(n) => format!("{n} здоровья"),
        Price::Style(1) => "1 Стиль".into(),
        Price::Style(n) => format!("{n} Стиля"),
        Price::Claim(_) => "свою землю".into(),
    }
}

/// What a god likes to be asked, as a hint in the wish panel.
pub fn god_likes(god: God) -> &'static str {
    match god {
        God::Trishna => {
            "Любит просьбы о силе и о мёртвых, тишина её скучает. Дарит — и растит свой голод."
        }
        God::Ahamar => {
            "Любит просьбы о земле и суде над соперником, мёртвые для него — бумаги. Записывает долг: +Угроза."
        }
        God::Maya => {
            "Любит просьбы о тишине и о слабости соперника, не любит просьбы о силе. Растворяет карту из руки."
        }
        God::Zaga => {
            "Любит просьбы о тишине и покое мёртвых, не любит просьбы о силе. Берёт плату Духом."
        }
        God::Bhava => "Любит просьбы о земле и о силе, не любит вред. Растит рощу и у соперника.",
    }
}

/// The god's answer, by grade (§7.4).
pub fn god_speech(god: God, grade: u8) -> &'static str {
    match (god, grade) {
        (God::Trishna, 0) => "Ты просишь, как голодный — и получишь, как голодный.",
        (God::Trishna, 1) => "Бери. Только не удивляйся, что захочется ещё.",
        (God::Trishna, 2) => "Вот это аппетит! Угощайся.",
        (God::Trishna, _) => "Ах, какой пир ты мне принёс. Держи — и приходи снова.",
        (God::Ahamar, 0) => "Без ставки, без правил. Исполню — и впишу тебя в долговую книгу.",
        (God::Ahamar, 1) => "Принято. Условия запомнены.",
        (God::Ahamar, 2) => "Достойная ставка. Порядок на твоей стороне.",
        (God::Ahamar, _) => "Вот игрок, который играет на себя. Уважаю.",
        (God::Maya, 0) => "Ты держишься за то, что тает у тебя в руках.",
        (God::Maya, 1) => "Будет. Но не таким, каким ты это держал.",
        (God::Maya, 2) => "Красиво. Пусть расцветёт — и опадёт в свой срок.",
        (God::Maya, _) => "Ты почти отпустил. Возьми этот цветок.",
        (God::Zaga, 0) => "Хотение ради хотения. Цепь затянется туже.",
        (God::Zaga, 1) => "Одно желание — ценой другого.",
        (God::Zaga, 2) => "Ты просишь меньше, чем мог бы. Это слышно.",
        (God::Zaga, _) => "Тишина. Как отзвучавшая нота.",
        (God::Bhava, 0) => "Растёт всё. И то, чего ты не просил.",
        (God::Bhava, 1) => "Прорастёт. Где захочет.",
        (God::Bhava, 2) => "Земля слышит тебя.",
        (God::Bhava, _) => "Ты не тронул — и вот, всё зелено.",
    }
}

/// "просит Тришну": the god's name in the accusative.
pub fn god_accusative(god: God) -> &'static str {
    match god {
        God::Bhava => "Бхаву",
        God::Trishna => "Тришну",
        God::Zaga => "Загу",
        God::Ahamar => "Ахамара",
        God::Maya => "Майю",
    }
}

/// Story lines (§8): title, and the god's words when telling it.
pub fn line_title(kind: necromy_rules::LineKind) -> &'static str {
    use necromy_rules::LineKind;
    match kind {
        LineKind::Pilgrimage => "Паломничество",
        LineKind::Tithe => "Десятина",
        LineKind::Spoils => "Трофей",
        LineKind::NewLand => "Новые земли",
        LineKind::TheDeadCall => "Мёртвые зовут",
        LineKind::QuietCrown => "Тихий Венец",
        LineKind::Trial => "Испытание Венца",
        LineKind::Ordeal => "Испытание бога",
    }
}

pub fn line_voice(kind: necromy_rules::LineKind) -> &'static str {
    use necromy_rules::LineKind;
    match kind {
        LineKind::Pilgrimage => "Приди в мой храм, пока дорога открыта.",
        LineKind::Tithe => "Ты забыл обо мне. Поднеси мне — и я вспомню тебя.",
        LineKind::Spoils => "Хочется крови? Возьми её в бою — получишь больше.",
        LineKind::NewLand => "Земля без хозяина — беспорядок. Займи её.",
        LineKind::TheDeadCall => "Мёртвые ждут, что ты с ними сделаешь.",
        LineKind::QuietCrown => "Ставка: удержи Венец до срока, не обнажая меча.",
        LineKind::Trial => "Докажи, что Венец твой: выиграй бой до срока.",
        LineKind::Ordeal => "Я поставил тебе испытание рядом. Выстоишь — запомню.",
    }
}

/// What a line asks, with progress where it has one.
pub fn line_goal(line: &necromy_rules::Line, g: &necromy_rules::Game) -> String {
    use necromy_rules::Goal;
    match line.goal {
        Goal::ReachHex(hex) => {
            let region = g
                .board()
                .tile(hex)
                .and_then(|t| t.region)
                .map_or("центр".to_string(), |god| {
                    format!("край {}", god_genitive(god))
                });
            format!("дойди до храма ({region}, подсвечен)")
        }
        Goal::Offer { god, amount, from } => format!(
            "поднеси {}: {}/{amount}",
            god_dative(god),
            g.favor(line.owner, god).saturating_sub(from).min(amount)
        ),
        Goal::WinBattle => "выиграй бой".into(),
        Goal::Claim => "займи поселение или храм".into(),
        Goal::Body => "сыграй карту на тело".into(),
        Goal::AvoidBattle => "не участвуй в боях".into(),
        Goal::PassTrial(_) => "пройди испытание (подсвечено)".into(),
    }
}

/// "поднеси Тришне": the god's name in the dative.
pub fn god_dative(god: God) -> &'static str {
    match god {
        God::Bhava => "Бхаве",
        God::Trishna => "Тришне",
        God::Zaga => "Заге",
        God::Ahamar => "Ахамару",
        God::Maya => "Майе",
    }
}

pub fn world_stir(stir: necromy_rules::WorldStir) -> &'static str {
    use necromy_rules::WorldStir;
    match stir {
        WorldStir::RisingDead => "На столе тихо — и мёртвые поднимаются у центра.",
        WorldStir::Overgrowth => "На столе тихо — и чаща разрастается сама.",
        WorldStir::Unrest => "На столе тихо — и по королевству ползёт тревога: всем +1 Угрозы.",
    }
}

/// Why a hidden champion was seen again (§11.6).
pub fn reveal(why: necromy_rules::RevealReason) -> &'static str {
    use necromy_rules::RevealReason::*;
    match why {
        Attacked => "удар из засады",
        Aimed => "карта в соперника",
        Crowd => "там люди",
        Spotted => "замечен рядом с соперником",
        Guard => "мимо прошла гвардия",
        Dawn => "рассвет в чистом поле",
        Stumbled => "на него наткнулись",
        Trial => "вышел на испытание",
    }
}

/// The god's land of the kingdom (docs/design.md §3).
pub fn land(god: God) -> &'static str {
    match god {
        God::Bhava => "Дикая Чаща",
        God::Trishna => "Пиршественные Земли",
        God::Zaga => "Серые Скиты",
        God::Ahamar => "Коронные Реестры",
        God::Maya => "Тихий Луг",
    }
}

/// Who the god's champion is (art/champions.md).
pub fn champion_title(god: God) -> &'static str {
    match god {
        God::Bhava => "Страж Нетронутого",
        God::Trishna => "Кухарка Пира",
        God::Zaga => "Кающийся Железного Скита",
        God::Ahamar => "Рыцарь-Регистратор",
        God::Maya => "Плакальщица Тихого Луга",
    }
}

/// Yang, yin, or both (earth), for the ring's rhythm (§4).
pub fn yin_yang(element: Element) -> &'static str {
    match (element.is_yang(), element.is_yin()) {
        (true, true) => "инь и ян",
        (true, false) => "ян",
        _ => "инь",
    }
}

/// What a law of the world does, in one line (§5.3). Its name is the name
/// of its god's stage (`stage`).
pub fn law_text(law: necromy_rules::Law) -> &'static str {
    use necromy_rules::Law::*;
    match law {
        Sprout => "тела прорастают рощей на раунд раньше",
        Thicket => "в лесу и роще можно скрыться и днём",
        Wildgrowth => "на закате равнина в краю Бхавы дичает в лес",
        Generosity => "поселение лечит на 1 того, кто начал в нём ход",
        Thirst => "кто в бою нанёс больше, получает +1 Дух",
        Devouring => "на закате тела в краю Тришны сгорают, в поселениях голод: −1 здоровья",
        Stillness => "на закате у всех −1 Угрозы",
        Burden => "каждая карта сверх двух за ход: +1 Угрозы",
        Sentence => "гвардия выходит уже при Угрозе 3 и бьёт на кубик больше",
        Mask => "у кого есть владения, тому +1 Стиль на рассвете",
        Crack => "владение платит на рассвете, только если хозяин не дальше 3 клеток",
        Exposure => "днём не скрыться нигде",
        Rest => "павший просыпается с полным Духом",
        Manipulation => "ночью скрыться можно на любой клетке",
        Wrath => "реагировать можно только в 1 клетке; павший теряет карту",
    }
}

/// The law's name: its god's stage.
pub fn law_name(law: necromy_rules::Law) -> &'static str {
    stage(law.god(), law.stage())
}

/// What the god's Chosen are spared or given (§5.4).
pub fn chosen_gift(god: God) -> &'static str {
    match god {
        God::Bhava => "лес и роща стоят 1 очко хода",
        God::Trishna => "голод Пожирания не трогает",
        God::Zaga => "Бремя не шумит, Приговор бьёт без лишнего кубика",
        God::Ahamar => "можно скрыться днём и при Разоблачении; все владения платят",
        God::Maya => "в Гневе павший не теряет карту",
    }
}

/// A rung of a god's patronage (§5.4).
pub fn patronage(p: necromy_rules::Patronage) -> &'static str {
    use necromy_rules::Patronage::*;
    match p {
        None => "нет",
        Sign => "Знак",
        Voice => "Голос",
        Chosen => "Избранник",
    }
}

/// A line for a card a wish changed (§7.3): what changed, green when it is
/// a gift to its holder, red when a blight.
pub fn card_mod(m: &necromy_rules::CardMod) -> (String, bevy::color::Color) {
    let mut parts = Vec::new();
    if m.cost != 0 {
        parts.push(format!("цена {:+}", m.cost));
    }
    if m.power != 0 {
        parts.push(format!("сила {:+}", m.power));
    }
    if m.range != 0 {
        parts.push(format!("дальность {:+}", m.range));
    }
    let gift = m.power - m.cost + m.range >= 0;
    let head = if gift {
        "Дар бога"
    } else {
        "Порча"
    };
    let color = if gift {
        bevy::color::Color::srgb(0.55, 0.9, 0.5)
    } else {
        bevy::color::Color::srgb(1.0, 0.5, 0.45)
    };
    (format!("{head}: {}", parts.join(", ")), color)
}

/// What a wager bets a rival will do before dusk, after the rival's name.
pub fn bet(bet: necromy_rules::Bet) -> &'static str {
    use necromy_rules::Bet;
    match bet {
        Bet::Fight => "вступит в бой",
        Bet::Claim => "займёт поселение или храм",
        Bet::Fall => "падёт",
        Bet::Hide => "скроется",
    }
}

/// "испытание тишины": each god's trial, by what it tests (§20.2).
pub fn trial_name(god: God) -> &'static str {
    match god {
        God::Bhava => "испытание чащи",
        God::Trishna => "испытание пира",
        God::Zaga => "испытание тишины",
        God::Ahamar => "испытание реестра",
        God::Maya => "испытание тумана",
    }
}

/// What a trial asks: "2 × щит; стихия идёт в зачёт".
pub fn trial_ask(g: &necromy_rules::Game, trial: &necromy_rules::Trial) -> String {
    use necromy_rules::Face;
    let face = trial.face();
    let when = match face {
        Face::Sun => " (только днём)",
        Face::Moon => " (только ночью)",
        _ => "",
    };
    let wild = if face == Face::Element {
        ""
    } else {
        "; стихия идёт в зачёт"
    };
    format!("{} × {}{when}{wild}", g.trial_need(trial), self::face(face))
}

/// What passing the trial gives now.
pub fn boon(g: &necromy_rules::Game, trial: &necromy_rules::Trial) -> String {
    use necromy_rules::Boon;
    let n = g.boon_amount(trial);
    match trial.boon {
        Boon::Style => format!("+{n} Стиля"),
        Boon::Favour => format!("+{n} благосклонности {}", god_genitive(trial.god)),
        Boon::Cards => format!("{n} карт(ы)"),
        Boon::Mending => format!("+{n} здоровья, яд снят"),
        Boon::Loot => "предмет из добычи".into(),
    }
}

/// What failing the trial costs; a god in its dark stage takes more.
pub fn trial_price(g: &necromy_rules::Game, god: God) -> &'static str {
    let dark = g.stage(god) >= 2;
    match (god, dark) {
        (God::Bhava, false) => "яд дерева ×2",
        (God::Bhava, true) => "яд дерева ×3",
        (God::Trishna, false) => "−1 здоровья",
        (God::Trishna, true) => "−2 здоровья",
        (God::Zaga, _) => "скован на следующий ход",
        (God::Ahamar, false) => "−1 Стиля",
        (God::Ahamar, true) => "−2 Стиля",
        (God::Maya, _) => "весь Дух",
    }
}

/// A wager's bet said to the one it is on: "ты вступишь в бой".
pub fn bet_you(bet: necromy_rules::Bet) -> &'static str {
    use necromy_rules::Bet;
    match bet {
        Bet::Fight => "вступишь в бой",
        Bet::Claim => "займёшь поселение или храм",
        Bet::Fall => "падёшь",
        Bet::Hide => "скроешься",
    }
}

/// "оружие": a slot for items (§20.3).
pub fn slot(slot: necromy_rules::Slot) -> &'static str {
    use necromy_rules::Slot;
    match slot {
        Slot::Weapon => "оружие",
        Slot::Armour => "облачение",
        Slot::Relic => "реликвия",
    }
}

/// What an item does now, its number as its god's stage has it.
pub fn item_does(g: &necromy_rules::Game, item: necromy_rules::ItemId) -> String {
    use necromy_rules::{ItemEffect, When};
    let n = g.item_power(item);
    let def = item.def();
    match def.effect {
        ItemEffect::Dice(when) => {
            let when = match when {
                When::Always => " в бою",
                When::Attacking => ", когда нападаешь",
                When::Defending => " в защите",
                When::Day => " в бою днём",
                When::Night => " в бою ночью",
            };
            format!("+{n} кубик(а){when}")
        }
        ItemEffect::Shields => format!("+{n} щит(а) в каждом бою"),
        ItemEffect::Mend => format!("в начале хода +{n} здоровья"),
        ItemEffect::Wellspring => format!("в начале хода +{n} Духа"),
        ItemEffect::Hush => format!("в начале хода −{n} Угрозы"),
        ItemEffect::Hands => format!("+{n} к пределу руки"),
        ItemEffect::Stride => format!("+{n} очк. движения каждый ход"),
        ItemEffect::TrialDice => format!("+{n} кубик(а) в испытаниях"),
        ItemEffect::Shade => "ночью скрываешься на любой клетке".into(),
        ItemEffect::Ward => format!("в начале хода оберег ({})", element(def.element)),
    }
}

/// What a dark god takes each turn from whoever wears its item.
pub fn item_toll(god: God) -> &'static str {
    match god {
        God::Bhava => "яд дерева",
        God::Trishna => "−1 здоровья (не до смерти)",
        God::Zaga => "+1 Угрозы",
        God::Ahamar => "−1 Стиля",
        God::Maya => "−1 Духа",
    }
}

/// An item's tooltip: what it is, does, what breaks it, how its god bends it.
pub fn item_tip(
    g: &necromy_rules::Game,
    wearer: Option<necromy_rules::PlayerId>,
    item: necromy_rules::ItemId,
) -> String {
    let def = item.def();
    let god = God::from_index(def.element.index());
    let mut lines = vec![
        format!(
            "{} · {} · {}",
            def.name,
            slot(def.slot),
            element(def.element)
        ),
        item_does(g, item),
        format!(
            "ломает: {} (карта или грань стихии)",
            element(def.element.quenched_by())
        ),
    ];
    match g.stage(god) {
        0 if def.effect.scales() => lines.push(format!("{} светел: сильнее на 1", god_name(god))),
        2 => {
            let spared = wearer.is_some_and(|p| !g.item_tolls(p, item));
            if spared {
                lines.push(format!("{} тёмен, но щадит избранника", god_name(god)));
            } else {
                lines.push(format!(
                    "{} тёмен: каждый ход {}",
                    god_name(god),
                    item_toll(god)
                ));
            }
        }
        _ => {}
    }
    lines.join("\n")
}

fn god_name(god: God) -> &'static str {
    self::god(god)
}

/// What the militia think of a champion, in a word (§20.4).
pub fn standing(standing: i8) -> &'static str {
    match standing {
        s if s >= necromy_rules::FRIENDLY => "друзья: пропускают, лечат в поселениях",
        s if s >= necromy_rules::MILITIA_PASS => "пропускают в поселения",
        s if s <= necromy_rules::HOSTILE => "враги: только с боем, гонят из поселений",
        _ => "не пропускают: только с боем",
    }
}

/// The colour of the militia's pennant for a standing: green friends, gold
/// who they let through, red who has to fight them (`mobs_ui.rs`).
pub fn standing_color(standing: i8) -> bevy::prelude::Color {
    use bevy::prelude::Color;
    if standing >= necromy_rules::FRIENDLY {
        Color::srgb_u8(64, 150, 56)
    } else if standing >= necromy_rules::MILITIA_PASS {
        Color::srgb_u8(150, 112, 30)
    } else {
        Color::srgb_u8(176, 44, 36)
    }
}

/// Bhava's beasts by variant (`id` modulo the count, as their pictures
/// `sprites/beast-N.png`): nominative, accusative (§20.4).
const BEASTS: [(&str, &str); 6] = [
    ("Волк", "волка"),
    ("Вепрь", "вепря"),
    ("Медведь", "медведя"),
    ("Рысь", "рысь"),
    ("Чёрный волк", "чёрного волка"),
    ("Лесной лис", "лесного лиса"),
];

/// A mob's name, nominative and accusative (§20.4).
pub fn mob(kind: necromy_rules::MobKind, id: u32) -> (&'static str, &'static str) {
    match kind {
        necromy_rules::MobKind::Undead => ("Неупокоенный", "неупокоенного"),
        necromy_rules::MobKind::Beast { .. } => BEASTS[id as usize % BEASTS.len()],
    }
}
