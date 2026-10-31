//! PreTNFの構文。ハイライトとAST解析で共用する。

use std::collections::HashMap;
use std::ops::Range;

use crate::ast::{Block, BlockKind, Document, ImageAlign, Inline, InlineKind, Metadata};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Tag {
    Meta(&'static str),
    Start,
    End,
    TnfStart,
    TnfEnd,
    Chapter,
    NewPage,
    Image(ImageAlign),
    Center,
    Under,
    PageCenterStart,
    PageCenterEnd,
}

/// `#〜#` タグの名前。文中の `#1` などと衝突させないため、リファレンスにあるものだけを認める。
fn tag(name: &str) -> Option<Tag> {
    use Tag::*;
    Some(match name {
        "TITLE" | "タイトル" => Meta("タイトル"),
        "VOLUME" | "巻数" => Meta("巻数"),
        "WRITER" | "筆者" => Meta("筆者"),
        "WRITER:URL" | "筆者:URL" => Meta("筆者URL"),
        "WRITER:MAIL" | "筆者:MAIL" => Meta("筆者メール"),
        "ILLUSTRATOR" | "絵師" => Meta("絵師"),
        "ILLUSTRATOR:URL" | "絵師:URL" => Meta("絵師URL"),
        "ILLUSTRATOR:MAIL" | "絵師:MAIL" => Meta("絵師メール"),
        "CIRCLE" | "サークル" => Meta("サークル"),
        "THANKS" | "協力" => Meta("協力者"),
        "EDITION" | "改版履歴" => Meta("改版履歴"),
        "PRINTSHOP" | "印刷所" => Meta("印刷所"),
        "COVER" | "表紙" => Meta("表紙"),
        "MIDDLECOVER" | "中表紙" => Meta("中表紙"),
        "FRONTISPIECE" | "口絵" => Meta("口絵"),
        "BACKCOVER" | "裏表紙" => Meta("裏表紙"),
        "MIDDLEBACKGROUND" | "中表紙背景" => Meta("中表紙背景"),
        "ILLUSTRATORBACKGROUND" | "絵師背景" => Meta("絵師背景"),
        "INDEXBACKGROUND" | "目次背景" => Meta("目次背景"),
        "RIGHTPAGEBACKGROUND" | "右ページ背景" => Meta("右ページ背景"),
        "LEFTPAGEBACKGROUND" | "左ページ背景" => Meta("左ページ背景"),
        "PUBLISHERBACKGROUND" | "奥付背景" => Meta("奥付背景"),
        "START" | "開始" => Start,
        "END" | "終了" => End,
        "TNF:START" => TnfStart,
        "TNF:END" => TnfEnd,
        "CHAPTER" | "章" => Chapter,
        // 改段は段組みを持たないので改ページとして扱う
        "NEWPAGE" | "改頁" | "改ページ" | "NEWSTEP" | "改段" => NewPage,
        "IMG:L" | "挿絵:左" => Image(ImageAlign::Left),
        "IMG:R" | "挿絵:右" => Image(ImageAlign::Right),
        "IMG:N" | "挿絵:次" => Image(ImageAlign::Next),
        "CENTER" | "中央配置" | "中央" => Center,
        "UNDER" | "地付き" => Under,
        "中央配置:開始" => PageCenterStart,
        "中央配置:終了" => PageCenterEnd,
        _ => return None,
    })
}

/// 行頭の記法。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Head {
    None,
    /// `#〜#`。`end` はタグの次の位置
    Tag { tag: Tag, end: usize },
    /// `[newpage]`
    NewPage,
    /// `[chapter:〜]`。`end` は閉じ括弧の次の位置
    PixivChapter { end: usize },
    /// `!`
    Comment,
    /// 単独行の `:` `{` `}`
    Alone(char),
    /// `-` `$` `+` `*` `%` `;`
    Prefix(char),
}

const CHAPTER_OPEN: usize = 9;

pub(crate) fn line_head(chars: &[char], symbol_forms: bool) -> Head {
    if chars.first() == Some(&'#')
        && let Some(end) = find(chars, &['#'], 1..chars.len())
        && let Some(tag) = tag(&chars[1..end].iter().collect::<String>())
    {
        return Head::Tag { tag, end: end + 1 };
    }
    if starts_with(chars, 0, "[newpage]") && chars.len() == 9 {
        return Head::NewPage;
    }
    if starts_with(chars, 0, "[chapter:")
        && let Some(end) = find(chars, &[']'], CHAPTER_OPEN..chars.len())
        && end > CHAPTER_OPEN
    {
        return Head::PixivChapter { end: end + 1 };
    }
    if !symbol_forms {
        return Head::None;
    }
    match chars.first() {
        Some('!') => Head::Comment,
        Some(&c @ (':' | '{' | '}')) if chars.len() == 1 => Head::Alone(c),
        Some(&c @ ('-' | '$' | '+' | '*' | '%' | ';')) if chars.len() > 1 => Head::Prefix(c),
        _ => Head::None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Style {
    Emphasis,
    Sideline,
}

/// 行内の要素。範囲は渡した文字列での位置。
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Node {
    Text(Range<usize>),
    Ruby { whole: Range<usize>, base: Range<usize>, ruby: Range<usize> },
    /// `[ベース文字]` 単独（rubymap）
    RubyRef { whole: Range<usize>, base: Range<usize> },
    Styled { whole: Range<usize>, style: Style, children: Vec<Node> },
}

pub(crate) fn find(chars: &[char], target: &[char], range: Range<usize>) -> Option<usize> {
    chars[range.clone()].windows(target.len()).position(|window| window == target).map(|offset| range.start + offset)
}

fn starts_with(chars: &[char], at: usize, target: &str) -> bool {
    let mut position = at;
    for c in target.chars() {
        if chars.get(position) != Some(&c) {
            return false;
        }
        position += 1;
    }
    true
}

pub(crate) fn parse_inline(chars: &[char], range: Range<usize>, symbol_forms: bool) -> Vec<Node> {
    let mut nodes = Vec::new();
    let mut text_start = range.start;
    let mut i = range.start;
    let end = range.end;
    while i < end {
        let node = pixiv_ruby(chars, i, end)
            .or_else(|| bracket_ruby(chars, i, end))
            .or_else(|| bar_ruby(chars, i, end))
            .or_else(|| double_angle_emphasis(chars, i, end, symbol_forms))
            .or_else(|| quoted(chars, i, end, symbol_forms));
        let Some((node, next)) = node else {
            i += 1;
            continue;
        };
        if text_start < i {
            nodes.push(Node::Text(text_start..i));
        }
        nodes.push(node);
        i = next;
        text_start = next;
    }
    if text_start < end {
        nodes.push(Node::Text(text_start..end));
    }
    nodes
}

/// `[[rb:ベース > ルビ]]`
fn pixiv_ruby(chars: &[char], i: usize, end: usize) -> Option<(Node, usize)> {
    if !starts_with(chars, i, "[[rb:") || i + 5 > end {
        return None;
    }
    let body = i + 5;
    let close = find(chars, &[']', ']'], body..end)?;
    let separator = find(chars, &['>'], body..close)?;
    let base_end = separator - chars[body..separator].iter().rev().take_while(|c| c.is_whitespace()).count();
    let ruby_start = separator + 1 + chars[separator + 1..close].iter().take_while(|c| c.is_whitespace()).count();
    if base_end == body || ruby_start == close {
        return None;
    }
    let node = Node::Ruby { whole: i..close + 2, base: body..base_end, ruby: ruby_start..close };
    Some((node, close + 2))
}

/// `[ベース|ルビ]`、`[ベース]`
fn bracket_ruby(chars: &[char], i: usize, end: usize) -> Option<(Node, usize)> {
    if chars[i] != '[' {
        return None;
    }
    let close = find(chars, &[']'], i + 1..end)?;
    if close == i + 1 {
        return None;
    }
    let Some(bar) = find(chars, &['|'], i + 1..close) else {
        return Some((Node::RubyRef { whole: i..close + 1, base: i + 1..close }, close + 1));
    };
    if bar == i + 1 || bar + 1 == close {
        return None;
    }
    Some((Node::Ruby { whole: i..close + 1, base: i + 1..bar, ruby: bar + 1..close }, close + 1))
}

/// `|ベース《ルビ》`、`｜ベース《ルビ》`
pub(crate) fn bar_ruby(chars: &[char], i: usize, end: usize) -> Option<(Node, usize)> {
    if !matches!(chars[i], '|' | '｜') {
        return None;
    }
    let open = find(chars, &['《'], i + 1..end)?;
    let close = find(chars, &['》'], open + 1..end)?;
    if open == i + 1 || close == open + 1 {
        return None;
    }
    Some((Node::Ruby { whole: i..close + 1, base: i + 1..open, ruby: open + 1..close }, close + 1))
}

/// `《《傍点》》`
fn double_angle_emphasis(chars: &[char], i: usize, end: usize, symbol_forms: bool) -> Option<(Node, usize)> {
    if !starts_with(chars, i, "《《") || i + 2 > end {
        return None;
    }
    let close = find(chars, &['》', '》'], i + 2..end)?;
    if close == i + 2 {
        return None;
    }
    let children = parse_inline(chars, i + 2..close, symbol_forms);
    Some((Node::Styled { whole: i..close + 2, style: Style::Emphasis, children }, close + 2))
}

/// 記号形式の `"傍点"`、`'傍線'`
fn quoted(chars: &[char], i: usize, end: usize, symbol_forms: bool) -> Option<(Node, usize)> {
    let style = match chars[i] {
        '"' if symbol_forms => Style::Emphasis,
        '\'' if symbol_forms => Style::Sideline,
        _ => return None,
    };
    let close = find(chars, &[chars[i]], i + 1..end)?;
    if close == i + 1 {
        return None;
    }
    let children = parse_inline(chars, i + 1..close, symbol_forms);
    Some((Node::Styled { whole: i..close + 1, style, children }, close + 1))
}

/// 文書全体を解析する。
pub(crate) fn parse(text: &str, symbol_forms: bool) -> Document {
    let mut parser = Parser {
        symbol_forms,
        rubymap: HashMap::new(),
        document: Document::default(),
    };
    let lines: Vec<(usize, Vec<char>)> = {
        let mut offset = 0;
        text.split('\n')
            .map(|line| {
                let start = offset;
                offset += line.chars().count() + 1;
                (start, line.strip_suffix('\r').unwrap_or(line).chars().collect())
            })
            .collect()
    };
    let has_start = lines
        .iter()
        .any(|(_, chars)| matches!(line_head(chars, symbol_forms), Head::Tag { tag: Tag::Start, .. }));
    // `#START#` があれば、その前は本文として扱わない
    let mut in_body = !has_start;
    let mut in_tnf = false;
    for (offset, chars) in &lines {
        let head = line_head(chars, symbol_forms);
        if in_tnf {
            if matches!(head, Head::Tag { tag: Tag::TnfEnd, .. }) {
                in_tnf = false;
            } else if in_body {
                parser.raw_paragraph(chars, *offset);
            }
            continue;
        }
        match head {
            Head::Tag { tag: Tag::Start, .. } => in_body = true,
            Head::Tag { tag: Tag::End, .. } => in_body = false,
            Head::Tag { tag: Tag::TnfStart, .. } => in_tnf = true,
            Head::Tag { tag: Tag::Meta(label), end } => parser.metadata(label, chars, end, *offset),
            Head::Prefix('-') => parser.metadata("タイトル", chars, 1, *offset),
            Head::Prefix('$') => parser.metadata("巻数", chars, 1, *offset),
            Head::Prefix('+') => parser.metadata("筆者", chars, 1, *offset),
            _ if in_body => parser.body_line(head, chars, *offset),
            _ => {}
        }
    }
    parser.document
}

struct Parser {
    symbol_forms: bool,
    rubymap: HashMap<String, String>,
    document: Document,
}

fn slice(chars: &[char], range: Range<usize>) -> String {
    chars[range].iter().collect()
}

/// 前後の半角・全角空白を除いた範囲。
fn trimmed(chars: &[char], start: usize) -> Range<usize> {
    let is_space = |c: &&char| c.is_whitespace();
    let leading = chars[start..].iter().take_while(is_space).count();
    let trailing = chars[start + leading..].iter().rev().take_while(is_space).count();
    start + leading..chars.len() - trailing
}

impl Parser {
    fn metadata(&mut self, label: &'static str, chars: &[char], start: usize, offset: usize) {
        let value = slice(chars, trimmed(chars, start));
        let span = offset..offset + chars.len();
        self.document.metadata.push(Metadata { label, value, span });
    }

    fn raw_paragraph(&mut self, chars: &[char], offset: usize) {
        let span = offset..offset + chars.len();
        let inlines = if chars.is_empty() {
            Vec::new()
        } else {
            vec![Inline { kind: InlineKind::Text(chars.iter().collect()), span: span.clone() }]
        };
        self.document.blocks.push(Block { kind: BlockKind::Paragraph, inlines, span });
    }

    fn block(&mut self, kind: BlockKind, inlines: Vec<Inline>, chars: &[char], offset: usize) {
        self.document.blocks.push(Block { kind, inlines, span: offset..offset + chars.len() });
    }

    fn inlines(&mut self, chars: &[char], range: Range<usize>, offset: usize) -> Vec<Inline> {
        let nodes = parse_inline(chars, range, self.symbol_forms);
        self.convert(&nodes, chars, offset)
    }

    fn convert(&mut self, nodes: &[Node], chars: &[char], offset: usize) -> Vec<Inline> {
        let absolute = |range: &Range<usize>| range.start + offset..range.end + offset;
        nodes
            .iter()
            .map(|node| match node {
                Node::Text(range) => Inline { kind: InlineKind::Text(slice(chars, range.clone())), span: absolute(range) },
                Node::Ruby { whole, base, ruby } => {
                    let (base, ruby) = (slice(chars, base.clone()), slice(chars, ruby.clone()));
                    self.rubymap.insert(base.clone(), ruby.clone());
                    Inline { kind: InlineKind::Ruby { base, ruby }, span: absolute(whole) }
                }
                Node::RubyRef { whole, base } => {
                    let base = slice(chars, base.clone());
                    let kind = match self.rubymap.get(&base) {
                        Some(ruby) => InlineKind::Ruby { base, ruby: ruby.clone() },
                        None => InlineKind::Text(slice(chars, whole.clone())),
                    };
                    Inline { kind, span: absolute(whole) }
                }
                Node::Styled { whole, style, children } => {
                    let children = self.convert(children, chars, offset);
                    let kind = match style {
                        Style::Emphasis => InlineKind::Emphasis(children),
                        Style::Sideline => InlineKind::Sideline(children),
                    };
                    Inline { kind, span: absolute(whole) }
                }
            })
            .collect()
    }

    fn body_line(&mut self, head: Head, chars: &[char], offset: usize) {
        let len = chars.len();
        match head {
            Head::Comment | Head::Prefix('%') => {}
            Head::NewPage | Head::Alone(':') | Head::Tag { tag: Tag::NewPage, .. } => {
                self.block(BlockKind::PageBreak, Vec::new(), chars, offset);
            }
            Head::Alone('{') | Head::Tag { tag: Tag::PageCenterStart, .. } => {
                self.block(BlockKind::PageCenterStart, Vec::new(), chars, offset);
            }
            Head::Alone('}') | Head::Tag { tag: Tag::PageCenterEnd, .. } => {
                self.block(BlockKind::PageCenterEnd, Vec::new(), chars, offset);
            }
            Head::PixivChapter { end } => {
                let inlines = self.inlines(chars, CHAPTER_OPEN..end - 1, offset);
                self.block(BlockKind::Chapter, inlines, chars, offset);
            }
            Head::Prefix('*') => {
                let inlines = self.inlines(chars, trimmed(chars, 1), offset);
                self.block(BlockKind::Chapter, inlines, chars, offset);
            }
            Head::Prefix(';') => {
                let inlines = self.inlines(chars, trimmed(chars, 1), offset);
                self.block(BlockKind::Strong, inlines, chars, offset);
            }
            Head::Tag { tag: Tag::Chapter, end } => {
                let inlines = self.inlines(chars, trimmed(chars, end), offset);
                self.block(BlockKind::Chapter, inlines, chars, offset);
            }
            Head::Tag { tag: Tag::Center, end } => {
                let inlines = self.inlines(chars, trimmed(chars, end), offset);
                self.block(BlockKind::Center, inlines, chars, offset);
            }
            Head::Tag { tag: Tag::Under, end } => {
                let inlines = self.inlines(chars, trimmed(chars, end), offset);
                self.block(BlockKind::Under, inlines, chars, offset);
            }
            Head::Tag { tag: Tag::Image(align), end } => {
                let source = slice(chars, trimmed(chars, end));
                self.block(BlockKind::Image { align, source }, Vec::new(), chars, offset);
            }
            Head::Tag { .. } | Head::Alone(_) | Head::Prefix(_) => {}
            Head::None => {
                // 全角空白行削除
                if !chars.is_empty() && chars.iter().all(|c| *c == '\u{3000}') {
                    return;
                }
                let inlines = self.inlines(chars, 0..len, offset);
                self.block(BlockKind::Paragraph, inlines, chars, offset);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(text: &str) -> Document {
        parse(text, true)
    }

    fn text(content: &str, span: Range<usize>) -> Inline {
        Inline { kind: InlineKind::Text(content.to_owned()), span }
    }

    #[test]
    fn paragraph_spans_point_into_source() {
        let d = doc("あ\nい[字|じ]う");
        assert_eq!(d.blocks.len(), 2);
        assert_eq!(d.blocks[1].span, 2..9);
        assert_eq!(
            d.blocks[1].inlines,
            [
                text("い", 2..3),
                Inline { kind: InlineKind::Ruby { base: "字".into(), ruby: "じ".into() }, span: 3..8 },
                text("う", 8..9),
            ]
        );
    }

    #[test]
    fn rubymap_resolves_only_earlier_definitions() {
        let d = doc("[字]\n[字|じ]\n[字]");
        assert_eq!(d.blocks[0].inlines[0].kind, InlineKind::Text("[字]".into()));
        assert_eq!(d.blocks[2].inlines[0].kind, InlineKind::Ruby { base: "字".into(), ruby: "じ".into() });
    }

    #[test]
    fn metadata_is_kept_out_of_body_even_outside_start_end() {
        let d = doc("#筆者# どこぞの誰か\n前書き\n#START#\n#TITLE# 題\n本文\n#END#\n+後の筆者\n後書き");
        let values: Vec<_> = d.metadata.iter().map(|m| (m.label, m.value.as_str())).collect();
        assert_eq!(values, [("筆者", "どこぞの誰か"), ("タイトル", "題"), ("筆者", "後の筆者")]);
        assert_eq!(d.blocks.len(), 1);
        assert_eq!(d.blocks[0].inlines[0].kind, InlineKind::Text("本文".into()));
    }

    #[test]
    fn layout_tags_and_symbols_become_blocks() {
        let kinds = |text: &str| -> Vec<BlockKind> { doc(text).blocks.into_iter().map(|b| b.kind).collect() };
        assert_eq!(kinds("#CHAPTER# 一\n*二\n[chapter:三]\n%目次のみ"), [BlockKind::Chapter, BlockKind::Chapter, BlockKind::Chapter]);
        assert_eq!(kinds(":\n#改頁#\n[newpage]\n#改段#"), vec![BlockKind::PageBreak; 4]);
        assert_eq!(kinds("{\n#中央配置:終了#"), [BlockKind::PageCenterStart, BlockKind::PageCenterEnd]);
        assert_eq!(kinds("#CENTER# ※\n#UNDER# 地\n;強"), [BlockKind::Center, BlockKind::Under, BlockKind::Strong]);
        assert_eq!(
            kinds("#IMG:L# a.jpg"),
            [BlockKind::Image { align: ImageAlign::Left, source: "a.jpg".into() }]
        );
    }

    #[test]
    fn comment_and_fullwidth_space_lines_are_dropped_but_empty_lines_stay() {
        let d = doc("! 消える\n\u{3000}\u{3000}\n\n本文");
        assert_eq!(d.blocks.len(), 2);
        assert!(d.blocks[0].inlines.is_empty());
    }

    #[test]
    fn inline_tnf_is_raw_text() {
        let d = doc("#TNF:START#\n<font>[字|じ]</font>\n#TNF:END#\n[字|じ]");
        assert_eq!(d.blocks[0].inlines[0].kind, InlineKind::Text("<font>[字|じ]</font>".into()));
        assert!(matches!(d.blocks[1].inlines[0].kind, InlineKind::Ruby { .. }));
    }

    #[test]
    fn symbol_forms_off_keeps_symbols_as_text() {
        let d = parse("! 残る\n-題\n\"傍点\"\n[字|じ]\n#TITLE# 題", false);
        assert_eq!(d.metadata.len(), 1);
        assert_eq!(d.blocks.len(), 4);
        assert_eq!(d.blocks[0].inlines[0].kind, InlineKind::Text("! 残る".into()));
        assert_eq!(d.blocks[2].inlines[0].kind, InlineKind::Text("\"傍点\"".into()));
        assert!(matches!(d.blocks[3].inlines[0].kind, InlineKind::Ruby { .. }));
    }

    #[test]
    fn styled_inlines_nest() {
        let d = doc("\"傍[字|じ]\"'線'");
        let InlineKind::Emphasis(children) = &d.blocks[0].inlines[0].kind else { panic!() };
        assert_eq!(children[0], text("傍", 1..2));
        assert!(matches!(children[1].kind, InlineKind::Ruby { .. }));
        assert!(matches!(d.blocks[0].inlines[1].kind, InlineKind::Sideline(_)));
    }

    #[test]
    fn crlf_lines_keep_offsets() {
        let d = doc("あ\r\nい");
        assert_eq!(d.blocks[1].span, 3..4);
    }
}
