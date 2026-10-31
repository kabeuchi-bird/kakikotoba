//! 共通AST。spanはすべて元ソースの文字（char）単位。

use std::ops::Range;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Document {
    pub metadata: Vec<Metadata>,
    pub blocks: Vec<Block>,
}

/// 本文に描画しない文章情報。
#[derive(Clone, Debug, PartialEq)]
pub struct Metadata {
    pub label: &'static str,
    pub value: String,
    pub span: Range<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub kind: BlockKind,
    pub inlines: Vec<Inline>,
    pub span: Range<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum BlockKind {
    Paragraph,
    /// 見出し（章）
    Chapter,
    /// 強調（大きい字）
    Strong,
    /// ラインセンタリング
    Center,
    /// 地付き
    Under,
    /// リスト項目。`depth` は字下げの段数
    ListItem { depth: usize },
    PageBreak,
    PageCenterStart,
    PageCenterEnd,
    Image { align: ImageAlign, source: String },
    /// 水平線
    Rule,
    /// 横組みブロック（コードブロック、表）
    Horizontal { lines: Vec<String> },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageAlign {
    Left,
    Right,
    Next,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Inline {
    pub kind: InlineKind,
    pub span: Range<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum InlineKind {
    /// 文字と元ソースが1対1で対応する文字列
    Text(String),
    Ruby { base: String, ruby: String },
    /// 傍点
    Emphasis(Vec<Inline>),
    /// 傍線
    Sideline(Vec<Inline>),
    /// 太字（ゴシック）
    Bold(Vec<Inline>),
    LineBreak,
}
