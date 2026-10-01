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
        StyleReason::Feast => "пир",
        StyleReason::Fair => "торг на ярмарке",
        StyleReason::Path => "шаг к деянию",
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
        Novelty::Built => "строит на поселении".into(),
        Novelty::Tamed => "приручает зверя".into(),
        Novelty::Enlisted => "вписывает мертвеца в легион".into(),
        Novelty::Paved => "мостит дорогу".into(),
        Novelty::Kindled => "поджигает".into(),
        Novelty::Doused => "тушит пожар".into(),
        Novelty::Sowed => "засевает поле".into(),
        Novelty::Feasted => "устраивает пир".into(),
        Novelty::HeldFair => "открывает ярмарку".into(),
        Novelty::Gifted => "одаривает правителя".into(),
        Novelty::Consecrated => "освящает кладбище".into(),
        Novelty::Buried => "хоронит тело".into(),
        Novelty::SlewMonster => "сражает чудовище".into(),
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
        Terrain::River => (
            "Река",
            "переправа кончает ход; вдоль реки шаг за 1; в реке могут быть пираньи",
        ),
        Terrain::Lake => ("Озеро", "стоячая вода: не пройти; отрезает землю, как мгла"),
        Terrain::Ash => (
            "Пепелище",
            "здесь прошёл огонь; тело на нём может прорасти рощей",
        ),
        Terrain::Fields => (
            "Поле",
            "на закате рождает еду; еду носят ношей в своё поселение",
        ),
        Terrain::Graveyard => (
            "Кладбище",
            "тело, положенное или оставленное здесь, не встаёт мертвецом",
        ),
        Terrain::Pit => (
            "Чумная яма",
            "тела гниют вместе; кто кончит ход рядом — отравлен",
        ),
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
        GreatDeed::City => (
            "Город",
            format!(
                "{} поселений бок о бок, все твои, а среди них таверна, кузня и святилище.",
                necromy_rules::CITY
            ),
        ),
        GreatDeed::River => (
            "Река",
            format!(
                "Река в {} клеток и больше: исток у гор, устье на краю мира — два заката подряд.",
                necromy_rules::RIVER
            ),
        ),
        GreatDeed::FloodedTable => (
            "Затопленный Стол",
            "Стол Ахамара и шесть клеток вокруг под озером — два заката подряд.".into(),
        ),
        GreatDeed::Amazon => (
            "Амазонка",
            format!(
                "Лес в {} клеток, сквозь который течёт река (не меньше {} клеток), и в реках пираньи — два заката подряд.",
                necromy_rules::JUNGLE,
                necromy_rules::JUNGLE_RIVER
            ),
        ),
        GreatDeed::Roads => (
            "Дороги реестра",
            "Одна сеть дорог связывает Стол и все пять храмов.".into(),
        ),
        GreatDeed::GreatFire => (
            "Великий пожар",
            format!(
                "Огонь, начатый тобой, прошёл по {} краям из пяти — и ещё горит.",
                necromy_rules::GREAT_FIRE
            ),
        ),
        GreatDeed::Treasury => (
            "Сокровищница",
            format!(
                "Под руинами открой ход вниз, пророй {} туннеля к залу, сделай в нём сокровищницу с охраной — и её не возьмут {} заката.",
                necromy_rules::HALL_DEPTH,
                necromy_rules::TREASURY_DUSKS
            ),
        ),
        GreatDeed::DeadBall => (
            "Бал мёртвых",
            "Ночной пир в твоём поселении: рядом сидят неупокоенные, ополчение и два соперника — и до рассвета никто не дерётся.".into(),
        ),
        GreatDeed::Arena => (
            "Арена",
            format!(
                "Построй арену и вызывай на дуэль: {} выигранные дуэли (неявка вызванного — тоже победа).",
                necromy_rules::ARENA_WINS
            ),
        ),
        GreatDeed::DebtBondage => (
            "Долговое рабство",
            format!(
                "Одновременно {} соперника у тебя в долгу по проигранным пари.",
                necromy_rules::DEBTORS
            ),
        ),
        GreatDeed::WalkingForest => (
            "Шагающий лес",
            "Разбуди рощу картой «Дикий энт»: каждую ночь она шагает к Столу Ахамара и, дойдя, укореняется у него.".into(),
        ),
        GreatDeed::Ark => (
            "Ковчег",
            format!(
                "Загон у своего святилища, и в нём на привязи по зверю каждой из {} стихий.",
                necromy_rules::ARK
            ),
        ),
        GreatDeed::Summoning => (
            "Призыв",
            format!(
                "Круг на камнях силы, напоенный {} телами, открывает врата; твой удар добивает чудовище.",
                necromy_rules::SUMMON_BODIES
            ),
        ),
        GreatDeed::Dragon => (
            "Дракон",
            format!(
                "Добудь яйцо в испытании в горах и высиживай его в огне {} раза: вылупившийся дракон — твой спутник.",
                necromy_rules::EGG_WARMTH
            ),
        ),
        GreatDeed::Guest => (
            "Гость из иного мира",
            "Из-за мглы выходит чужак; проведи его живым к Столу Ахамара.".into(),
        ),
        GreatDeed::Necropolis => (
            "Некрополь",
            format!(
                "Кладбище из {} клеток подряд, на нём погребено {} тел, и на закате в краю Заги ни одного мертвеца.",
                necromy_rules::NECROPOLIS,
                necromy_rules::NECROPOLIS_BODIES
            ),
        ),
        GreatDeed::PlaguePit => (
            "Чумная яма",
            format!(
                "Вырой яму, брось в неё {} тел и реши их участь: упокоить или поднять.",
                necromy_rules::PIT_BODIES
            ),
        ),
        GreatDeed::TripleUnion => (
            "Тройная уния",
            format!(
                "Дарами склони правителей и пожени их: твои браки связывают {} края в один род.",
                necromy_rules::UNION_LANDS
            ),
        ),
        GreatDeed::FallenEmpire => (
            "Падшая империя",
            format!(
                "{} правителя присягают тебе, ты коронуешься на Столе, а потом сеешь раздор: {} бывших вассала в распре.",
                necromy_rules::CROWN_VASSALS,
                necromy_rules::FEUDING
            ),
        ),
        GreatDeed::FairOfFive => (
            "Ярмарка пяти краёв",
            "На твоей ярмарке продан товар каждого из пяти краёв: древесина, вино, соль, железо и жемчуг.".into(),
        ),
        GreatDeed::DeadFeast => (
            "Пир для мёртвых",
            format!(
                "Мертвецы пришли на твою ярмарку и съели пир — {} раза.",
                necromy_rules::DEAD_FEASTS
            ),
        ),
        GreatDeed::Feast => (
            "Пир на весь край",
            format!(
                "{} поля у твоих поселений и твой пир ({} еды), на который пришли хотя бы {} гостя.",
                necromy_rules::FEAST_FIELDS,
                necromy_rules::FEAST_FOOD,
                necromy_rules::FEAST_GUESTS
            ),
        ),
        GreatDeed::Legion => (
            "Легион",
            format!(
                "{} мертвецов твоего легиона идут за тобой — два заката подряд.",
                necromy_rules::LEGION
            ),
        ),
        GreatDeed::Reconciliation => (
            "Примирение",
            "Два бога, один из которых гасит другого, оба в свете, и твоё святилище обоих на стыке их земель — два заката подряд.".into(),
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
        CheckKind::CitySize => "поселений в твоём городе",
        CheckKind::CityHas => "таверна, кузня, святилище",
        CheckKind::PairLight => "боги пары в свете",
        CheckKind::SharedShrine => "святилище обоих",
        CheckKind::LegionSize => "мертвецов в твоём легионе",
        CheckKind::TemplesLinked => "храмов на дорогах реестра",
        CheckKind::RegionsBurnt => "краёв прошёл твой огонь",
        CheckKind::FireBurning => "твой огонь ещё горит",
        CheckKind::FieldsOwned => "полей у твоих поселений",
        CheckKind::FeastHeld => "пир с гостями",
        CheckKind::FairGoods => "товаров продано на твоей ярмарке",
        CheckKind::DeadFeasts => "твоих ярмарок съели мертвецы",
        CheckKind::UnionLands => "краёв связано твоими браками",
        CheckKind::NecropolisSize => "клеток кладбища подряд",
        CheckKind::NecropolisBodies => "погребено на нём",
        CheckKind::ZagaQuiet => "в краю Заги нет мертвецов",
        CheckKind::PitSettled => "твоя яма упокоена или поднята",
        CheckKind::Summoned => "твоё чудовище пало от твоей руки",
        CheckKind::DragonFollows => "за тобой идёт дракон",
        CheckKind::GuestHome => "гость доведён до Стола",
        CheckKind::GroveRooted => "твоя роща укоренилась у Стола",
        CheckKind::ArenaWins => "дуэлей выиграно на твоей арене",
        CheckKind::Debtors => "соперников у тебя в долгу",
        CheckKind::BallKept => "бал мёртвых прошёл без драки",
        CheckKind::TreasuryHeld => "закатов твоя сокровищница цела",
        CheckKind::PenElements => "стихий зверей в твоём загоне",
        CheckKind::PenByShrine => "твоё святилище рядом",
        CheckKind::Crowned => "коронация на Столе",
        CheckKind::Feuding => "бывших вассалов в распре",
        CheckKind::RiverLength => "клеток самой длинной реки",
        CheckKind::RiverSource => "исток у гор",
        CheckKind::RiverMouth => "устье на краю мира",
        CheckKind::TableFlooded => "клеток Стола под водой",
        CheckKind::JungleWoods => "клеток леса у реки",
        CheckKind::JungleRiver => "клеток реки в лесу",
        CheckKind::Piranhas => "пираньи в реках",
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
        WishKind::River => "Пусть потечёт река",
        WishKind::Flood => "Пусть поднимутся воды",
        WishKind::Road => "Проложи дорогу реестра",
        WishKind::Fire => "Подожги лес",
        WishKind::Awaken => "Принеси в мир новое",
        WishKind::Treasure => "Дай мне сокровище",
        WishKind::Ordeal => "Испытай меня",
        WishKind::Poison => "Отрави соперника",
        WishKind::Beast => "Пошли мне зверя",
        WishKind::Undead => "Подними мертвеца на соперника",
        WishKind::Guard => "Натрави на соперника гвардию",
        WishKind::Debt => "Пусть соперник будет мне должен",
        WishKind::Build => "Построй мне дом в моём поселении",
        WishKind::Sway => "Пусть правитель склонится ко мне",
        WishKind::Harvest => "Пусть поля уродят",
        WishKind::Fair => "Пусть у меня будет ярмарка",
        WishKind::WakeGrove => "Пусть лес пойдёт",
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
        Feature::Cargo => (
            "Ноша",
            "тело можно взять и нести, на шаг медленнее; победитель забирает ношу побеждённого",
        ),
        Feature::Buildings => (
            "Постройки",
            "на своём поселении можно построить таверну, кузню, святилище или стену",
        ),
        Feature::City => (
            "Города",
            "поселение прирастает кварталом; поселения бок о бок — один город",
        ),
        Feature::Companions => (
            "Спутники",
            "зверя рядом можно приручить за Дух; спутник идёт за чемпионом и добавляет кость в бою",
        ),
        Feature::Legion => (
            "Легион",
            "мертвеца рядом можно вписать в свой легион; победитель в бою уводит спутника",
        ),
        Feature::Rivers => (
            "Реки",
            "реки текут от гор к краю мира; переправа кончает ход, вдоль реки идти легко",
        ),
        Feature::Lakes => (
            "Озёра",
            "вода разливается озёрами: их не пройти, они отрезают землю",
        ),
        Feature::Piranhas => (
            "Пираньи",
            "в реках пираньи: кусают входящего, но не насмерть",
        ),
        Feature::Roads => (
            "Дороги",
            "шаг по дороге стоит 1 на любой земле, дорога через реку — мост; на дороге не скрыться",
        ),
        Feature::Underworld => (
            "Подземный мир",
            "под руинами можно открыть ход вниз, прорыть туннели к залу и спрятать там сокровищницу",
        ),
        Feature::Arena => (
            "Арены",
            "на поселении можно построить арену и вызывать соперников на дуэль; неявка — поражение",
        ),
        Feature::Debts => (
            "Долги",
            "можно поспорить с соперником, что он что-то сделает до заката; проигравший в долгу",
        ),
        Feature::WalkingGroves => (
            "Бродячие рощи",
            "карта «Дикий энт» будит рощу: каждую ночь она шагает к Столу, оставляя лес",
        ),
        Feature::Wilds => (
            "Звери стихий",
            "звери выходят из лесов любого края, каждый — стихии своего края",
        ),
        Feature::Pens => (
            "Загоны",
            "на поселении можно поставить загон и привязать там прирученного зверя; чужого можно увести",
        ),
        Feature::Ritual => (
            "Ритуал",
            "на камнях силы чертят круг и кормят его телами; сытый круг открывает врата",
        ),
        Feature::Monsters => (
            "Чудовища",
            "из врат выходит чудовище: сильное, идёт к живым; павшее даёт Стиль и добычу",
        ),
        Feature::Dragons => (
            "Драконы",
            "испытание в горах может дать яйцо; в огне оно греется и вылупляется драконом",
        ),
        Feature::Guests => (
            "Гости",
            "из-за мглы выходит чужак: его можно повести за собой к Столу",
        ),
        Feature::Burial => (
            "Погребение",
            "клетку можно освятить под кладбище или вырыть чумную яму; погребённые не встают",
        ),
        Feature::Rulers => (
            "Правители",
            "у поселений правители: дар склоняет их, присяга делает вассалом, браки связывают края",
        ),
        Feature::Goods => (
            "Товары",
            "занятые поселения на закате дают товар своего края; товар носят ношей, его отнимают в бою",
        ),
        Feature::Fairs => (
            "Ярмарки",
            "в своём поселении можно открыть ярмарку: привезённый товар даёт Стиль и карту, а мертвецы идут на шум",
        ),
        Feature::Fields => (
            "Поля",
            "равнину у своего поселения можно засеять; на закате поле рождает еду, из еды — пир",
        ),
        Feature::Fires => (
            "Пожары",
            "лес, рощи и поселения горят; огонь перекидывается на соседей и оставляет пепел",
        ),
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
        Price::Cargo => "свою ношу".into(),
        Price::Companion => "своего спутника".into(),
        Price::Item(_) => "свою вещь".into(),
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
        LineKind::Bring => "Недостающее",
        LineKind::Thwart => "Сорвать деяние",
        LineKind::Invitation => "Приглашение на пир",
        LineKind::Errand => "Просьба",
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
        LineKind::Bring => "Твоему замыслу не хватает того, чего нет в мире. Принеси это.",
        LineKind::Thwart => "Чужое деяние вот-вот свершится. Не дай ему дожить до заката.",
        LineKind::Invitation => "В поселении готовят пир. Приходи — накормят и почтут.",
        LineKind::Errand => "Сделай то, о чём прошу, — и я тебя запомню.",
    }
}

/// What a line asks, with progress where it has one, and the other way
/// to close it, if it has one.
pub fn line_goal(line: &necromy_rules::Line, g: &necromy_rules::Game) -> String {
    let main = goal_text(line.goal, line, g);
    match line.fork {
        Some(f) => format!(
            "{main} · или: {} (для {})",
            goal_text(f.goal, line, g),
            god_genitive(f.god)
        ),
        None => main,
    }
}

fn goal_text(
    goal: necromy_rules::Goal,
    line: &necromy_rules::Line,
    g: &necromy_rules::Game,
) -> String {
    use necromy_rules::Goal;
    match goal {
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
        Goal::Bring(f) => format!(
            "пусть в мире будет: {} (желанием или чьим угодно)",
            feature(f).0.to_lowercase()
        ),
        Goal::Thwart(rival) => format!(
            "сорви деяние {} до заката",
            g.champion(rival).map_or("?", |c| god_genitive(c.god))
        ),
        Goal::Do(d) => doing(d).into(),
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
        WorldStir::Awakening => "На столе тихо — и самый тёмный бог сам приносит в мир своё.",
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
        Sentence => {
            "гвардия выходит уже при Угрозе 3 и бьёт на кубик больше; яд заразен: укус передаёт стак соседям без яда"
        }
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
        necromy_rules::MobKind::Monster { .. } => ("Чудовище", "чудовище"),
        necromy_rules::MobKind::Guest => ("Гость из-за мглы", "гостя из-за мглы"),
    }
}

/// A burden a champion carries (§21.8).
pub fn cargo(c: necromy_rules::Cargo) -> &'static str {
    use necromy_rules::Cargo;
    match c {
        Cargo::Body { hero: true, .. } => "тело чемпиона",
        Cargo::Body { hero: false, .. } => "тело",
        Cargo::Food => "еда",
        Cargo::Goods(g) => goods(g),
        Cargo::Egg { .. } => "драконье яйцо",
    }
}

/// A building on a settlement (§21.8), for the feed and the buttons.
pub fn building(b: necromy_rules::Building) -> String {
    use necromy_rules::Building;
    match b {
        Building::Tavern => "таверна".into(),
        Building::Forge => "кузня".into(),
        Building::Wall => "стена".into(),
        Building::Pen => "загон".into(),
        Building::Arena => "арена".into(),
        Building::Shrine([a, b]) if a == b => format!("святилище {}", god_genitive(a)),
        Building::Shrine([a, b]) => {
            format!("святилище {} и {}", god_genitive(a), god_genitive(b))
        }
    }
}

/// A companion (§21.8): nominative, for the feed and the sheet.
pub fn companion(c: necromy_rules::Companion) -> &'static str {
    use necromy_rules::Companion;
    match c {
        Companion::Beast(_) => "зверь",
        Companion::Undead => "мертвец",
        Companion::Dragon => "дракон",
        Companion::Guest => "гость",
    }
}

/// The goods of a god's land (§21.8).
pub fn goods(god: necromy_rules::God) -> &'static str {
    use necromy_rules::God;
    match god {
        God::Bhava => "древесина",
        God::Trishna => "вино",
        God::Zaga => "соль",
        God::Ahamar => "железо",
        God::Maya => "жемчуг",
    }
}

/// What a champion does at a way down (§21.8), for the buttons.
pub fn delve_work(w: necromy_rules::DelveWork) -> &'static str {
    use necromy_rules::DelveWork;
    match w {
        DelveWork::Open => "Открыть ход\nвниз",
        DelveWork::Dig => "Рыть\nтуннель",
        DelveWork::Treasury => "Сделать\nсокровищницу",
        DelveWork::Guard => "Оставить\nспутника в охране",
        DelveWork::Raid => "Набег на\nсокровищницу",
    }
}

/// A law's second rule, over a mechanic of its god's domain (§21.8), and
/// the mechanic it needs.
pub fn law_world_text(law: necromy_rules::Law) -> (necromy_rules::Feature, &'static str) {
    use necromy_rules::Feature;
    use necromy_rules::Law::*;
    match law {
        Sprout => (
            Feature::WalkingGroves,
            "бродячие рощи шагают дважды за ночь",
        ),
        Thicket => (
            Feature::Companions,
            "звери доверчивы: приручить стоит на Дух меньше",
        ),
        Wildgrowth => (Feature::Fields, "на закате поле в его краю дичает в лес"),
        Generosity => (Feature::Fields, "пир нужен уже из трёх еды"),
        Thirst => (Feature::Fairs, "товар на ярмарке приносит ещё и Дух"),
        Devouring => (
            Feature::Fires,
            "огонь перекидывается всегда; на закате в её краю вспыхивает пожар",
        ),
        Stillness => (
            Feature::Burial,
            "погребение — подношение Заге и −1 Угрозы хоронящему",
        ),
        Burden => (Feature::Cargo, "ноша отнимает два шага, а не один"),
        Sentence => (Feature::Burial, "чумная яма травит вдвое"),
        Mask => (
            Feature::Rulers,
            "у кого есть присягнувшие правители, тому +1 Стиль на рассвете",
        ),
        Crack => (Feature::Debts, "неотданные долги на закате растут на 1"),
        Exposure => (Feature::Arena, "неявившийся на дуэль получает +2 Угрозы"),
        Rest => (
            Feature::Rivers,
            "на закате река, не дошедшая до края мира, течёт дальше на клетку",
        ),
        Manipulation => (Feature::Rivers, "скрытые переходят реку за один шаг"),
        Wrath => (
            Feature::Lakes,
            "на закате самое малое озеро разливается на клетку, до 7",
        ),
    }
}

/// A law in words, with its second rule where the world has its mechanic.
pub fn law_line(law: necromy_rules::Law, game: &necromy_rules::Game) -> String {
    let (feature, more) = law_world_text(law);
    if game.has(feature) {
        format!("{}; {more}", law_text(law))
    } else {
        law_text(law).to_string()
    }
}

/// What a god did to the world by itself at dusk (§21.6).
pub fn god_act(act: necromy_rules::GodAct) -> &'static str {
    use necromy_rules::GodAct::*;
    match act {
        Grove => "поднялась роща",
        Woods => "равнина зарастает лесом",
        Beast => "из чащи вышел зверь",
        Settlement => "поселились люди",
        Goods => "поселения дали товар",
        Fire => "вспыхнул пожар",
        Stones => "встали камни силы",
        Mountains => "поднялись горы",
        Dam => "река заилилась в болото, рядом легло тело",
        Road => "пролегла дорога реестра",
        Trial => "поставлено испытание",
        Judgement => "реестр записал: +2 Угрозы",
        Unveil => "мгла отступила",
        River => "река потекла дальше",
        Flood => "поднялись воды",
        Land => "земля бога разрослась",
    }
}

/// The next step of the human's deed (the path, `necromy_rules::path`) in
/// words: what to do, plainly, then a short line of why.
pub fn step(
    g: &necromy_rules::Game,
    me: necromy_rules::PlayerId,
    step: &necromy_rules::path::Step,
) -> (String, String) {
    use necromy_rules::path::{Errand, StepWhat, Why};
    use necromy_rules::{Intent, Target};
    let here = g.champion(me).map(|c| c.hex);
    let far = match (step.at, here) {
        (Some(at), Some(h)) if at != h => format!(" · {} кл.", at.unsigned_distance_to(h)),
        _ => String::new(),
    };
    let why = step
        .check
        .map(|c| format!("ради: {}", check(c).to_lowercase()))
        .unwrap_or_default();
    let what = match &step.what {
        StepWhat::Hold => {
            return (
                "Деяние готово: удержи его до заката".into(),
                "соперники попытаются сорвать канун".into(),
            );
        }
        StepWhat::Spirit(n) => format!("Накопи Дух: нужно {n} (копится за ход)"),
        StepWhat::Wait(why) => match why {
            Why::Dusk => "Жди заката: мир сдвинется сам".into(),
            Why::Goods => "Жди заката: товар появится в поселениях".into(),
            Why::Duel => "Жди на арене: вызванный должен прийти до заката".into(),
            Why::Bets => "Жди заката: пари рассчитаются".into(),
        },
        StepWhat::Wish { god: whom, act } => format!(
            "На закате попроси {}: «{}»",
            god_accusative(*whom),
            wish(act.kind()).to_lowercase()
        ),
        StepWhat::Go(errand) => match errand {
            Errand::Dissolve => "Иди в край, что растворяешь, и зови туда мглу".into(),
            Errand::HeroGrove => "Иди к роще из тела чемпиона и береги её".into(),
            Errand::RiverEnd => "Иди к концу реки: продли её к краю мира".into(),
            Errand::Spring => "Иди к Столу: пусть вода пойдёт к нему".into(),
            Errand::Stones => "Иди к камням силы: начерти там круг".into(),
            Errand::Body => "Иди к телу и подними его".into(),
            Errand::Circle => "Неси тело в свой круг".into(),
            Errand::Monster => "Иди к своему чудовищу и сразись с ним".into(),
            Errand::MountainTrial => "Иди к испытанию на горе: награда — яйцо".into(),
            Errand::Nest => "Неси яйцо в лес и положи там".into(),
            Errand::EggFire => "Встань у яйца: подожги лес вокруг".into(),
            Errand::Egg => "Подними своё яйцо снова".into(),
            Errand::BuildSite(b) => format!(
                "Иди в своё поселение: построй {}",
                building(*b).to_lowercase()
            ),
            Errand::Arena => "Иди на свою арену".into(),
            Errand::Delve => "Иди к своему ходу под землю".into(),
            Errand::Ruins => "Иди к руинам: открой ход вниз".into(),
            Errand::Grove => "Иди к роще: разбуди её".into(),
            Errand::Pen => "Веди зверя в свой загон и привяжи".into(),
            Errand::Beast => "Иди к зверю нужной стихии и приручи".into(),
            Errand::Guest => "Иди к гостю из-за мглы: он пойдёт за тобой".into(),
            Errand::Table => "Иди к Столу".into(),
            Errand::Consecrate => "Иди к кладбищу: освяти землю рядом".into(),
            Errand::Bury => "Неси тело на кладбище и положи".into(),
            Errand::Pit => "Иди к своей чумной яме".into(),
            Errand::Undead => "Упокой мертвеца в краю Заги".into(),
            Errand::Enlist => "Иди к мертвецу: возьми его в легион".into(),
            Errand::Ruler => "Иди к правителю: поднеси дар".into(),
            Errand::Match => "Иди к правителю, что к тебе расположен: сосватай".into(),
            Errand::Vassal => "Иди к вассалу: посей раздор".into(),
            Errand::Settlement => "Займи поселение".into(),
            Errand::FairSite => "Иди в своё поселение: открой ярмарку".into(),
            Errand::Fair => "Неси товар на свою ярмарку".into(),
            Errand::Goods => "Подними товар, которого нет на ярмарке".into(),
            Errand::GoodsTown => {
                "Иди к поселению края, чьего товара нет: он появится на закате".into()
            }
            Errand::Hall => "Иди в свой зал: жди гостей на пир".into(),
            Errand::Store => "Неси еду в своё поселение".into(),
            Errand::Field => "Иди к равнине у своего поселения: засей".into(),
            Errand::Food => "Подними урожай с поля".into(),
            Errand::Burn => "Иди к лесу края, где ещё не горело: подожги".into(),
            Errand::Border => {
                "Иди в поселение у двух враждующих земель: построй святилище обоих".into()
            }
            Errand::BorderLand => "Иди туда, где сходятся враждующие земли".into(),
            Errand::Prey => "Иди к ослабшему сопернику: тело чемпиона вырастит рощу".into(),
        },
        StepWhat::Do(intent) => match intent {
            Intent::Build { building: b } => {
                format!("Построй здесь: {}", building(*b).to_lowercase())
            }
            Intent::Quarter { .. } => "Построй новый квартал".into(),
            Intent::Recruit { .. } => "Возьми его к себе".into(),
            Intent::Sow => "Засей поле здесь".into(),
            Intent::Feast => "Устрой пир".into(),
            Intent::Fair => "Открой здесь ярмарку".into(),
            Intent::DrawCircle => "Начерти здесь круг".into(),
            Intent::Delve { work } => delve_work(*work).replace('\n', " "),
            Intent::Challenge { .. } => "Вызови соперника на поединок".into(),
            Intent::BetOn { .. } => "Заключи пари на соперника".into(),
            Intent::Tether { .. } => "Привяжи зверя в загоне".into(),
            Intent::Consecrate => "Освяти здесь кладбище".into(),
            Intent::DigPit => "Вырой здесь чумную яму".into(),
            Intent::SettlePit { .. } => "Упокой яму".into(),
            Intent::Gift { .. } => "Поднеси дар правителю".into(),
            Intent::Betroth { .. } => "Сосватай правителей".into(),
            Intent::Coronation => "Коронуйся на Столе".into(),
            Intent::Discord => "Посей раздор между вассалами".into(),
            Intent::Kindle { .. } => "Подожги".into(),
            Intent::Douse { .. } => "Потуши огонь".into(),
            Intent::Take => "Подними ношу".into(),
            Intent::Lay => "Положи ношу здесь".into(),
            Intent::Pave => "Замости дорогу".into(),
            Intent::Play { card, target } => {
                let on = match target {
                    Target::Hex(_) => " на клетку",
                    Target::Champion(_) => " на чемпиона",
                    Target::None => "",
                };
                format!("Сыграй «{}»{on}", g.card_name(*card))
            }
            _ => "Сделай ход".into(),
        },
    };
    (format!("{what}{far}"), why)
}

/// What a letter's line asks to be done (docs/storyteller-plan.md).
pub fn doing(d: necromy_rules::Doing) -> &'static str {
    use necromy_rules::Doing;
    match d {
        Doing::Build => "построй что-нибудь в своём поселении",
        Doing::Kindle => "подожги лес или чужое поселение",
        Doing::Bury => "похорони тело на кладбище",
        Doing::Gift => "поднеси дар правителю",
        Doing::Tame => "приручи зверя или возьми мертвеца в легион",
        Doing::Sell => "продай товар на ярмарке",
        Doing::OpenFair => "открой ярмарку в своём поселении",
        Doing::Rebuild => "отстрой поселение из руин",
        Doing::FellMob => "срази зверя, мертвеца или чудовище",
        Doing::Hide => "скройся из виду",
        Doing::StoreFood => "отнеси еду в закрома своего поселения",
        Doing::Seed => "засей тело: сыграй на него карту «семя»",
        Doing::Fuel => "сожги тело: сыграй на него карту «топливо»",
        Doing::Rest => "упокой тело картой покоя",
        Doing::Dissolve => "растворь тело картой Майи",
        Doing::Raise => "подними тело в легион картой",
    }
}
