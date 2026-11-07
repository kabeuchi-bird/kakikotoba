//! 英字（ラテン、ギリシャ、キリル）の判定。欧文の向き、英単語の折り返し、Undoの単位で同じものを使う。

/// 英字か。各ブロックの字母と、語の中の結合文字（アクセントなど）。全角の英字（U+FF00〜）は含めない。
pub fn is_letter(c: char) -> bool {
    c.is_alphabetic() && in_blocks(c) || is_combining_mark(c)
}

/// アクセントなどを付ける結合文字（ダイアクリティカルマーク）。
fn is_combining_mark(c: char) -> bool {
    matches!(c as u32, 0x300..=0x36f | 0x1ab0..=0x1aff | 0x1dc0..=0x1dff | 0x20d0..=0x20ff | 0xfe20..=0xfe2f)
}

/// ラテン、ギリシャ、キリルの各ブロックか（Unicodeのブロック一覧に従う）。
fn in_blocks(c: char) -> bool {
    matches!(c as u32,
        // ラテン: 基本・Latin-1補助・拡張A/B、IPA、前進を伴う修飾字、音声記号（拡張・補助を含む）
        0x0000..=0x02ff | 0x1d00..=0x1dbf
        // ラテン: 拡張追加、拡張C/D/E、合字（ﬀ ﬁ など）、拡張F/G
        | 0x1e00..=0x1eff | 0x2c60..=0x2c7f | 0xa720..=0xa7ff | 0xab30..=0xab6f | 0xfb00..=0xfb06
        | 0x10780..=0x107bf | 0x1df00..=0x1dfff
        // ギリシャ（拡張を含む）
        | 0x0370..=0x03ff | 0x1f00..=0x1fff
        // キリル（補助、拡張A/B/C/Dを含む）
        | 0x0400..=0x052f | 0x1c80..=0x1c8f | 0x2de0..=0x2dff | 0xa640..=0xa69f | 0x1e030..=0x1e08f)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letters_of_every_block_and_combining_marks_but_not_fullwidth() {
        for c in ['a', 'ệ', 'ⱥ', 'ꝛ', 'ﬁ', 'ʰ', 'ω', 'ἄ', 'ж', 'ԁ', 'ᲀ', 'ꙁ', '\u{301}'] {
            assert!(is_letter(c), "{c} は英字");
        }
        for c in ['1', '-', ' ', 'Ａ', 'あ', '漢', '×'] {
            assert!(!is_letter(c), "{c} は英字ではない");
        }
    }
}
