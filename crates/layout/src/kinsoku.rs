//! 禁則処理（追い出し方式）。ぶら下げは次段階。

fn forbidden_at_start(c: char) -> bool {
    matches!(c,
        '、' | '。' | '，' | '．' | '・' | '：' | '；' | '？' | '！' | '‼' | '⁇' | '⁈' | '⁉'
        | '）' | '］' | '｝' | '〕' | '〉' | '》' | '」' | '』' | '】' | '〙' | '〗' | '〟' | '’' | '”' | '»'
        | 'ヽ' | 'ヾ' | 'ゝ' | 'ゞ' | '々' | '〻' | 'ー' | '‐' | '゠' | '–' | '〜'
        | 'ぁ' | 'ぃ' | 'ぅ' | 'ぇ' | 'ぉ' | 'っ' | 'ゃ' | 'ゅ' | 'ょ' | 'ゎ' | 'ゕ' | 'ゖ'
        | 'ァ' | 'ィ' | 'ゥ' | 'ェ' | 'ォ' | 'ッ' | 'ャ' | 'ュ' | 'ョ' | 'ヮ' | 'ヵ' | 'ヶ'
        | '\u{31F0}'..='\u{31FF}'
        | ')' | ']' | '}' | ',' | '.' | '!' | '?' | ':' | ';')
}

fn forbidden_at_end(c: char) -> bool {
    matches!(c,
        '（' | '［' | '｛' | '〔' | '〈' | '《' | '「' | '『' | '【' | '〘' | '〖' | '〝' | '‘' | '“' | '«'
        | '(' | '[' | '{')
}

fn hangs(c: char) -> bool {
    matches!(c, '、' | '。' | '，' | '．')
}

/// 単語の途中の分割を判定する対象の文字（ラテン、ギリシャ、キリルの英字）。
fn is_word_letter(c: char) -> bool {
    c.is_alphabetic() && c < '\u{530}'
}

/// 2つのクラスタの間で分けると、単語の途中になるか。
pub(crate) fn splits_word(before: char, after: char) -> bool {
    is_word_letter(before) && is_word_letter(after)
}

/// 単語の前後に残す最小の文字数
const MIN_WORD_PART: usize = 2;

/// 折り返しの方式。編集モードだけが使う。
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Wrap {
    /// 行末の句読点を列の外へぶら下げる
    pub hanging: bool,
    /// 単語の途中で分けるときに列末へ置くハイフンの送り量
    pub hyphen: Option<f32>,
}

/// 単語の途中で分けるなら、ハイフンが収まり前後に文字が残る位置へずらす。
fn adjust_word_break(clusters: &[(char, char, f32)], start: usize, fit: usize, limit: f32, hyphen: f32) -> usize {
    if fit == start || fit == clusters.len() || !splits_word(clusters[fit - 1].1, clusters[fit].0) {
        return fit;
    }
    let mut word_start = fit;
    while word_start > start && is_word_letter(clusters[word_start - 1].1) {
        word_start -= 1;
    }
    let mut word_end = fit;
    while word_end < clusters.len() && is_word_letter(clusters[word_end].0) {
        word_end += 1;
    }
    let usable = (word_start + 1..=fit).rev().find(|&end| {
        let length: f32 = clusters[start..end].iter().map(|c| c.2).sum();
        end - word_start >= MIN_WORD_PART && word_end - end >= MIN_WORD_PART && length + hyphen <= limit
    });
    match usable {
        Some(end) => end,
        None if word_start > start => word_start,
        // 列全体が1語で分けようがない
        None => fit,
    }
}

/// クラスタ列 `(先頭文字, 末尾文字, 送り量)` を `limit` に収まるよう列に分け、各列の終端位置を返す。
pub(crate) fn break_columns(clusters: &[(char, char, f32)], limit: f32, wrap: Wrap) -> Vec<usize> {
    // 字数×字送りの累積が浮動小数点誤差で溢れないように
    let limit = limit * (1.0 + 1e-4);
    let mut ends = Vec::new();
    let mut start = 0;
    while start < clusters.len() {
        let mut total = 0.0;
        let mut fit = start;
        while fit < clusters.len() && (fit == start || total + clusters[fit].2 <= limit) {
            total += clusters[fit].2;
            fit += 1;
        }
        if let Some(hyphen) = wrap.hyphen {
            fit = adjust_word_break(clusters, start, fit, limit, hyphen);
        }
        let mut end = fit;
        if wrap.hanging
            && fit > start
            && clusters.get(fit).is_some_and(|c| hangs(c.0))
            && !clusters.get(fit + 1).is_some_and(|c| forbidden_at_start(c.0))
        {
            end = fit + 1;
        } else if fit < clusters.len() {
            while end > start && (forbidden_at_start(clusters[end].0) || forbidden_at_end(clusters[end - 1].1)) {
                end -= 1;
            }
            // 列全体が禁則文字なら追い出しようがないので、詰めるだけ詰める
            if end == start {
                end = fit;
            }
        }
        ends.push(end);
        start = end;
    }
    ends
}

#[cfg(test)]
mod tests {
    use super::*;

    fn breaks(text: &str, per_column: usize) -> Vec<String> {
        let chars: Vec<char> = text.chars().collect();
        let clusters: Vec<_> = chars.iter().map(|&c| (c, c, 1.0)).collect();
        let mut start = 0;
        break_columns(&clusters, per_column as f32, Wrap::default())
            .into_iter()
            .map(|end| {
                let column = chars[start..end].iter().collect();
                start = end;
                column
            })
            .collect()
    }

    #[test]
    fn plain_text_fills_columns() {
        assert_eq!(breaks("あいうえおか", 3), ["あいう", "えおか"]);
        assert_eq!(breaks("あ", 3), ["あ"]);
        assert!(breaks("", 3).is_empty());
    }

    #[test]
    fn pushes_out_forbidden_start() {
        assert_eq!(breaks("あいう。え", 3), ["あい", "う。え"]);
        assert_eq!(breaks("あいう」。え", 3), ["あい", "う」。", "え"]);
    }

    #[test]
    fn pushes_out_forbidden_end() {
        assert_eq!(breaks("あい「うえ", 3), ["あい", "「うえ"]);
    }

    #[test]
    fn forbidden_only_column_still_terminates() {
        assert_eq!(breaks("。。。。", 3), ["。。。", "。"]);
    }

    #[test]
    fn oversized_cluster_gets_own_column() {
        let clusters = [('あ', 'あ', 5.0), ('い', 'い', 1.0)];
        assert_eq!(break_columns(&clusters, 3.0, Wrap::default()), [1, 2]);
    }

    fn hanging_ends(text: &str, per_column: usize) -> Vec<usize> {
        let clusters: Vec<_> = text.chars().map(|c| (c, c, 1.0)).collect();
        break_columns(&clusters, per_column as f32, Wrap { hanging: true, hyphen: None })
    }

    fn hyphen_ends(text: &str, per_column: usize) -> Vec<usize> {
        let clusters: Vec<_> = text.chars().map(|c| (c, c, 1.0)).collect();
        break_columns(&clusters, per_column as f32, Wrap { hanging: true, hyphen: Some(0.5) })
    }

    #[test]
    fn word_break_leaves_room_for_the_hyphen() {
        // 5字目まで入るが、ハイフンの分だけ前へずれる
        assert_eq!(hyphen_ends("abcdefgh", 5), [4, 8]);
        // ハイフンが不要な位置はそのまま
        assert_eq!(hyphen_ends("abcde fgh", 5), [5, 9]);
        assert_eq!(hyphen_ends("abcde1fgh", 5), [5, 9]);
    }

    #[test]
    fn word_break_keeps_two_letters_on_each_side() {
        // 次の列に1字だけ残さない
        assert_eq!(hyphen_ends("abcdefg", 6), [5, 7]);
        // 列末に1字だけ残さず、単語ごと送る
        assert_eq!(hyphen_ends("abcd efgh", 6), [5, 9]);
    }

    #[test]
    fn word_without_room_to_split_moves_whole_and_long_word_still_terminates() {
        assert_eq!(hyphen_ends("あいうab", 4), [3, 5]);
        assert_eq!(hyphen_ends("abcdefghij", 3), [2, 4, 6, 8, 10]);
        assert_eq!(hyphen_ends("abc", 1), [1, 2, 3]);
    }

    #[test]
    fn only_letters_of_western_scripts_split_words() {
        assert!(splits_word('a', 'b'));
        assert!(splits_word('Ω', 'д'));
        assert!(!splits_word('a', '1'));
        assert!(!splits_word('a', ' '));
        assert!(!splits_word('あ', 'い'));
    }

    #[test]
    fn punctuation_hangs_past_the_limit() {
        assert_eq!(hanging_ends("あいう。え", 3), [4, 5]);
        assert_eq!(hanging_ends("あいう、", 3), [4]);
        // 収まる位置の句読点はそのまま
        assert_eq!(hanging_ends("あい。うえ", 3), [3, 5]);
    }

    #[test]
    fn hanging_falls_back_when_next_char_is_forbidden() {
        assert_eq!(hanging_ends("あいう。」え", 3), [2, 5, 6]);
        // ぶら下げ対象でない行頭禁則文字は追い出し
        assert_eq!(hanging_ends("あいう！え", 3), [2, 5]);
    }
}
