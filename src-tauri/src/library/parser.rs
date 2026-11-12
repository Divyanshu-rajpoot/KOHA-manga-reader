//! File-name parser: works out the series and volume number from a CBZ's name.
//!
//! Rules, first match wins (see ARCHITECTURE.md → File-name parser):
//! 1. `{Series} v{NN}` / `Vol. {N}` / `Volume {N}` (also ranges, `v01-03`)
//! 2. `{Series} c{NNN}` / `Ch {N}` / `Chapter {N}`. If a name has both a
//!    volume and a chapter marker it is a chapter file, so the chapter wins.
//! 3. `{Series} {NN}-{NN}`: an omnibus covering a range of volumes
//! 4. `{Series} {NN}`: a bare trailing number is taken as the volume
//! 5. Otherwise unmatched: the user picks the series in the import queue.
//!
//! Tags like `[Group]`, `(Digital)`, `(2019)` are stripped first.
//! ComicInfo.xml, when present, overrides all of this (handled by the importer).

use super::Unit;

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedName {
    /// Best guess at the series title. May be non-empty even when unmatched.
    pub series: String,
    pub number: Option<f64>,
    /// End of the range for an omnibus (`01-05`).
    pub number_end: Option<f64>,
    pub unit: Unit,
}

#[cfg(test)]
impl ParsedName {
    pub fn is_match(&self) -> bool {
        !self.series.is_empty() && self.number.is_some()
    }
}

const VOLUME_WORDS: [&str; 4] = ["volume", "vol.", "vol", "v"];
const CHAPTER_WORDS: [&str; 8] = ["chapter", "ch.", "ch", "c", "episode", "ep.", "ep", "#"];
const RANGE_SEPARATORS: [char; 4] = ['-', '–', '—', '~'];

type Span = (f64, Option<f64>);

/// Removes `[...]`, `(...)` and `{...}` groups.
fn strip_tags(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut depth = 0usize;
    for c in name.chars() {
        match c {
            '[' | '(' | '{' => depth += 1,
            ']' | ')' | '}' => depth = depth.saturating_sub(1),
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out
}

fn parse_number(text: &str) -> Option<f64> {
    let valid = !text.is_empty()
        && text.len() <= 7
        && text.starts_with(|c: char| c.is_ascii_digit())
        && text.ends_with(|c: char| c.is_ascii_digit())
        && text.chars().all(|c| c.is_ascii_digit() || c == '.')
        && text.matches('.').count() <= 1;
    if valid {
        text.parse().ok()
    } else {
        None
    }
}

/// `07`, `12.5` or a range like `01-05`.
fn parse_span(text: &str) -> Option<Span> {
    let mut parts = text.split(RANGE_SEPARATORS);
    let first = parse_number(parts.next()?)?;
    match (parts.next(), parts.next()) {
        (None, _) => Some((first, None)),
        (Some(second), None) => {
            let last = parse_number(second)?;
            (last > first).then_some((first, Some(last)))
        }
        _ => None,
    }
}

/// Is `tokens[at]` a marker like `v07`, or a word like `Vol` followed by a number?
fn marker(tokens: &[String], at: usize, words: &[&str]) -> Option<Span> {
    let token = tokens[at].as_str();
    for word in words {
        if token == *word {
            if let Some(span) = tokens.get(at + 1).and_then(|next| parse_span(next)) {
                return Some(span);
            }
        } else if let Some(span) = token.strip_prefix(word).and_then(parse_span) {
            return Some(span);
        }
    }
    None
}

fn series_title(tokens: &[&str]) -> String {
    let mut tokens = tokens.to_vec();
    while let Some(last) = tokens.last() {
        let filler = last.chars().all(|c| "-–—:,".contains(c)) || last.eq_ignore_ascii_case("omnibus");
        if !filler {
            break;
        }
        tokens.pop();
    }
    tokens
        .join(" ")
        .trim_end_matches(|c: char| "-–—:,".contains(c) || c.is_whitespace())
        .to_string()
}

/// Parses a file name without its extension.
pub fn parse_file_name(stem: &str) -> ParsedName {
    let mut cleaned = strip_tags(stem).replace('_', " ");
    if !cleaned.trim().contains(' ') {
        // "Paper.Ronin.v07": dots are the word separators.
        cleaned = cleaned.replace('.', " ");
    }
    let tokens: Vec<&str> = cleaned.split_whitespace().collect();
    let lower: Vec<String> = tokens
        .iter()
        .map(|token| token.to_lowercase().trim_end_matches([',', ';']).to_string())
        .collect();

    let find = |words: &[&str]| (0..lower.len()).find_map(|at| marker(&lower, at, words).map(|span| (at, span)));
    let volume = find(&VOLUME_WORDS);
    let chapter = find(&CHAPTER_WORDS);

    let (title_end, span, unit) = match (volume, chapter) {
        (Some((v_at, _)), Some((c_at, span))) => (v_at.min(c_at), Some(span), Unit::Chapter),
        (Some((at, span)), None) => (at, Some(span), Unit::Volume),
        (None, Some((at, span))) => (at, Some(span), Unit::Chapter),
        (None, None) => {
            // Omnibus range, then a bare trailing number.
            let range = (1..lower.len())
                .rev()
                .find_map(|at| parse_span(&lower[at]).filter(|span| span.1.is_some()).map(|span| (at, span)));
            let bare = lower
                .len()
                .checked_sub(1)
                .filter(|&last| last >= 1)
                .and_then(|last| parse_number(&lower[last]).map(|n| (last, (n, None))));
            match range.or(bare) {
                Some((at, span)) => (at, Some(span), Unit::Volume),
                None => (tokens.len(), None, Unit::Volume),
            }
        }
    };

    ParsedName {
        series: series_title(&tokens[..title_end]),
        number: span.map(|span| span.0),
        number_end: span.and_then(|span| span.1),
        unit,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn volume(series: &str, number: f64) -> ParsedName {
        ParsedName { series: series.into(), number: Some(number), number_end: None, unit: Unit::Volume }
    }

    fn omnibus(series: &str, from: f64, to: f64) -> ParsedName {
        ParsedName { series: series.into(), number: Some(from), number_end: Some(to), unit: Unit::Volume }
    }

    fn chapter(series: &str, number: f64) -> ParsedName {
        ParsedName { series: series.into(), number: Some(number), number_end: None, unit: Unit::Chapter }
    }

    #[test]
    fn parses_real_world_style_names() {
        let cases = [
            // v{NN}
            ("Paper Ronin v07", volume("Paper Ronin", 7.0)),
            ("Ashen Tide v12", volume("Ashen Tide", 12.0)),
            ("Paper Ronin v07 (2019) (Digital) (Group)", volume("Paper Ronin", 7.0)),
            ("[Group] Paper Ronin v07 [1080p]", volume("Paper Ronin", 7.0)),
            ("Paper_Ronin_v07", volume("Paper Ronin", 7.0)),
            ("Paper.Ronin.v07", volume("Paper Ronin", 7.0)),
            ("Paper Ronin - v07", volume("Paper Ronin", 7.0)),
            ("Paper Ronin, v07", volume("Paper Ronin", 7.0)),
            ("paper ronin V7", volume("paper ronin", 7.0)),
            ("Paper Ronin v007 (Digital) {tag}", volume("Paper Ronin", 7.0)),
            ("86 v01", volume("86", 1.0)),
            ("20th Century Boys v03", volume("20th Century Boys", 3.0)),
            ("Dr. STONE v01", volume("Dr. STONE", 1.0)),
            ("Vinland Saga v12", volume("Vinland Saga", 12.0)),
            ("Paper Ronin v12.5", volume("Paper Ronin", 12.5)),
            // Vol / Vol. / Volume
            ("Ember Ward Vol 04", volume("Ember Ward", 4.0)),
            ("Ember Ward Vol. 4", volume("Ember Ward", 4.0)),
            ("Ember Ward Vol.04", volume("Ember Ward", 4.0)),
            ("Ember Ward Volume 4", volume("Ember Ward", 4.0)),
            ("Ember Ward - Volume 04 (Digital)", volume("Ember Ward", 4.0)),
            ("EMBER WARD VOL 4", volume("EMBER WARD", 4.0)),
            // Omnibus ranges
            ("Quiet Blade Omnibus 01-05", omnibus("Quiet Blade", 1.0, 5.0)),
            ("Quiet Blade 01-05", omnibus("Quiet Blade", 1.0, 5.0)),
            ("Quiet Blade v01-03", omnibus("Quiet Blade", 1.0, 3.0)),
            ("Quiet Blade Vol 1–3 (Omnibus)", omnibus("Quiet Blade", 1.0, 3.0)),
            // Chapters
            ("Tidebreaker c012", chapter("Tidebreaker", 12.0)),
            ("Tidebreaker Ch 12", chapter("Tidebreaker", 12.0)),
            ("Tidebreaker Ch. 110", chapter("Tidebreaker", 110.0)),
            ("Tidebreaker - Chapter 12", chapter("Tidebreaker", 12.0)),
            ("Tidebreaker ch.12.5", chapter("Tidebreaker", 12.5)),
            ("Tidebreaker Episode 3", chapter("Tidebreaker", 3.0)),
            ("Tidebreaker #45", chapter("Tidebreaker", 45.0)),
            ("Tidebreaker v02 c010", chapter("Tidebreaker", 10.0)),
            ("Tidebreaker c010 (v02) [Group]", chapter("Tidebreaker", 10.0)),
            // Bare trailing number
            ("Paper Ronin 07", volume("Paper Ronin", 7.0)),
            ("Paper Ronin - 07 (Digital)", volume("Paper Ronin", 7.0)),
        ];
        for (name, expected) in cases {
            let parsed = parse_file_name(name);
            assert_eq!(parsed, expected, "{name}");
            assert!(parsed.is_match(), "{name}");
        }
    }

    #[test]
    fn leaves_unclear_names_unmatched() {
        for (name, series) in [
            ("chapter_bundle_final", "chapter bundle final"),
            ("Paper Ronin", "Paper Ronin"),
            ("Paper Ronin (Complete)", "Paper Ronin"),
            ("scan0001x", "scan0001x"),
            ("Vagabond", "Vagabond"),
        ] {
            let parsed = parse_file_name(name);
            assert!(!parsed.is_match(), "{name}");
            assert_eq!(parsed.series, series, "{name}");
        }
    }

    #[test]
    fn a_number_without_a_series_is_not_a_match() {
        for name in ["v07", "Vol. 3", "c012", "[Group] (Digital)", ""] {
            let parsed = parse_file_name(name);
            assert!(!parsed.is_match(), "{name}");
            assert_eq!(parsed.series, "", "{name}");
        }
    }

    #[test]
    fn rejects_backwards_ranges() {
        assert_eq!(parse_span("05-01"), None);
        assert_eq!(parse_span("1-2-3"), None);
        assert_eq!(parse_span("01-05"), Some((1.0, Some(5.0))));
    }
}
