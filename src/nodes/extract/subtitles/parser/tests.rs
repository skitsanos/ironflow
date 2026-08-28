use super::*;

// run_blocking_step needs 'static, so the fixture is moved in rather than
// borrowed.
async fn parse(contents: &'static str, is_vtt: bool) -> Vec<SubtitleCue> {
    crate::util::execution::run_blocking_step(move |execution| {
        let limits = Limits {
            max_output_bytes: 1_000_000,
            max_items: 10_000,
            max_zip_entries: 10,
            max_zip_bytes: 10,
            max_pdf_pages: 10,
        };
        let mut budget = Budget::new("test", limits, &execution);
        parse_subtitle_cues(contents, is_vtt, "extract_vtt", &mut budget)
    })
    .await
    .unwrap()
}

#[tokio::test]
async fn voice_span_names_the_speaker_and_leaves_the_text_clean() {
    let cues = parse(
        "WEBVTT\n\n1\n00:00:00.000 --> 00:00:02.000\n<v Alice>Hello there</v>\n",
        true,
    )
    .await;
    assert_eq!(cues.len(), 1);
    assert_eq!(cues[0].speaker.as_deref(), Some("Alice"));
    assert_eq!(cues[0].text, "Hello there");
}

#[tokio::test]
async fn classes_are_not_part_of_the_speaker_name() {
    let cues = parse(
        "WEBVTT\n\n1\n00:00:00.000 --> 00:00:02.000\n<v.first.loud Bob Smith>Hi</v>\n",
        true,
    )
    .await;
    assert_eq!(cues[0].speaker.as_deref(), Some("Bob Smith"));
}

#[tokio::test]
async fn captions_without_a_voice_span_stay_unlabelled() {
    // The unlabelled path has to keep working: a caption-only transcript
    // still parses, with no speaker rather than an empty one.
    let cues = parse(
        "WEBVTT\n\n1\n00:00:00.000 --> 00:00:02.000\nJust a caption\n",
        true,
    )
    .await;
    assert_eq!(cues.len(), 1);
    assert_eq!(cues[0].speaker, None);
    assert_eq!(cues[0].text, "Just a caption");
}

#[tokio::test]
async fn other_markup_is_not_mistaken_for_a_voice_span() {
    let cues = parse(
        "WEBVTT\n\n1\n00:00:00.000 --> 00:00:02.000\n<i>emphasis</i> and <video> too\n",
        true,
    )
    .await;
    assert_eq!(cues[0].speaker, None);
}

#[tokio::test]
async fn srt_never_reports_a_speaker() {
    let cues = parse("1\n00:00:00,000 --> 00:00:02,000\n<v Alice>Hello\n", false).await;
    assert_eq!(cues[0].speaker, None);
}
