use super::*;
use lopdf::{Document, Encoding, Stream, dictionary};

const HEAD: &str = "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
    /CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
    /CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n";
const TAIL: &str = "endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n";

fn cmap(body: &str) -> String {
    format!("{HEAD}{body}{TAIL}")
}

fn cost(text: &str) -> Cost {
    scan(text.as_bytes(), || Ok(())).unwrap()
}

/// The codes lopdf maps for this CMap, or `None` when its parser rejects it
/// and the font falls back to a one-byte encoding.
fn lopdf_codes(text: &str) -> Option<u64> {
    let mut doc = Document::new();
    let stream = doc.add_object(Stream::new(dictionary! {}, text.as_bytes().to_vec()));
    let font = dictionary! { "Type" => "Font", "Encoding" => "Identity-H", "ToUnicode" => stream };
    match font.get_font_encoding(&doc).unwrap() {
        Encoding::UnicodeMapEncoding(map) => Some(
            map.bf_ranges
                .iter()
                .flat_map(|ranges| ranges.iter())
                .map(|(range, _)| u64::from(range.end() - range.start()) + 1)
                .sum(),
        ),
        _ => None,
    }
}

#[test]
fn well_formed_sections_are_charged_exactly() {
    let narrow = cmap(
        "1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n\
         1 beginbfrange\n<0001> <00FF> <0041>\nendbfrange\n",
    );
    assert_eq!(
        cost(&narrow),
        Cost {
            entries: 256,
            longest_target: 1
        },
        "255 codes and one codespace line"
    );
    assert_eq!(lopdf_codes(&narrow), Some(255));

    let mixed = cmap(
        "1 begincodespacerange\n<00> <FF>\nendcodespacerange\n\
         2 beginbfchar\n<01> <00660069>\n<02> <0041>\nendbfchar\n\
         2 beginbfrange\n<10> <1F> <00410042>\n<20> <22> [<0041> <004200430044> <0045>]\nendbfrange\n",
    );
    // codespace 1, bfchar 2 + 1, range 16 x 2, array 3 x 3.
    assert_eq!(
        cost(&mixed),
        Cost {
            entries: 1 + 3 + 32 + 9,
            longest_target: 3
        }
    );
    assert_eq!(lopdf_codes(&mixed), Some(2 + 16 + 3));
}

#[test]
fn wide_ranges_are_measured_without_being_expanded() {
    for (low, high, span) in [
        ("000001", "FFFFFE", 16_777_214_u64),
        ("00000001", "FFFFFFFE", 4_294_967_294),
        ("00000000", "FFFFFFFF", 4_294_967_296),
    ] {
        let text = cmap(&format!(
            "1 beginbfrange\n<{low}> <{high}> <0041>\nendbfrange\n"
        ));
        assert_eq!(cost(&text).entries, span, "{low}");
    }
    let long = cmap(&format!(
        "1 beginbfrange\n<0000> <FFFF> <{}>\nendbfrange\n",
        "0041".repeat(256)
    ));
    assert_eq!(
        cost(&long),
        Cost {
            entries: 65_536 * 256,
            longest_target: 256
        }
    );
}

#[test]
fn a_minified_cmap_is_not_overcharged() {
    let text = "/CIDInit /ProcSet findresource begin 12 dict begin begincmap \
        /CMapName /A def /CMapType 2 def 1 begincodespacerange <0000> <FFFF> endcodespacerange \
        3 beginbfchar <0001> <4E00> <0002> <4E01> <0003> <4E02> endbfchar \
        2 beginbfrange <0010> <0020> <5E00> <0021> <0030> <5F00> endbfrange \
        endcmap CMapName currentdict /CMap defineresource pop end end\n";
    assert_eq!(lopdf_codes(text), Some(3 + 17 + 16));
    assert_eq!(cost(text).entries, 1 + 3 + 17 + 16);
}

/// Every form lopdf accepts must cost at least the codes it maps, however the
/// range is dressed up. Each variant holds one 65,536-code range.
#[test]
fn a_range_cannot_be_hidden_from_the_scan() {
    let range = "<0000> <FFFF> <0041>";
    let variants = [
        (
            "comment ends the section",
            cmap(&format!(
                "1 beginbfrange\n% endbfrange\n{range}\nendbfrange\n"
            )),
        ),
        (
            "comment opens bfchar",
            cmap(&format!(
                "1 beginbfrange\n% endbfrange 1 beginbfchar\n{range}\nendbfrange\n"
            )),
        ),
        (
            "comment between lines",
            cmap(&format!(
                "2 beginbfrange\n<00> <01> <0041>\n%<00> <01> <0041\n{range}\nendbfrange\n"
            )),
        ),
        (
            "section on a string's comment line",
            format!(
                "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
             /CIDSystemInfo << /Registry (%) /Ordering (UCS) /Supplement 0 >> def 1 beginbfrange {range} endbfrange\n{TAIL}"
            ),
        ),
        (
            "bfchar hidden in a string",
            format!(
                "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
             /CIDSystemInfo << /Registry (1 beginbfchar %) /Ordering (UCS) /Supplement 0 >> def 1 beginbfrange\n{range}\nendbfrange\n{TAIL}"
            ),
        ),
        (
            "after a bfchar section",
            cmap(&format!(
                "1 beginbfchar\n<0000> <0041>\nendbfchar\n1 beginbfrange\n{range}\nendbfrange\n"
            )),
        ),
        (
            "tabs and no blanks",
            cmap("1 beginbfrange\n\t<0000><FFFF>\t<0041>\nendbfrange\n"),
        ),
        (
            "destination split over lines",
            cmap("1 beginbfrange\n<0000> <FFFF> <0041\n0042>\nendbfrange\n"),
        ),
        (
            "comment inside the destination",
            cmap("1 beginbfrange\n<0000> <FFFF> <0041 % >\n0042>\nendbfrange\n"),
        ),
        (
            "array destination",
            cmap("1 beginbfrange\n<0000> <FFFF> [<0041> <0042>]\nendbfrange\n"),
        ),
        (
            "array split over lines",
            cmap("1 beginbfrange\n<0000> <FFFF> [<0041\n> <0042>]\nendbfrange\n"),
        ),
        (
            "two on one line",
            cmap(&format!(
                "2 beginbfrange\n<0000> <0001> <0041> {range}\nendbfrange\n"
            )),
        ),
        (
            "carriage returns",
            cmap(&format!(
                "1 beginbfrange\r% endbfrange\r{range}\rendbfrange\r"
            )),
        ),
    ];
    for (name, text) in variants {
        let mapped = lopdf_codes(&text);
        assert!(
            mapped.is_some_and(|codes| codes >= 65_536),
            "{name}: lopdf must accept the variant for it to be a threat, got {mapped:?}"
        );
        assert!(
            cost(&text).entries >= mapped.unwrap(),
            "{name}: {:?} for {mapped:?} codes",
            cost(&text)
        );
    }
}

#[test]
fn the_longest_destination_is_found_in_any_section() {
    let text = cmap(&format!(
        "1 beginbfchar\n<01> <{}>\nendbfchar\n",
        "0041".repeat(200)
    ));
    assert_eq!(cost(&text).longest_target, 200);
    // A comment inside a destination hides its length: assume lopdf's maximum.
    let text = cmap("1 beginbfchar\n<01> <0041 % x\n0042>\nendbfchar\n");
    assert_eq!(cost(&text).longest_target, MAX_TARGET_UNITS);
}

#[test]
fn a_range_destination_that_hides_its_extent_is_over_budget() {
    for body in [
        "<00> <01> <0041 % >\n0042>",
        "<00> <01> [<0041> <0042 % >\n>]",
    ] {
        let text = cmap(&format!("1 beginbfrange\n{body}\nendbfrange\n"));
        assert_eq!(lopdf_codes(&text), Some(2), "{body}");
        assert_eq!(cost(&text).entries, u64::MAX, "{body}");
    }
}

#[test]
fn hostile_filler_is_scanned_in_linear_time_and_stays_cancellable() {
    // Shapes that restart a lookahead at every `<`: an unclosed destination,
    // an unclosed array, a comment inside a destination, and long blank runs.
    let fillers = [
        "<00 <00> <01> [".repeat(400_000),
        "<00> <01> [<0041> ".repeat(400_000),
        "<00 % <00> <01> [<00 %".repeat(300_000),
        format!(
            "{}{}",
            "<00> <01> <00".repeat(100_000),
            " ".repeat(4_000_000)
        ),
        format!("<00> <01> <{}", "0".repeat(6_000_000)),
    ];
    for filler in &fillers {
        let text = format!("1 beginbfrange\n{filler}");
        let started = std::time::Instant::now();
        cost(&text);
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "{:?} for {:?}...",
            started.elapsed(),
            &filler[..32]
        );
    }

    let mut checkpoints = 0;
    let error = scan(fillers[0].as_bytes(), || {
        checkpoints += 1;
        anyhow::ensure!(checkpoints < 3, "cancelled");
        Ok(())
    })
    .unwrap_err();
    assert_eq!(error.to_string(), "cancelled");
}

/// Deterministic generator of CMaps in lopdf's grammar, with the freedoms it
/// allows: any separator, comments carrying decoy keywords and ranges, strings
/// carrying `%`, destinations split over lines, arrays.
struct Generator {
    state: u64,
    clean: bool,
    text: String,
    next_code: u32,
    weighted: u64,
    codes: u64,
    longest: usize,
}

impl Generator {
    fn below(&mut self, bound: u64) -> u64 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        self.state % bound
    }

    fn pick(&mut self, options: &[&str]) -> String {
        options[self.below(options.len() as u64) as usize].to_string()
    }

    fn comment(&mut self) -> String {
        let decoy = self.pick(&[
            "",
            " endbfrange",
            " 1 beginbfchar",
            " endbfrange 1 beginbfchar",
            " endbfchar 1 beginbfrange",
            " <0041",
            " ( %",
        ]);
        format!("%{decoy}\n")
    }

    /// At least one separator: blanks, line ends or comments.
    fn gap(&mut self) -> String {
        let mut gap = self.pick(&[" ", "\n", "\t", "\r\n", "\r", "  \n"]);
        if !self.clean && self.below(4) == 0 {
            gap.push_str(&self.comment());
        }
        gap
    }

    fn blank(&mut self) -> String {
        self.pick(&["", " ", "\t", "  "])
    }

    fn target(&mut self) -> (String, usize) {
        let units = 1 + self.below(3) as usize;
        let mut text = "<".to_string();
        for unit in 0..units {
            text.push_str(&format!("{:04X}", 0x41 + unit));
            if self.below(6) == 0 {
                text.push_str(&self.pick(&[" ", "\n", "\r\n"]));
            }
        }
        text.push('>');
        self.longest = self.longest.max(units);
        (text, units)
    }

    fn range_section(&mut self) {
        self.text.push_str("3 beginbfrange");
        let gap = self.gap();
        self.text.push_str(&gap);
        for _ in 0..self.below(4) {
            let span = 1 + self.below(600) as u32;
            let (low, high) = (self.next_code, self.next_code + span - 1);
            if high > 0xFFFF {
                break;
            }
            self.next_code = high + 1;
            let (first, second) = (self.blank(), self.blank());
            let line = format!("{}<{low:04X}>{first}<{high:04X}>{second}", self.blank());
            self.text.push_str(&line);
            let span = u64::from(span);
            if self.below(3) == 0 {
                let (mut longest, mut total) = (0, 0);
                let mut array = "[".to_string();
                for element in 0..1 + self.below(3) {
                    let (target, units) = self.target();
                    array.push_str(if element == 0 { "" } else { " " });
                    array.push_str(&target);
                    longest = longest.max(units as u64);
                    total += units as u64;
                }
                array.push(']');
                self.text.push_str(&array);
                self.weighted += (span * longest).max(total);
            } else {
                let (target, units) = self.target();
                self.text.push_str(&target);
                self.weighted += span * units as u64;
            }
            self.codes += span;
            let gap = self.gap();
            self.text.push_str(&gap);
        }
        self.text.push_str("endbfrange");
        let gap = self.gap();
        self.text.push_str(&gap);
    }

    fn char_section(&mut self) {
        self.text.push_str("2 beginbfchar");
        let gap = self.gap();
        self.text.push_str(&gap);
        for _ in 0..self.below(4) {
            if self.next_code > 0xFFFF {
                break;
            }
            let (target, units) = self.target();
            let (lead, code, middle) = (self.blank(), self.next_code, self.blank());
            let line = format!("{lead}<{code:04X}>{middle}{target}");
            self.text.push_str(&line);
            self.next_code += 1;
            self.weighted += units as u64;
            self.codes += 1;
            let gap = self.gap();
            self.text.push_str(&gap);
        }
        self.text.push_str("endbfchar");
        let gap = self.gap();
        self.text.push_str(&gap);
    }

    fn generate(seed: u64, clean: bool) -> Self {
        let mut generator = Self {
            state: seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1,
            clean,
            text: String::new(),
            next_code: 0,
            weighted: 0,
            codes: 0,
            longest: 1,
        };
        let registry = if clean {
            "Adobe".to_string()
        } else {
            generator.pick(&[
                "Adobe",
                "%",
                "1 beginbfchar %",
                "endbfrange",
                "<00> <FF> <0041>",
            ])
        };
        generator.text = format!(
            "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n\
             /CIDSystemInfo << /Registry ({registry}) /Ordering (UCS) /Supplement 0 >> def"
        );
        let gap = generator.gap();
        generator.text.push_str(&gap);
        generator.text.push_str("/CMapName /X def\n/CMapType 2 def");
        let gap = generator.gap();
        generator.text.push_str(&gap);
        generator
            .text
            .push_str("1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange");
        let gap = generator.gap();
        generator.text.push_str(&gap);
        for _ in 0..1 + generator.below(4) {
            if generator.below(3) == 0 {
                generator.char_section();
            } else {
                generator.range_section();
            }
        }
        generator.text.push_str(TAIL);
        generator
    }
}

#[test]
fn generated_cmaps_are_never_undercharged_and_clean_ones_are_exact() {
    let mut accepted = 0;
    for seed in 1..=3000 {
        let clean = seed % 3 == 0;
        let generated = Generator::generate(seed, clean);
        let Some(mapped) = lopdf_codes(&generated.text) else {
            continue;
        };
        accepted += 1;
        assert_eq!(mapped, generated.codes, "seed {seed}:\n{}", generated.text);
        let cost = cost(&generated.text);
        assert!(
            cost.entries > generated.weighted && cost.longest_target >= generated.longest,
            "seed {seed}: {cost:?} for {} entries:\n{}",
            generated.weighted,
            generated.text
        );
        if clean {
            assert_eq!(
                (cost.entries, cost.longest_target),
                (generated.weighted + 1, generated.longest),
                "seed {seed}:\n{}",
                generated.text
            );
        }
    }
    assert!(accepted > 1500, "lopdf accepted only {accepted} of 3000");
}
