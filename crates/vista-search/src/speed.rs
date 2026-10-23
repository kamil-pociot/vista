//! Speed search matching (IntelliJ's type-to-search in trees and lists):
//! case-insensitive, either a plain substring of the name or pieces of the
//! query at word starts (`stRs` finds `status.rs`, `mfb` finds `MyFileBrowser`).

/// The char positions in `name` that `query` matches, or `None`.
pub fn speed_match(name: &str, query: &str) -> Option<Vec<usize>> {
    let name: Vec<char> = name.chars().collect();
    let query: Vec<char> = query.chars().flat_map(char::to_lowercase).collect();
    if query.is_empty() {
        return Some(Vec::new());
    }
    let lower: Vec<char> = name.iter().map(|c| c.to_lowercase().next().unwrap_or(*c)).collect();
    if let Some(at) = lower.windows(query.len()).position(|w| w == query.as_slice()) {
        return Some((at..at + query.len()).collect());
    }
    let starts: Vec<bool> = (0..name.len()).map(|i| word_start(&name, i)).collect();
    let mut out = Vec::new();
    fragments(&lower, &starts, &query, 0, &mut out).then_some(out)
}

/// A word begins at the start, after a separator, at an upper-case letter
/// after a lower-case one, and at a digit after a letter.
fn word_start(name: &[char], i: usize) -> bool {
    let Some(prev) = i.checked_sub(1).map(|p| name[p]) else {
        return true;
    };
    let c = name[i];
    c.is_alphanumeric()
        && (!prev.is_alphanumeric()
            || (c.is_uppercase() && prev.is_lowercase())
            || (c.is_ascii_digit() && !prev.is_ascii_digit()))
}

/// Matches `query` from name position `from`: the next query char starts a
/// fragment at a word start, which then runs on over following chars.
/// Backtracks over shorter fragments when a longer one leads nowhere.
fn fragments(name: &[char], starts: &[bool], query: &[char], from: usize, out: &mut Vec<usize>) -> bool {
    if query.is_empty() {
        return true;
    }
    for at in from..name.len() {
        if !starts[at] || name[at] != query[0] {
            continue;
        }
        // The longest run first: `stat` in `status` beats `s` + `tat…`.
        let run = (0..query.len())
            .take_while(|&k| at + k < name.len() && name[at + k] == query[k])
            .count();
        for len in (1..=run).rev() {
            let mark = out.len();
            out.extend(at..at + len);
            if fragments(name, starts, &query[len..], at + len, out) {
                return true;
            }
            out.truncate(mark);
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::speed_match;

    fn m(name: &str, q: &str) -> Option<String> {
        let idx = speed_match(name, q)?;
        Some(
            name.chars()
                .enumerate()
                .map(|(i, c)| if idx.contains(&i) { c.to_ascii_uppercase() } else { '.' })
                .collect(),
        )
    }

    #[test]
    fn substrings_match_anywhere_ignoring_case() {
        assert_eq!(m("status.rs", "tus").as_deref(), Some("...TUS..."));
        assert_eq!(m("README.md", "read").as_deref(), Some("READ....."));
        assert_eq!(m("status.rs", ""), Some(".........".into()));
        assert_eq!(m("status.rs", "xyz"), None);
    }

    #[test]
    fn otherwise_pieces_match_at_word_starts() {
        assert_eq!(m("status.rs", "stRs").as_deref(), Some("ST.....RS"));
        assert_eq!(m("MyFileBrowser.java", "mfb").as_deref(), Some("M.F...B..........."));
        assert_eq!(m("file_browser_test", "fbt").as_deref(), Some("F....B.......T..."));
        assert_eq!(m("Cargo.toml", "ct").as_deref(), Some("C.....T..."));
        // Not at a word start: no match.
        assert_eq!(m("status.rs", "sa"), None);
    }

    #[test]
    fn a_longer_first_piece_gives_way_when_the_rest_needs_it() {
        // `ab` first would leave `d`, which isn't at a word start; `a` + `bd` fits.
        assert_eq!(m("abc_bd", "abd").as_deref(), Some("A...BD"));
        assert_eq!(m("file_browser", "fileb").as_deref(), Some("FILE.B......"));
    }
}
