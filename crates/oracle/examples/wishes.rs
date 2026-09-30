//! Try the wish prompt on a live model: `cargo run -p necromy-oracle --example wishes`
//! (NECROMY_ORACLE picks the server).
use necromy_oracle::{client, prompt};
use necromy_rules::{Game, God, PlayerId, Setup};

fn main() {
    let (g, _) = Game::new(Setup {
        seed: 3,
        champions: God::ALL.to_vec(),
        mode: Default::default(),
    });
    let me = PlayerId(1);
    let addr = necromy_oracle::addr_from_env();
    let hand: Vec<&str> = g.hand(me).iter().map(|&c| g.def(c).name).collect();
    println!("hand: {hand:?}");
    let asks = [
        (God::Trishna, "Тришна, пусть лес вокруг Бхавы вспыхнет"),
        (
            God::Maya,
            "Майя, пусть вода поднимется и отрежет Ахамара от всех",
        ),
        (God::Maya, "Майя, пусть река пройдёт у ног Заги"),
        (God::Zaga, "Зага, пусть яд тихо войдёт в кровь Майи"),
        (God::Bhava, "Бхава, дай мне зверя, чтобы шёл рядом"),
        (God::Bhava, "Бхава, пусть волки выйдут на Ахамара"),
        (God::Maya, "Майя, подними мертвеца рядом с Бхавой"),
        (
            God::Ahamar,
            "Ахамар, отдаю свой клинок: пусть стража придёт за Майей",
        ),
        (God::Ahamar, "Ахамар, пусть Зага станет мне должна"),
        (God::Ahamar, "Ахамар, построй в моём поселении кузню"),
        (God::Ahamar, "Ахамар, пусть ближний князь поклонится мне"),
        (
            God::Trishna,
            "Тришна, пусть поля уродят и мой народ наестся",
        ),
        (
            God::Trishna,
            "Тришна, устрой у меня ярмарку, чтобы шумела до утра",
        ),
        (God::Bhava, "Бхава, разбуди рощу, пусть лес пойдёт к Столу"),
        (
            God::Zaga,
            "Зага, возьми мою ношу и пусть мёртвые лягут вокруг Тришны",
        ),
        (God::Ahamar, "Ахамар, проложи мне дорогу к храму"),
    ];
    for (god, text) in asks {
        let text = text.replace("{card}", hand.first().copied().unwrap_or("?"));
        let (messages, schema) = prompt::wish(&g, me, god, &text);
        let t = std::time::Instant::now();
        let reply = client::chat(&addr, &messages, Some(&schema), 600, 0.4);
        let secs = t.elapsed().as_secs_f32();
        match reply.and_then(|r| prompt::read_wish(&g, me, god, &text, &r)) {
            Ok((wish, said)) => println!(
                "\n[{god:?}] {text}\n  {secs:.1}s acts={:?} price={:?} grade={}\n  {} // {}",
                wish.acts, wish.price, said.grade, said.speech, said.reason
            ),
            Err(e) => println!("\n[{god:?}] {text}\n  error: {e}"),
        }
    }
}
