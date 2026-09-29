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
        (
            God::Bhava,
            "Бхава, пусть всё древесное в колоде нальётся соком",
        ),
        (
            God::Maya,
            "Майя, пусть огненные карты в колоде размокнут и ослабнут",
        ),
        (God::Zaga, "Зага, отрави колоду"),
        (
            God::Trishna,
            "Тришна, спрячь в колоде угощение с ядом для того, кто его вытянет",
        ),
        (God::Ahamar, "Ахамар, покажи мне, что придёт из колоды"),
        (
            God::Maya,
            "Майя, пусть я окажусь там, где стоит Зага, а она — на моём месте",
        ),
    ];
    for (god, text) in asks {
        let text = text.replace("{card}", hand.first().copied().unwrap_or("?"));
        let (messages, schema) = prompt::wish(&g, me, god, &text);
        let t = std::time::Instant::now();
        let reply = client::chat(&addr, &messages, Some(&schema), 600, 0.4);
        let secs = t.elapsed().as_secs_f32();
        match reply.and_then(|r| prompt::read_wish(&g, me, &text, &r)) {
            Ok((wish, said)) => println!(
                "\n[{god:?}] {text}\n  {secs:.1}s acts={:?} price={:?} grade={}\n  {} // {}",
                wish.acts, wish.price, said.grade, said.speech, said.reason
            ),
            Err(e) => println!("\n[{god:?}] {text}\n  error: {e}"),
        }
    }
}
