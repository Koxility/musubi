//! Fuzzy-Filter für Listen.

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

/// Liefert die Indizes der passenden Texte, die besten Treffer zuerst.
/// Ein leeres Muster behält alle Einträge in ihrer Reihenfolge.
pub fn filter<'a>(texts: impl Iterator<Item = &'a str>, query: &str) -> Vec<usize> {
    if query.is_empty() {
        return texts.enumerate().map(|(i, _)| i).collect();
    }
    let mut matcher = Matcher::new(Config::DEFAULT);
    let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);
    let mut buffer = Vec::new();
    let mut scored: Vec<(usize, u32)> = texts
        .enumerate()
        .filter_map(|(i, text)| {
            pattern
                .score(Utf32Str::new(text, &mut buffer), &mut matcher)
                .map(|s| (i, s))
        })
        .collect();
    scored.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    scored.into_iter().map(|(i, _)| i).collect()
}

#[cfg(test)]
mod tests {
    use super::filter;

    #[test]
    fn empty_query_keeps_order() {
        assert_eq!(filter(["b", "a"].into_iter(), ""), [0, 1]);
    }

    #[test]
    fn matches_subsequences_and_drops_others() {
        let titles = ["Frieren", "One Piece", "Fruits Basket"];
        assert_eq!(filter(titles.into_iter(), "frn"), [0]);
        assert_eq!(filter(titles.into_iter(), "xyz"), Vec::<usize>::new());
    }
}
