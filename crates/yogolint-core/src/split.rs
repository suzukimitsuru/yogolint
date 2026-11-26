//! 識別子を単語に分割する(cspell と同じく 区切り文字 → 大小文字の境界 → 数字 の順)。

/// 分割した1語。範囲は識別子の中のバイト位置。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Word {
    /// 小文字にした語
    pub text: String,
    pub start: usize,
    pub end: usize,
}

const SEPARATORS: [u8; 4] = *b"_-$.";

/// 前後の `_` `$` と、Rust の生識別子の `r#` を除いた範囲を返す。
pub fn trim_identifier(identifier: &str) -> (usize, usize) {
    let bytes = identifier.as_bytes();
    let mut start = 0;
    if bytes.starts_with(b"r#") {
        start = 2;
    }
    let mut end = bytes.len();
    while start < end && matches!(bytes[start], b'_' | b'$') {
        start += 1;
    }
    while end > start && matches!(bytes[end - 1], b'_' | b'$') {
        end -= 1;
    }
    (start, end)
}

/// 識別子を語に分割する。`digit_words` は数字込みで辞書に登録された語(小文字、長い順)。
///
/// 識別子は ASCII であること(英字以外を含む識別子は呼び出し側で除く)。
pub fn split_identifier(identifier: &str, digit_words: &[String]) -> Vec<Word> {
    let (start, end) = trim_identifier(identifier);
    let bytes = identifier.as_bytes();
    let mut words = Vec::new();
    let mut segment_start = start;
    for index in start..=end {
        if bytes.get(index).is_none_or(|byte| SEPARATORS.contains(byte)) || index == end {
            if segment_start < index {
                split_segment(identifier, segment_start, index, digit_words, &mut words);
            }
            segment_start = index + 1;
        }
    }
    words
}

/// 区切り文字の無い1区間を分割する。数字込みの辞書語を先に丸ごと取り出す。
fn split_segment(identifier: &str, start: usize, end: usize, digit_words: &[String], words: &mut Vec<Word>) {
    let bytes = identifier.as_bytes();
    let mut piece_start = start;
    let mut index = start;
    while index < end {
        if let Some(length) = match_digit_word(bytes, index, start, end, digit_words) {
            split_case(identifier, piece_start, index, words);
            words.push(Word {
                text: identifier[index..index + length].to_ascii_lowercase(),
                start: index,
                end: index + length,
            });
            index += length;
            piece_start = index;
        } else {
            index += 1;
        }
    }
    split_case(identifier, piece_start, end, words);
}

/// `index` の位置から数字込みの辞書語が語境界で一致すれば、その長さを返す。
fn match_digit_word(bytes: &[u8], index: usize, start: usize, end: usize, digit_words: &[String]) -> Option<usize> {
    let at_boundary = index == start
        || bytes[index].is_ascii_uppercase()
        || bytes[index - 1].is_ascii_digit()
        || (bytes[index - 1].is_ascii_lowercase() && bytes[index].is_ascii_digit());
    if !at_boundary {
        return None;
    }
    digit_words.iter().map(String::as_bytes).find_map(|word| {
        let stop = index + word.len();
        if stop > end || !bytes[index..stop].eq_ignore_ascii_case(word) {
            return None;
        }
        let ends_at_boundary = stop == end || bytes[stop].is_ascii_uppercase() || !bytes[stop].is_ascii_alphanumeric();
        // 小文字だけの続き(utf8octets)は、語の最後が数字なら境界とみなす
        let digit_then_lower = bytes[stop - 1].is_ascii_digit() && bytes[stop].is_ascii_lowercase();
        (ends_at_boundary || digit_then_lower).then_some(word.len())
    })
}

/// 大小文字の境界で分割し、数字は区切りとして捨てる。
fn split_case(identifier: &str, start: usize, end: usize, words: &mut Vec<Word>) {
    let bytes = identifier.as_bytes();
    let mut token_start: Option<usize> = None;
    let push = |from: usize, to: usize, words: &mut Vec<Word>| {
        if from < to {
            words.push(Word {
                text: identifier[from..to].to_ascii_lowercase(),
                start: from,
                end: to,
            });
        }
    };
    for index in start..end {
        let current = bytes[index];
        if !current.is_ascii_alphabetic() {
            if let Some(from) = token_start.take() {
                push(from, index, words);
            }
            continue;
        }
        match token_start {
            None => token_start = Some(index),
            Some(from) => {
                let previous = bytes[index - 1];
                if previous.is_ascii_lowercase() && current.is_ascii_uppercase() {
                    // parseJson → parse / Json
                    push(from, index, words);
                    token_start = Some(index);
                } else if previous.is_ascii_uppercase() && current.is_ascii_lowercase() && index - 1 > from {
                    // HTMLInput → HTML / Input
                    push(from, index - 1, words);
                    token_start = Some(index - 1);
                }
            }
        }
    }
    if let Some(from) = token_start {
        push(from, end, words);
    }
}

/// 辞書に無い塊を、辞書の語の連結として分解する(語数が最小、1語は2文字以上)。
/// 語数が同じなら、長い語を含む分け方(語長の二乗和が大きいもの)を採る。
pub fn decompose_compound(text: &str, is_word: impl Fn(&str) -> bool) -> Option<Vec<String>> {
    let length = text.len();
    // best[i] = text[..i] を分解したときの (語数, 語長の二乗和, 最後の語の開始位置)
    let mut best: Vec<Option<(usize, usize, usize)>> = vec![None; length + 1];
    best[0] = Some((0, 0, 0));
    for stop in 2..=length {
        for from in 0..=stop - 2 {
            let Some((count, score, _)) = best[from] else { continue };
            if !is_word(&text[from..stop]) {
                continue;
            }
            let candidate = (count + 1, score + (stop - from).pow(2), from);
            let better = best[stop].is_none_or(|(current_count, current_score, _)| {
                candidate.0 < current_count || (candidate.0 == current_count && candidate.1 > current_score)
            });
            if better {
                best[stop] = Some(candidate);
            }
        }
    }
    let (count, _, _) = best[length]?;
    if count < 2 {
        return None;
    }
    let mut parts = Vec::with_capacity(count);
    let mut stop = length;
    while stop > 0 {
        let (_, _, from) = best[stop]?;
        parts.push(text[from..stop].to_string());
        stop = from;
    }
    parts.reverse();
    Some(parts)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(identifier: &str) -> Vec<String> {
        let digit_words = ["utf8", "i18n", "md5"].map(String::from).to_vec();
        split_identifier(identifier, &digit_words)
            .into_iter()
            .map(|word| word.text)
            .collect()
    }

    #[test]
    fn spec_examples() {
        assert_eq!(texts("parseJson"), ["parse", "json"]);
        assert_eq!(texts("HTMLInput"), ["html", "input"]);
        assert_eq!(texts("getUtf8Octets"), ["get", "utf8", "octets"]);
        assert_eq!(texts("MAX_RETRY_COUNT"), ["max", "retry", "count"]);
        assert_eq!(texts("__init__"), ["init"]);
        assert_eq!(texts("camel2snake"), ["camel", "snake"]);
    }

    #[test]
    fn snake_case_and_raw_identifiers() {
        assert_eq!(texts("get_utf8_octets"), ["get", "utf8", "octets"]);
        assert_eq!(texts("r#type"), ["type"]);
        assert_eq!(texts("utf8octets"), ["utf8", "octets"]);
        assert_eq!(texts("butf8"), ["butf"]);
    }

    #[test]
    fn keeps_offsets() {
        let words = split_identifier("_parseJson", &[]);
        assert_eq!((words[1].start, words[1].end), (6, 10));
    }

    #[test]
    fn decomposes_compound_words() {
        let dictionary = ["save", "file", "as", "fil", "eas"];
        let is_word = |word: &str| dictionary.contains(&word);
        assert_eq!(
            decompose_compound("savefileas", is_word).unwrap(),
            ["save", "file", "as"]
        );
        assert_eq!(decompose_compound("save", is_word), None);
        assert_eq!(decompose_compound("savex", is_word), None);
    }
}
