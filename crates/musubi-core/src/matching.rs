//! Reine Matching-Logik: Titel vergleichen, Kandidaten bewerten, Staffeln auf Werke verteilen.

use musubi_domain::{ItemKind, Mapping, MappingSource, Media};
use strsim::jaro_winkler;

/// Ab diesem Wert wird eine Zuordnung automatisch übernommen.
pub const AUTO_ACCEPT: f64 = 0.92;
/// Mindestabstand zum Zweitplatzierten für eine automatische Übernahme.
pub const AUTO_MARGIN: f64 = 0.05;
/// Abzug für Fortsetzungen, wenn die Suche nicht nach einer Fortsetzung fragt.
const SEQUEL_PENALTY: f64 = 0.06;
/// Darunter ist ein Kandidat zu schlecht, um ihn überhaupt zuzuordnen.
pub const MIN_GUESS: f64 = 0.6;
/// Vereinheitlicht Titel: Kleinschreibung, keine Satzzeichen, keine Staffel-Angaben.
pub fn normalize_title(title: &str) -> String {
    let cleaned: String = title
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();
    let words: Vec<&str> = cleaned.split_whitespace().collect();
    let mut out: Vec<&str> = Vec::with_capacity(words.len());
    let mut i = 0;
    while i < words.len() {
        let word = words[i];
        let next = words.get(i + 1).copied();
        let is_number = |w: &str| w.chars().all(|c| c.is_ascii_digit());
        // "season 2", "part 2", "cour 2"
        if matches!(word, "season" | "part" | "cour") && next.is_some_and(is_number) {
            i += 2;
            continue;
        }
        // "2nd season"
        if next == Some("season") && is_ordinal(word) {
            i += 2;
            continue;
        }
        out.push(word);
        i += 1;
    }
    // Eine angehängte Jahreszahl wie "(2019)" gehört nicht zum Titel.
    if out.len() > 1 && out.last().is_some_and(|w| is_year(w)) {
        out.pop();
    }
    out.join(" ")
}

/// Erkennt Angaben wie „Season 2“, „2nd Season“ oder „Part 2“ (aber nicht Teil oder Staffel 1).
fn names_a_later_part(title: &str) -> bool {
    let cleaned: String = title
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();
    let words: Vec<&str> = cleaned.split_whitespace().collect();
    words.windows(2).any(|pair| match (pair[0], pair[1]) {
        ("season" | "part" | "cour", number) => number.parse::<u32>().is_ok_and(|n| n > 1),
        (ordinal, "season" | "part" | "cour") => {
            is_ordinal(ordinal) && !ordinal.starts_with("1") || ordinal == "final"
        }
        _ => false,
    })
}

fn is_year(word: &str) -> bool {
    word.len() == 4
        && word
            .parse::<u32>()
            .is_ok_and(|y| (1950..=2100).contains(&y))
}

fn is_ordinal(word: &str) -> bool {
    word.strip_suffix("st")
        .or_else(|| word.strip_suffix("nd"))
        .or_else(|| word.strip_suffix("rd"))
        .or_else(|| word.strip_suffix("th"))
        .is_some_and(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
}

/// Suchbegriffe für AniList, der genaueste zuerst. `normalize_title` taugt nur zum Vergleichen:
/// Aus „JoJo's“ würde „jojo s“, und dafür findet AniList nichts.
pub fn search_terms(title: &str) -> Vec<String> {
    let trimmed = title.trim();
    let without_year = trimmed
        .rsplit_once('(')
        .filter(|(_, rest)| {
            rest.trim_end_matches(')')
                .parse::<i32>()
                .is_ok_and(|y| (1950..=2100).contains(&y))
        })
        .map_or(trimmed, |(head, _)| head.trim());
    let without_subtitle = [": ", " - "]
        .iter()
        .filter_map(|separator| without_year.split_once(separator))
        .map(|(head, _)| head.trim())
        .min_by_key(|head| head.len())
        .unwrap_or(without_year);
    let without_umlauts: String = without_year
        .chars()
        .flat_map(|c| match c {
            'ä' | 'Ä' => vec!['a'],
            'ö' | 'Ö' => vec!['o'],
            'ü' | 'Ü' => vec!['u'],
            'ß' => vec!['s', 's'],
            other => vec![other],
        })
        .collect();
    let mut terms: Vec<String> = Vec::new();
    for term in [
        without_year.to_owned(),
        without_umlauts,
        without_year.replace('\'', ""),
        without_subtitle.to_owned(),
        normalize_title(title),
    ] {
        if !term.is_empty() && !terms.contains(&term) {
            terms.push(term);
        }
    }
    terms
}

#[derive(Debug, Clone)]
pub struct MatchQuery<'a> {
    pub title: &'a str,
    pub year: Option<i32>,
    pub kind: ItemKind,
    pub episodes: Option<u32>,
}

/// Bewertet einen Kandidaten. Werte über 1 sind möglich, wenn alle Boni zutreffen.
pub fn score_candidate(query: &MatchQuery<'_>, media: &Media) -> f64 {
    let wanted = normalize_title(query.title);
    let query_is_sequel = names_a_later_part(query.title);
    let title_score = media
        .title
        .all()
        .chain(media.synonyms.iter().map(String::as_str))
        .map(|candidate| {
            let similarity = jaro_winkler(&wanted, &normalize_title(candidate));
            // Die Normalisierung entfernt Staffelangaben. Ohne Abzug wären „Foo“ und
            // „Foo 2nd Season“ gleichwertig und jede Serie mit Fortsetzung müsste geprüft werden.
            if names_a_later_part(candidate) && !query_is_sequel {
                similarity - SEQUEL_PENALTY
            } else {
                similarity
            }
        })
        .fold(0.0, f64::max);

    let year_bonus = match (query.year, media.season_year) {
        (Some(a), Some(b)) => match (a - b).abs() {
            0 => 0.04,
            1 => 0.02,
            2 => 0.0,
            _ => -0.08,
        },
        _ => 0.0,
    };
    let format_bonus = match (query.kind, media.format.as_deref()) {
        (ItemKind::Movie, Some("MOVIE")) => 0.02,
        (ItemKind::Series, Some("TV" | "TV_SHORT" | "ONA")) => 0.02,
        (ItemKind::Movie, Some(_)) | (ItemKind::Series, Some("MOVIE")) => -0.05,
        _ => 0.0,
    };
    let episode_bonus = match (query.episodes, media.episodes) {
        (Some(a), Some(b)) if a == b => 0.03,
        _ => 0.0,
    };
    // Nach oben offen: Beim Vergleich mit dem Zweitplatzierten zählt jeder Bonus.
    (title_score + year_bonus + format_bonus + episode_bonus).max(0.0)
}

/// Sortiert Kandidaten absteigend nach Bewertung.
pub fn rank<'m>(query: &MatchQuery<'_>, candidates: &'m [Media]) -> Vec<(&'m Media, f64)> {
    let mut ranked: Vec<_> = candidates
        .iter()
        .map(|m| (m, score_candidate(query, m)))
        .collect();
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
    ranked
}

/// `true`, wenn der beste Kandidat sicher genug für eine automatische Übernahme ist.
/// Wie sicher der beste Kandidat einer Suche ist.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Sure,
    Unsure,
    NoMatch,
}

pub fn verdict(ranked: &[(&Media, f64)]) -> Verdict {
    if should_auto_accept(ranked) {
        Verdict::Sure
    } else if ranked.first().is_some_and(|(_, best)| *best >= MIN_GUESS) {
        Verdict::Unsure
    } else {
        Verdict::NoMatch
    }
}

pub fn should_auto_accept(ranked: &[(&Media, f64)]) -> bool {
    match ranked {
        [] => false,
        [(_, best)] => *best >= AUTO_ACCEPT,
        [(_, best), (_, second), ..] => *best >= AUTO_ACCEPT && best - second >= AUTO_MARGIN,
    }
}

/// Episoden einer Jellyfin-Staffel.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeasonSpan {
    pub season: u32,
    pub first_episode: u32,
    pub count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainLink {
    pub media_id: i64,
    pub episodes: Option<u32>,
    pub title: String,
}

/// Verteilt Staffeln auf eine Kette aufeinanderfolgender AniList-Werke.
///
/// Das deckt beide Richtungen ab: eine Jellyfin-Staffel über mehrere Cours und ein Werk, das
/// sich über mehrere Jellyfin-Staffeln erstreckt.
pub fn plan_chain(
    jellyfin_id: &str,
    seasons: &[SeasonSpan],
    chain: &[ChainLink],
    source: MappingSource,
    confidence: f64,
) -> Vec<Mapping> {
    let mut out = Vec::new();
    let mut link_index = 0;
    let mut used = 0u32;
    for span in seasons {
        let mut first = span.first_episode;
        let mut remaining = span.count;
        while remaining > 0 {
            let Some(link) = chain.get(link_index) else {
                return out;
            };
            let available = link.episodes.map(|e| e.saturating_sub(used));
            let take = available.map_or(remaining, |a| a.min(remaining));
            if take == 0 {
                link_index += 1;
                used = 0;
                continue;
            }
            out.push(Mapping {
                jellyfin_id: jellyfin_id.to_owned(),
                season: span.season,
                ep_start: first,
                ep_end: link.episodes.map(|_| first + take - 1),
                media_id: link.media_id,
                offset: used as i32 + 1 - first as i32,
                source,
                confidence,
                title: link.title.clone(),
            });
            used += take;
            first += take;
            remaining -= take;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_keeps_punctuation_and_falls_back_to_simpler_forms() {
        let jojo = search_terms("JoJo's Bizarre Adventure");
        assert_eq!(jojo[0], "JoJo's Bizarre Adventure");
        assert!(jojo.contains(&"JoJos Bizarre Adventure".to_owned()));

        let arifureta = search_terms("Arifureta: From Commonplace to World's Strongest");
        assert!(arifureta.contains(&"Arifureta".to_owned()));

        assert_eq!(search_terms("Berserk (2016)")[0], "Berserk");
        assert_eq!(search_terms("Kämpfer")[1], "Kampfer");
        assert_eq!(search_terms("K-ON!")[0], "K-ON!");
    }

    #[test]
    fn verdict_separates_sure_unsure_and_no_match() {
        let media = Media::default();
        assert_eq!(verdict(&[(&media, 0.95)]), Verdict::Sure);
        // Knapp hinter dem Zweiten ist trotz hohem Wert nicht sicher.
        assert_eq!(verdict(&[(&media, 0.95), (&media, 0.94)]), Verdict::Unsure);
        assert_eq!(verdict(&[(&media, MIN_GUESS)]), Verdict::Unsure);
        assert_eq!(verdict(&[(&media, MIN_GUESS - 0.01)]), Verdict::NoMatch);
        assert_eq!(verdict(&[]), Verdict::NoMatch);
    }
    use musubi_domain::{MediaTitle, resolve_episode};

    fn media(id: i64, title: &str, year: i32, format: &str, episodes: u32) -> Media {
        Media {
            id,
            title: MediaTitle {
                romaji: Some(title.into()),
                ..MediaTitle::default()
            },
            season_year: Some(year),
            format: Some(format.into()),
            episodes: Some(episodes),
            ..Media::default()
        }
    }

    #[test]
    fn normalization_removes_noise() {
        let cases = [
            ("Attack on Titan: Season 2", "attack on titan"),
            ("Mushoku Tensei 2nd Season", "mushoku tensei"),
            ("Steins;Gate (2011)", "steins gate"),
            ("Spy x Family Part 2", "spy x family"),
            ("2012", "2012"),
            ("進撃の巨人", "進撃の巨人"),
        ];
        for (input, expected) in cases {
            assert_eq!(normalize_title(input), expected, "{input}");
        }
    }

    #[test]
    fn exact_title_with_right_year_is_accepted() {
        let query = MatchQuery {
            title: "Frieren: Beyond Journey's End",
            year: Some(2023),
            kind: ItemKind::Series,
            episodes: Some(28),
        };
        let candidates = [
            media(1, "Frieren: Beyond Journey's End", 2023, "TV", 28),
            media(2, "Totally Different Show", 2023, "TV", 12),
        ];
        let ranked = rank(&query, &candidates);
        assert_eq!(ranked[0].0.id, 1);
        assert!(should_auto_accept(&ranked));
    }

    #[test]
    fn ambiguous_remakes_need_review() {
        let query = MatchQuery {
            title: "Hunter x Hunter",
            year: None,
            kind: ItemKind::Series,
            episodes: None,
        };
        let candidates = [
            media(1, "Hunter x Hunter", 1999, "TV", 62),
            media(2, "Hunter x Hunter", 2011, "TV", 148),
        ];
        assert!(!should_auto_accept(&rank(&query, &candidates)));
    }

    #[test]
    fn year_disambiguates_remakes() {
        let query = MatchQuery {
            title: "Hunter x Hunter",
            year: Some(2011),
            kind: ItemKind::Series,
            episodes: Some(148),
        };
        let candidates = [
            media(1, "Hunter x Hunter", 1999, "TV", 62),
            media(2, "Hunter x Hunter", 2011, "TV", 148),
        ];
        let ranked = rank(&query, &candidates);
        assert_eq!(ranked[0].0.id, 2);
        assert!(should_auto_accept(&ranked));
    }

    #[test]
    fn first_season_beats_its_sequels() {
        let query = MatchQuery {
            title: "Sousou no Frieren",
            year: Some(2023),
            kind: ItemKind::Series,
            episodes: Some(28),
        };
        let candidates = [
            media(3, "Sousou no Frieren 2nd Season", 2024, "TV", 10),
            media(1, "Sousou no Frieren", 2023, "TV", 28),
            media(4, "Sousou no Frieren Final Season", 2024, "TV", 12),
        ];
        let ranked = rank(&query, &candidates);
        assert_eq!(ranked[0].0.id, 1);
        assert!(should_auto_accept(&ranked));
    }

    #[test]
    fn explicit_sequel_search_is_not_penalized() {
        let query = MatchQuery {
            title: "Mob Psycho 100 Season 2",
            year: Some(2019),
            kind: ItemKind::Series,
            episodes: Some(13),
        };
        let sequel = media(2, "Mob Psycho 100 2nd Season", 2019, "TV", 13);
        assert!(score_candidate(&query, &sequel) >= AUTO_ACCEPT);
    }

    #[test]
    fn later_parts_are_detected() {
        assert!(names_a_later_part("Foo Season 2"));
        assert!(names_a_later_part("Foo 3rd Season"));
        assert!(names_a_later_part("Foo Part 2"));
        assert!(names_a_later_part("Foo: The Final Season"));
        assert!(!names_a_later_part("Foo Season 1"));
        assert!(!names_a_later_part("Foo 1st Season"));
        assert!(!names_a_later_part("Foo"));
    }

    #[test]
    fn weak_match_is_rejected_and_empty_is_not_accepted() {
        let query = MatchQuery {
            title: "Bleach",
            year: None,
            kind: ItemKind::Series,
            episodes: None,
        };
        let candidates = [media(1, "One Piece", 1999, "TV", 1000)];
        assert!(!should_auto_accept(&rank(&query, &candidates)));
        assert!(!should_auto_accept(&[]));
    }

    #[test]
    fn movie_prefers_movie_format() {
        let query = MatchQuery {
            title: "Akira",
            year: Some(1988),
            kind: ItemKind::Movie,
            episodes: None,
        };
        let movie = media(1, "Akira", 1988, "MOVIE", 1);
        let tv = media(2, "Akira", 1988, "TV", 12);
        assert!(score_candidate(&query, &movie) > score_candidate(&query, &tv));
    }

    fn link(media_id: i64, episodes: Option<u32>) -> ChainLink {
        ChainLink {
            media_id,
            episodes,
            title: String::new(),
        }
    }

    fn span(season: u32, first_episode: u32, count: u32) -> SeasonSpan {
        SeasonSpan {
            season,
            first_episode,
            count,
        }
    }

    fn plan(seasons: &[SeasonSpan], chain: &[ChainLink]) -> Vec<Mapping> {
        plan_chain("x", seasons, chain, MappingSource::Title, 1.0)
    }

    #[test]
    fn one_to_one_seasons() {
        let maps = plan(
            &[span(1, 1, 12), span(2, 1, 13)],
            &[link(10, Some(12)), link(20, Some(13))],
        );
        assert_eq!(resolve_episode(&maps, 1, 12), Some((10, 12)));
        assert_eq!(resolve_episode(&maps, 2, 1), Some((20, 1)));
        assert_eq!(resolve_episode(&maps, 2, 13), Some((20, 13)));
    }

    #[test]
    fn single_season_splits_over_cours() {
        let maps = plan(&[span(1, 1, 24)], &[link(10, Some(12)), link(20, Some(12))]);
        assert_eq!(resolve_episode(&maps, 1, 12), Some((10, 12)));
        assert_eq!(resolve_episode(&maps, 1, 13), Some((20, 1)));
        assert_eq!(resolve_episode(&maps, 1, 24), Some((20, 12)));
    }

    #[test]
    fn one_work_spans_two_seasons() {
        let maps = plan(&[span(1, 1, 12), span(2, 1, 12)], &[link(10, Some(24))]);
        assert_eq!(resolve_episode(&maps, 2, 1), Some((10, 13)));
        assert_eq!(resolve_episode(&maps, 2, 12), Some((10, 24)));
    }

    #[test]
    fn absolute_numbering_in_later_seasons() {
        let maps = plan(
            &[span(1, 1, 12), span(2, 13, 12)],
            &[link(10, Some(12)), link(20, Some(12))],
        );
        assert_eq!(resolve_episode(&maps, 2, 13), Some((20, 1)));
    }

    #[test]
    fn releasing_show_stays_open_ended() {
        let maps = plan(&[span(1, 1, 5)], &[link(10, None)]);
        assert_eq!(maps[0].ep_end, None);
        assert_eq!(resolve_episode(&maps, 1, 9), Some((10, 9)));
    }

    #[test]
    fn episodes_beyond_the_chain_stay_unmapped() {
        let maps = plan(&[span(1, 1, 24)], &[link(10, Some(12))]);
        assert_eq!(resolve_episode(&maps, 1, 13), None);
    }
}
