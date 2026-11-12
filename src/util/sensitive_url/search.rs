/// A fixed set of ASCII needles, searched without ASCII case.
pub(super) struct AsciiNeedles {
    needles: &'static [&'static str],
    /// Whether a byte starts a needle in either letter case. Most bytes do
    /// not, so the scan compares needles at few positions.
    starts: [bool; 256],
}

impl AsciiNeedles {
    /// Needles must be non-empty; an empty one fails constant evaluation.
    pub(super) const fn new(needles: &'static [&'static str]) -> Self {
        let mut starts = [false; 256];
        let mut index = 0;
        while index < needles.len() {
            let first = needles[index].as_bytes()[0];
            starts[first.to_ascii_lowercase() as usize] = true;
            starts[first.to_ascii_uppercase() as usize] = true;
            index += 1;
        }
        Self { needles, starts }
    }

    /// Earliest occurrence of any needle. When more than one matches at that
    /// index the longest wins, so a needle is never shadowed by one of its
    /// own prefixes.
    ///
    /// The scan stops at the first index that matches, so finding every
    /// occurrence by resuming after each one reads the text once (IF-177).
    /// Needles are ASCII: a match starts and ends on a character boundary.
    pub(super) fn find_first(&self, text: &str) -> Option<(usize, &'static str)> {
        let bytes = text.as_bytes();
        bytes.iter().enumerate().find_map(|(index, byte)| {
            if !self.starts[usize::from(*byte)] {
                return None;
            }
            let rest = &bytes[index..];
            self.needles
                .iter()
                .copied()
                .filter(|needle| starts_with_ignore_ascii_case(rest, needle.as_bytes()))
                .max_by_key(|needle| needle.len())
                .map(|needle| (index, needle))
        })
    }
}

fn starts_with_ignore_ascii_case(text: &[u8], prefix: &[u8]) -> bool {
    text.get(..prefix.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
}
