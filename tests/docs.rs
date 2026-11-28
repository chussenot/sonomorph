//! The scores in the documentation are scores the engines render: every
//! ```` ```abc ```` block in `docs/` is rendered by chip8 here, and every
//! ```` ```score ```` block by the studio when `render-advanced` is on.

use sonomorph::chip8::render_file;

fn blocks(page: &str, fence: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut current: Option<String> = None;
    for line in page.lines() {
        match (&mut current, line.trim_end()) {
            (None, open) if open == fence => current = Some(String::new()),
            (Some(block), "```") => {
                blocks.push(std::mem::take(block));
                current = None;
            }
            (Some(block), text) => {
                block.push_str(text);
                block.push('\n');
            }
            (None, _) => {}
        }
    }
    blocks
}

#[test]
fn every_score_in_the_docs_renders() {
    let docs = concat!(env!("CARGO_MANIFEST_DIR"), "/docs");
    let dir = std::env::temp_dir().join(format!("sonomorph-docs-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut rendered = 0;
    for entry in std::fs::read_dir(docs).unwrap() {
        let page = entry.unwrap().path();
        if page.extension().is_none_or(|ext| ext != "md") {
            continue;
        }
        let text = std::fs::read_to_string(&page).unwrap();
        for (i, block) in blocks(&text, "```abc").into_iter().enumerate() {
            let score = dir.join(format!("block{rendered}.abc"));
            std::fs::write(&score, block).unwrap();
            let samples = render_file(&score, 22_050)
                .unwrap_or_else(|e| panic!("{} block {i}: {e}", page.display()));
            assert!(
                samples.iter().any(|s| s.unsigned_abs() > 1_000),
                "{} block {i} renders silence",
                page.display()
            );
            rendered += 1;
        }
    }
    std::fs::remove_dir_all(&dir).ok();
    assert!(rendered > 0, "no abc blocks found in docs/");
}

#[cfg(feature = "render-advanced")]
#[test]
fn every_studio_score_in_the_docs_renders() {
    use sonomorph::advanced::render_score;

    let docs = concat!(env!("CARGO_MANIFEST_DIR"), "/docs");
    let dir = std::env::temp_dir().join(format!("sonomorph-docs-score-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut rendered = 0;
    for entry in std::fs::read_dir(docs).unwrap() {
        let page = entry.unwrap().path();
        if page.extension().is_none_or(|ext| ext != "md") {
            continue;
        }
        let text = std::fs::read_to_string(&page).unwrap();
        for (i, block) in blocks(&text, "```score").into_iter().enumerate() {
            let score = dir.join(format!("block{rendered}.score"));
            std::fs::write(&score, block).unwrap();
            let render = render_score(&score, None)
                .unwrap_or_else(|e| panic!("{} block {i}: {e}", page.display()));
            assert!(
                render.frames().iter().any(|s| s.abs() > 0.01),
                "{} block {i} renders silence",
                page.display()
            );
            rendered += 1;
        }
    }
    std::fs::remove_dir_all(&dir).ok();
    assert!(rendered > 0, "no score blocks found in docs/");
}
