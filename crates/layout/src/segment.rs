//! 編集モードの文字の向きの判定。

use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Orientation {
    Upright,
    /// 縦中横
    Tcy,
    /// 90°回転
    Rotated,
}

const TCY_DIGITS: Range<usize> = 2..4;

/// 欧文の連続に含める文字。全角文字は含まない。
fn is_western(c: char) -> bool {
    matches!(c, ' '..='~' | '\u{A1}'..='\u{2FF}' | '\u{370}'..='\u{52F}')
}

fn classify(run: &str) -> Orientation {
    if run.bytes().all(|b| b.is_ascii_digit()) {
        if TCY_DIGITS.contains(&run.len()) {
            return Orientation::Tcy;
        }
        if run.len() < TCY_DIGITS.start {
            return Orientation::Upright;
        }
    }
    if run.chars().any(char::is_alphanumeric) { Orientation::Rotated } else { Orientation::Upright }
}

/// 文字列を向きごとの区間に分ける。区間はバイト単位で、全体を隙間なく覆う。
pub(crate) fn segments(text: &str) -> Vec<(Orientation, Range<usize>)> {
    let mut segments: Vec<(Orientation, Range<usize>)> = Vec::new();
    let mut push = |orientation: Orientation, range: Range<usize>| {
        if range.is_empty() {
            return;
        }
        match segments.last_mut() {
            // 同じ向きでも縦中横と回転は単位が変わるので、正立だけつなげる
            Some((Orientation::Upright, last)) if orientation == Orientation::Upright => last.end = range.end,
            _ => segments.push((orientation, range)),
        }
    };
    let mut chars = text.char_indices().peekable();
    while let Some(&(start, c)) = chars.peek() {
        let western = is_western(c);
        let mut end = start;
        while let Some(&(position, c)) = chars.peek() {
            if is_western(c) != western {
                break;
            }
            end = position + c.len_utf8();
            chars.next();
        }
        if !western {
            push(Orientation::Upright, start..end);
            continue;
        }
        // 連続の前後の空白は回転させない
        let run = &text[start..end];
        let trimmed = run.trim_matches(' ');
        let run_start = start + (run.len() - run.trim_start_matches(' ').len());
        let run_end = run_start + trimmed.len();
        push(Orientation::Upright, start..run_start);
        push(classify(trimmed), run_start..run_end);
        push(Orientation::Upright, run_end..end);
    }
    segments
}

#[cfg(test)]
mod tests {
    use super::*;
    use Orientation::*;

    fn parts(text: &str) -> Vec<(Orientation, &str)> {
        segments(text).into_iter().map(|(o, range)| (o, &text[range])).collect()
    }

    #[test]
    fn digits_by_length() {
        assert_eq!(parts("第1話"), [(Upright, "第1話")]);
        assert_eq!(parts("第12話"), [(Upright, "第"), (Tcy, "12"), (Upright, "話")]);
        assert_eq!(parts("第123話"), [(Upright, "第"), (Tcy, "123"), (Upright, "話")]);
        assert_eq!(parts("2024年"), [(Rotated, "2024"), (Upright, "年")]);
    }

    #[test]
    fn fullwidth_is_never_western() {
        assert_eq!(parts("１２３ＡＢＣ"), [(Upright, "１２３ＡＢＣ")]);
    }

    #[test]
    fn latin_runs_rotate_with_inner_spaces() {
        assert_eq!(parts("あHello Worldい"), [(Upright, "あ"), (Rotated, "Hello World"), (Upright, "い")]);
        assert_eq!(parts("A12"), [(Rotated, "A12")]);
        assert_eq!(parts("12%"), [(Rotated, "12%")]);
        assert_eq!(parts("Ωμέγα"), [(Rotated, "Ωμέγα")]);
    }

    #[test]
    fn surrounding_spaces_and_punctuation_only_runs_stay_upright() {
        assert_eq!(parts("あ abc い"), [(Upright, "あ "), (Rotated, "abc"), (Upright, " い")]);
        assert_eq!(parts("あ!い"), [(Upright, "あ!い")]);
        assert_eq!(parts("!?"), [(Upright, "!?")]);
        assert_eq!(parts("  "), [(Upright, "  ")]);
    }

    #[test]
    fn segments_cover_the_whole_text() {
        for text in ["", "あ", "a", "12 34", "あ 1 い", "x\ty", " 12 "] {
            let mut position = 0;
            for (_, range) in segments(text) {
                assert_eq!(range.start, position, "{text:?}");
                position = range.end;
            }
            assert_eq!(position, text.len(), "{text:?}");
        }
    }
}
