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

/// クラスタ列 `(先頭文字, 末尾文字, 送り量)` を `limit` に収まるよう列に分け、各列の終端位置を返す。
pub(crate) fn break_columns(clusters: &[(char, char, f32)], limit: f32) -> Vec<usize> {
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
        let mut end = fit;
        if fit < clusters.len() {
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
        break_columns(&clusters, per_column as f32)
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
        assert_eq!(break_columns(&clusters, 3.0), [1, 2]);
    }
}
