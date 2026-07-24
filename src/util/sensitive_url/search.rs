pub(super) fn find_first_ascii_case_insensitive<'a>(
    text: &str,
    needles: &'a [&'a str],
) -> Option<(usize, &'a str)> {
    text.char_indices()
        .flat_map(|(index, _)| {
            needles.iter().filter_map(move |needle| {
                text[index..]
                    .get(..needle.len())
                    .filter(|candidate| candidate.eq_ignore_ascii_case(needle))
                    .map(|_| (index, *needle))
            })
        })
        .min_by_key(|(index, _)| *index)
}
