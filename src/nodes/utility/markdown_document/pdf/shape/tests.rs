use super::super::super::model::Style;
use super::*;
use crate::{engine::types::Context, util::execution::run_tracked_blocking_step};

#[tokio::test]
async fn expanded_link_text_is_bounded_before_layout() {
    let error = run_tracked_blocking_step(|execution| {
        let config = Config::parse(
            &serde_json::json!({"input":"", "output_path":"unused.pdf"}),
            &Context::new(),
            "markdown_to_pdf",
        )?;
        let mut runs = Vec::new();
        let target: Arc<str> = format!("https://example.com/{}", "a".repeat(1900)).into();
        for _ in 0..600 {
            runs.push(Run {
                text: "link".to_owned(),
                style: Style {
                    link: Some(target.clone()),
                    ..Default::default()
                },
            });
            runs.push(Run {
                text: " ".to_owned(),
                style: Style::default(),
            });
        }
        Shaper::new()?
            .paragraph(&runs, 400.0, 11.0, &config, &execution)
            .map(|_| ())
    })
    .await
    .unwrap_err();
    assert!(error.to_string().contains("expanded layout text"));
}

#[tokio::test]
async fn arabic_is_shaped_and_rtl_alignment_is_applied() {
    run_tracked_blocking_step(|execution| {
        let config = Config::parse(
            &serde_json::json!({"input":"", "output_path":"unused.pdf"}),
            &Context::new(),
            "markdown_to_pdf",
        )?;
        let mut shaper = Shaper::new()?;
        let joined = shaper.paragraph(
            &[Run {
                text: "\u{633}\u{644}\u{627}\u{645}".to_owned(),
                style: Style::default(),
            }],
            400.0,
            11.0,
            &config,
            &execution,
        )?;
        let glyphs: Vec<_> = joined[0]
            .runs
            .iter()
            .flat_map(|run| run.glyphs.iter().map(|g| g.glyph_id.to_u32()))
            .collect();
        assert!(joined[0].runs[0].x > 300.0);
        let mut isolated = Vec::new();
        for c in ['\u{633}', '\u{644}', '\u{627}', '\u{645}'] {
            let lines = shaper.paragraph(
                &[Run {
                    text: c.to_string(),
                    style: Style::default(),
                }],
                400.0,
                11.0,
                &config,
                &execution,
            )?;
            isolated.extend(
                lines[0]
                    .runs
                    .iter()
                    .flat_map(|r| r.glyphs.iter().map(|g| g.glyph_id.to_u32())),
            );
        }
        assert_ne!(
            glyphs, isolated,
            "connected Arabic must not use isolated glyphs"
        );
        Ok(())
    })
    .await
    .unwrap();
}
