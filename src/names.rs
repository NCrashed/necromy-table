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
        TasteKind::Land => "Земля: владения стоят вдвое",
        TasteKind::Stage => "Сцена: земля почти ничего не стоит, характер вдвое",
        TasteKind::Arena => "Арена: победы стоят вдвое, храмы ничего",
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
        Terrain::Settlement => ("Поселение", "войди — займёшь; на рассвете даёт Стиль"),
        Terrain::Temple => (
            "Храм",
            "займи ради Стиля; конец хода здесь — молитва богу края",
        ),
        Terrain::Ruins => ("Руины", "пока без особых свойств"),
        Terrain::Stones => ("Камни силы", "пока без особых свойств"),
        Terrain::Grove => ("Роща", "выросла из нетронутого тела; идти дороже"),
        Terrain::Table => ("Стол Ахамара", "центр; займи — больше всего Стиля"),
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

/// Name and explanation of a victory condition (§10).
pub fn condition(c: necromy_rules::Condition) -> (String, String) {
    use necromy_rules::Condition;
    match c {
        Condition::Registry { regions } => (
            "Реестр".into(),
            format!("Держи поселение или храм в {regions} краях из пяти."),
        ),
        Condition::GodLimit => (
            "Предел бога".into(),
            "Служи одному богу фанатично (благосклонность к нему 8+, вектор длиной 0,6+), пока он в тёмной стадии.".into(),
        ),
        Condition::Fusion => (
            "Слияние".into(),
            "Служи двум соседним по кругу богам (к каждому 8+, вместе 60% твоей благосклонности), и ни один не в светлой стадии.".into(),
        ),
        Condition::MiddlePath { dawns } => (
            "Срединный путь".into(),
            format!("Благосклонность 10+ поровну у центра пентаграммы, ни один бог не тёмный, {dawns} рассвета подряд."),
        ),
        Condition::Overthrow { wins } => (
            "Свержение".into(),
            format!("Победи Доминирующего в бою {wins} раза."),
        ),
        Condition::FirstAtTable { round } => (
            "Первый на столе".into(),
            format!("Начиная с раунда {round}, будь единственным лидером по Стилю."),
        ),
        Condition::Wager { refusals } => (
            "Пари Ахамара".into(),
            format!("Будучи Доминирующим, откажись от желания {refusals} рассвета подряд: выигрывай стол и не бери его плату. Каждый отказ — +2 Угрозы; загаданное желание, рассвет без Венца или смерть обнуляют счёт."),
        ),
    }
}

pub fn check(kind: necromy_rules::CheckKind) -> &'static str {
    use necromy_rules::CheckKind;
    match kind {
        CheckKind::RegionsHeld => "края с владениями",
        CheckKind::TopFavor => "благосклонность к главному богу",
        CheckKind::SecondFavor => "к слабейшему из пары",
        CheckKind::PairShare => "доля пары, %",
        CheckKind::TotalFavor => "вся благосклонность",
        CheckKind::Fanaticism => "фанатизм, %",
        CheckKind::Balance => "близость к центру, %",
        CheckKind::GodStage => "стадия главного бога",
        CheckKind::PairNotLight => "боги пары не в свете",
        CheckKind::GodsNotDark => "боги не во тьме",
        CheckKind::Streak => "рассветов подряд",
        CheckKind::Overthrows => "побед над Доминирующим",
        CheckKind::Round => "раунд",
        CheckKind::StyleLead => "единственный лидер по Стилю",
        CheckKind::Refusals => "отказов от желания подряд",
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
