//! Hear the gods write letters and tell cases on a live model:
//! `cargo run -p necromy-oracle --example voices` (NECROMY_ORACLE picks the
//! server). Bots play a match until a seat holds letters and a case, then
//! each is voiced.
use necromy_oracle::{client, prompt};
use necromy_rules::{Game, God, LineKind, Setup, bot};

fn main() {
    let (mut g, _) = Game::new(Setup {
        seed: 13,
        champions: God::ALL.to_vec(),
        mode: Default::default(),
    });
    let addr = necromy_oracle::addr_from_env();
    let mut heard = 0;
    let mut voiced: Vec<u32> = Vec::new();
    for _ in 0..4000 {
        if heard >= 8 || g.winner().is_some() {
            break;
        }
        let p = g.awaiting()[0];
        // Look before the bot takes its letters.
        let lines: Vec<_> = g
            .letters(p)
            .iter()
            .chain(
                g.lines_of(p)
                    .filter(|l| matches!(l.kind, LineKind::Case(_))),
            )
            .copied()
            .filter(|l| !voiced.contains(&l.id))
            .collect();
        for line in &lines {
            if heard >= 8 {
                break;
            }
            voiced.push(line.id);
            let messages = prompt::line_voice(&g, line);
            let t = std::time::Instant::now();
            let reply = client::chat(&addr, &messages, None, 140, 0.8);
            println!(
                "\n[{:?} → {:?}] {:?} {:?} fork {:?} chapter {} betrays {:?} ({:.1}s)\n  {}",
                line.god,
                g.champion(line.owner).map(|c| c.god),
                line.kind,
                line.goal,
                line.fork.map(|f| f.god),
                line.chapter,
                line.betrays,
                t.elapsed().as_secs_f32(),
                reply.map_or_else(|e| format!("error: {e}"), |r| prompt::tidy_voice(&r))
            );
            heard += 1;
        }
        let intent = bot::choose(&g, p);
        let _ = g.apply(p, intent);
    }
}
