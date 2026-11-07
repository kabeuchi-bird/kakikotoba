//! 文字数。記法を除いた本文の字を、見た目の1字（書記素クラスタ）ごとに数える。

use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

use crate::ast::{BlockKind, Document, Inline, InlineKind};

/// 本文の見た目の1字ごとの、元ソースでの位置（読む順）。行頭の空白は含めない。
/// ルビは親文字だけを、ルビ全体の先頭の位置で持つ。
pub struct BodyText {
    starts: Vec<usize>,
}

impl BodyText {
    pub fn new(document: &Document) -> Self {
        let mut body = Collector { text: String::new(), positions: Vec::new(), line_start: true };
        for block in &document.blocks {
            body.line_start = true;
            if let BlockKind::Horizontal { lines } = &block.kind {
                for line in lines {
                    body.line_start = true;
                    line.chars().for_each(|c| body.push(block.span.start, c));
                }
            }
            body.push_inlines(&block.inlines);
        }
        // 字の位置をバイト位置から引けるよう、字ごとの番号を数えながら進める
        let mut chars = body.text.char_indices().map(|(byte, _)| byte).enumerate();
        let starts = body
            .text
            .grapheme_indices(true)
            .map(|(byte, _)| {
                let (k, _) = chars.find(|&(_, b)| b == byte).expect("書記素の先頭は字の先頭");
                body.positions[k]
            })
            .collect();
        Self { starts }
    }

    /// 元ソースの範囲（文字単位）に先頭がある字の数。
    pub fn count(&self, range: Range<usize>) -> usize {
        self.starts.partition_point(|&at| at < range.end) - self.starts.partition_point(|&at| at < range.start)
    }

    /// 本文全体の字数。
    pub fn total(&self) -> usize {
        self.starts.len()
    }
}

/// 組み立て中の本文。字と、その元ソースでの位置を並べる。
struct Collector {
    text: String,
    positions: Vec<usize>,
    line_start: bool,
}

impl Collector {
    fn push_inlines(&mut self, inlines: &[Inline]) {
        for inline in inlines {
            let at = inline.span.start;
            match &inline.kind {
                InlineKind::Text(text) => text.chars().enumerate().for_each(|(k, c)| self.push(at + k, c)),
                InlineKind::Ruby { base, .. } => base.chars().for_each(|c| self.push(at, c)),
                InlineKind::Emphasis(children) | InlineKind::Sideline(children) | InlineKind::Bold(children) => {
                    self.push_inlines(children)
                }
                InlineKind::LineBreak => self.line_start = true,
            }
        }
    }

    fn push(&mut self, at: usize, c: char) {
        if self.line_start && c.is_whitespace() {
            return;
        }
        self.line_start = false;
        self.text.push(c);
        self.positions.push(at);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::highlight::{Dialect, Options};

    fn body(text: &str, dialect: Dialect) -> BodyText {
        BodyText::new(&crate::parse(text, Options { dialect, symbol_forms: true }))
    }

    #[test]
    fn counts_body_without_markup_leading_spaces_and_line_breaks() {
        // ルビは親文字だけ、行頭の全角・半角空白は数えない。行中の空白は数える
        let text = "#TITLE#題\n　|漢字《かんじ》の 本\n !コメントではない\n!コメント\n《《傍点》》";
        let b = body(text, Dialect::PreTnf);
        assert_eq!(b.total(), "漢字の 本".chars().count() + "!コメントではない".chars().count() + 2);
        // 選択範囲は元ソースの位置で数える。ルビは範囲に先頭が入れば親文字ごと数える
        let line = text.find('　').map(|i| text[..i].chars().count()).unwrap();
        assert_eq!(b.count(line..line + 2), 2);
        assert_eq!(b.count(0..line), 0);
    }

    #[test]
    fn one_visible_character_counts_once() {
        let b = body("葛\u{e0100}👍🏽🇯🇵👨\u{200d}👩\u{200d}👧が\u{3099}", Dialect::PreTnf);
        assert_eq!(b.total(), 5);
    }

    #[test]
    fn markdown_marks_are_not_counted() {
        let b = body("# 見出し\n\n**太字**と`code`\n\n- 項目\n  続き", Dialect::Markdown);
        assert_eq!(b.total(), "見出し太字とcode項目続き".chars().count());
    }
}
