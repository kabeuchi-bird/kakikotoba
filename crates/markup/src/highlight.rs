//! 編集モードの記法ハイライト。行内で完結する記法だけを、行単位で解析する。

use std::ops::Range;
use std::path::Path;

use pulldown_cmark::{Event, Options as CmarkOptions, Parser, Tag, TagEnd};

/// `#〜#` タグの名前。文中の `#1` などと衝突させないため、リファレンスにあるものだけを認める。
const TAGS: &[&str] = &[
    "TITLE", "タイトル", "VOLUME", "巻数", "WRITER", "筆者", "WRITER:URL", "筆者:URL", "WRITER:MAIL", "筆者:MAIL",
    "ILLUSTRATOR", "絵師", "ILLUSTRATOR:URL", "絵師:URL", "ILLUSTRATOR:MAIL", "絵師:MAIL", "CIRCLE", "サークル",
    "THANKS", "協力", "EDITION", "改版履歴", "PRINTSHOP", "印刷所", "COVER", "表紙", "MIDDLECOVER", "中表紙",
    "FRONTISPIECE", "口絵", "BACKCOVER", "裏表紙", "MIDDLEBACKGROUND", "中表紙背景", "ILLUSTRATORBACKGROUND",
    "絵師背景", "INDEXBACKGROUND", "目次背景", "RIGHTPAGEBACKGROUND", "右ページ背景", "LEFTPAGEBACKGROUND",
    "左ページ背景", "PUBLISHERBACKGROUND", "奥付背景", "START", "開始", "END", "終了", "TNF:START", "TNF:END",
    "CHAPTER", "章", "NEWPAGE", "改頁", "改ページ", "NEWSTEP", "改段", "IMG:L", "挿絵:左", "IMG:R", "挿絵:右",
    "IMG:N", "挿絵:次", "CENTER", "中央配置", "中央", "UNDER", "地付き", "中央配置:開始", "中央配置:終了",
];

/// 色分けの種類。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Kind {
    /// 記法の区切り記号
    Delimiter,
    /// `#〜#` タグや行頭の記号
    Tag,
    /// ルビの読み
    Ruby,
    /// 傍点
    Emphasis,
    /// 傍線
    Sideline,
    Comment,
    /// 太字
    Strong,
    Code,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dialect {
    PreTnf,
    Markdown,
}

impl Dialect {
    /// 拡張子で方言を選ぶ。`.md` 以外（新規文書を含む）はPreTNF。
    pub fn from_path(path: Option<&Path>) -> Self {
        match path.and_then(Path::extension) {
            Some(extension) if extension == "md" => Self::Markdown,
            _ => Self::PreTnf,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Options {
    pub dialect: Dialect,
    /// PreTNFの記号形式を有効にするか
    pub symbol_forms: bool,
}

/// 文字（char）単位の範囲と種類。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub range: Range<usize>,
    pub kind: Kind,
}

struct Line {
    text: String,
    char_count: usize,
    spans: Vec<Span>,
}

/// 前回の解析結果を行単位で持ち、変更のあった行だけ解析し直す。
pub struct Highlighter {
    options: Options,
    lines: Vec<Line>,
}

impl Highlighter {
    pub fn new(options: Options) -> Self {
        Self { options, lines: Vec::new() }
    }

    pub fn options(&self) -> Options {
        self.options
    }

    /// 全文を受け取り、前回と異なる行だけを解析し直す。編集箇所の指定は不要。
    pub fn update(&mut self, text: &str) {
        let new: Vec<&str> = text.split('\n').collect();
        let common = new.len().min(self.lines.len());
        let prefix = (0..common).take_while(|&i| new[i] == self.lines[i].text).count();
        let suffix = (0..common - prefix)
            .take_while(|&i| new[new.len() - 1 - i] == self.lines[self.lines.len() - 1 - i].text)
            .count();
        let changed = new[prefix..new.len() - suffix].iter().map(|text| Line {
            text: (*text).to_owned(),
            char_count: text.chars().count(),
            spans: highlight_line(text, self.options),
        });
        self.lines.splice(prefix..self.lines.len() - suffix, changed);
    }

    /// 全文での位置に直した範囲を、先頭から順に返す。
    pub fn spans(&self) -> impl Iterator<Item = Span> + '_ {
        let mut line_start = 0;
        self.lines.iter().flat_map(move |line| {
            let start = line_start;
            line_start += line.char_count + 1;
            line.spans.iter().map(move |span| Span {
                range: span.range.start + start..span.range.end + start,
                kind: span.kind,
            })
        })
    }
}

/// 1行を解析する。範囲は行頭からの文字単位で、重ならず、先頭から順に並ぶ。
pub fn highlight_line(line: &str, options: Options) -> Vec<Span> {
    let line = line.strip_suffix('\r').unwrap_or(line);
    let mut painter = Painter::new(line);
    match options.dialect {
        Dialect::PreTnf => paint_pretnf(&mut painter, options.symbol_forms),
        Dialect::Markdown => paint_markdown(&mut painter, line),
    }
    painter.into_spans()
}

/// 文字ごとの種類を塗り、後で範囲にまとめる。内側の要素は外側を上書きする。
struct Painter {
    chars: Vec<char>,
    kinds: Vec<Option<Kind>>,
}

impl Painter {
    fn new(line: &str) -> Self {
        let chars: Vec<char> = line.chars().collect();
        let kinds = vec![None; chars.len()];
        Self { chars, kinds }
    }

    fn paint(&mut self, range: Range<usize>, kind: Kind) {
        self.kinds[range].fill(Some(kind));
    }

    fn find(&self, target: &[char], range: Range<usize>) -> Option<usize> {
        self.chars[range.clone()]
            .windows(target.len())
            .position(|window| window == target)
            .map(|offset| range.start + offset)
    }

    fn into_spans(self) -> Vec<Span> {
        let mut spans: Vec<Span> = Vec::new();
        for (i, kind) in self.kinds.into_iter().enumerate() {
            let Some(kind) = kind else { continue };
            match spans.last_mut() {
                Some(span) if span.kind == kind && span.range.end == i => span.range.end = i + 1,
                _ => spans.push(Span { range: i..i + 1, kind }),
            }
        }
        spans
    }
}

fn paint_pretnf(p: &mut Painter, symbol_forms: bool) {
    let len = p.chars.len();
    let body = match head_length(p, symbol_forms) {
        Head::Whole(kind) => {
            p.paint(0..len, kind);
            return;
        }
        Head::Length(length) => {
            p.paint(0..length, Kind::Tag);
            length
        }
    };
    paint_inline(p, body..len, None, symbol_forms);
}

enum Head {
    /// 行全体が1つの要素
    Whole(Kind),
    /// 行頭のタグや記号の長さ（なければ0）
    Length(usize),
}

fn head_length(p: &Painter, symbol_forms: bool) -> Head {
    let chars = &p.chars;
    let text: String = chars.iter().collect();
    if chars.first() == Some(&'#')
        && let Some(end) = p.find(&['#'], 1..chars.len())
        && TAGS.contains(&chars[1..end].iter().collect::<String>().as_str())
    {
        return Head::Length(end + 1);
    }
    if text == "[newpage]" {
        return Head::Whole(Kind::Tag);
    }
    if text.starts_with("[chapter:")
        && let Some(end) = p.find(&[']'], 9..chars.len())
        && end > 9
    {
        return Head::Length(end + 1);
    }
    if !symbol_forms {
        return Head::Length(0);
    }
    match chars.first() {
        Some('!') => Head::Whole(Kind::Comment),
        Some(':' | '{' | '}') if chars.len() == 1 => Head::Whole(Kind::Tag),
        Some('-' | '$' | '+' | '*' | '%' | ';') if chars.len() > 1 => Head::Length(1),
        _ => Head::Length(0),
    }
}

/// 行内の記法を塗る。記法でない文字は `base` で塗る。
fn paint_inline(p: &mut Painter, range: Range<usize>, base: Option<Kind>, symbol_forms: bool) {
    let mut i = range.start;
    while i < range.end {
        let next = paint_pixiv_ruby(p, i, range.end, base)
            .or_else(|| paint_bracket_ruby(p, i, range.end, base))
            .or_else(|| paint_bar_ruby(p, i, range.end, base))
            .or_else(|| paint_double_angle_emphasis(p, i, range.end, symbol_forms))
            .or_else(|| paint_quoted(p, i, range.end, symbol_forms));
        i = next.unwrap_or_else(|| {
            paint_base(p, i..i + 1, base);
            i + 1
        });
    }
}

fn paint_base(p: &mut Painter, range: Range<usize>, base: Option<Kind>) {
    if let Some(kind) = base {
        p.paint(range, kind);
    }
}

/// `[[rb:ベース > ルビ]]`
fn paint_pixiv_ruby(p: &mut Painter, i: usize, end: usize, base: Option<Kind>) -> Option<usize> {
    const OPEN: [char; 5] = ['[', '[', 'r', 'b', ':'];
    let body = i + OPEN.len();
    if body > end || p.chars[i..body] != OPEN {
        return None;
    }
    let close = p.find(&[']', ']'], body..end)?;
    let separator = p.find(&['>'], body..close)?;
    let is_space = |c: &char| c.is_whitespace();
    let base_end = separator - p.chars[body..separator].iter().rev().take_while(|c| is_space(c)).count();
    let ruby_start = separator + 1 + p.chars[separator + 1..close].iter().take_while(|c| is_space(c)).count();
    if base_end == body || ruby_start == close {
        return None;
    }
    p.paint(i..body, Kind::Delimiter);
    paint_base(p, body..base_end, base);
    p.paint(base_end..ruby_start, Kind::Delimiter);
    p.paint(ruby_start..close, Kind::Ruby);
    p.paint(close..close + 2, Kind::Delimiter);
    Some(close + 2)
}

/// `[ベース|ルビ]`。`[ベース]` 単独はrubymapなのでページモードに任せる。
fn paint_bracket_ruby(p: &mut Painter, i: usize, end: usize, base: Option<Kind>) -> Option<usize> {
    if p.chars[i] != '[' {
        return None;
    }
    let close = p.find(&[']'], i + 1..end)?;
    let bar = p.find(&['|'], i + 1..close)?;
    if bar == i + 1 || bar + 1 == close {
        return None;
    }
    p.paint(i..i + 1, Kind::Delimiter);
    paint_base(p, i + 1..bar, base);
    p.paint(bar..bar + 1, Kind::Delimiter);
    p.paint(bar + 1..close, Kind::Ruby);
    p.paint(close..close + 1, Kind::Delimiter);
    Some(close + 1)
}

/// `|ベース《ルビ》`、`｜ベース《ルビ》`
fn paint_bar_ruby(p: &mut Painter, i: usize, end: usize, base: Option<Kind>) -> Option<usize> {
    if !matches!(p.chars[i], '|' | '｜') {
        return None;
    }
    let open = p.find(&['《'], i + 1..end)?;
    let close = p.find(&['》'], open + 1..end)?;
    if open == i + 1 || close == open + 1 {
        return None;
    }
    p.paint(i..i + 1, Kind::Delimiter);
    paint_base(p, i + 1..open, base);
    p.paint(open..open + 1, Kind::Delimiter);
    p.paint(open + 1..close, Kind::Ruby);
    p.paint(close..close + 1, Kind::Delimiter);
    Some(close + 1)
}

/// `《《傍点》》`
fn paint_double_angle_emphasis(p: &mut Painter, i: usize, end: usize, symbol_forms: bool) -> Option<usize> {
    if i + 2 > end || p.chars[i..i + 2] != ['《', '《'] {
        return None;
    }
    let close = p.find(&['》', '》'], i + 2..end)?;
    if close == i + 2 {
        return None;
    }
    p.paint(i..i + 2, Kind::Delimiter);
    paint_inline(p, i + 2..close, Some(Kind::Emphasis), symbol_forms);
    p.paint(close..close + 2, Kind::Delimiter);
    Some(close + 2)
}

/// 記号形式の `"傍点"`、`'傍線'`
fn paint_quoted(p: &mut Painter, i: usize, end: usize, symbol_forms: bool) -> Option<usize> {
    let quote = p.chars[i];
    let kind = match quote {
        '"' if symbol_forms => Kind::Emphasis,
        '\'' if symbol_forms => Kind::Sideline,
        _ => return None,
    };
    let close = p.find(&[quote], i + 1..end)?;
    if close == i + 1 {
        return None;
    }
    p.paint(i..i + 1, Kind::Delimiter);
    paint_inline(p, i + 1..close, Some(kind), symbol_forms);
    p.paint(close..close + 1, Kind::Delimiter);
    Some(close + 1)
}

fn paint_markdown(p: &mut Painter, line: &str) {
    let to_char = |byte: usize| line[..byte].chars().count();
    let mut stack: Vec<Option<Kind>> = Vec::new();
    for (event, range) in Parser::new_ext(line, CmarkOptions::empty()).into_offset_iter() {
        let chars = to_char(range.start)..to_char(range.end);
        match event {
            Event::Start(tag) => {
                let inner = match tag {
                    Tag::Emphasis => Some(Some(Kind::Emphasis)),
                    Tag::Strong => Some(Some(Kind::Strong)),
                    Tag::Link { .. } | Tag::Image { .. } => Some(None),
                    _ => None,
                };
                if let Some(inner) = inner {
                    p.paint(chars, Kind::Delimiter);
                    stack.push(inner.or_else(|| stack.last().copied().flatten()));
                }
            }
            Event::End(TagEnd::Emphasis | TagEnd::Strong | TagEnd::Link | TagEnd::Image) => {
                stack.pop();
            }
            Event::Code(_) => p.paint(chars, Kind::Code),
            Event::Text(_) if !stack.is_empty() => match stack.last().copied().flatten() {
                Some(kind) => p.paint(chars, kind),
                None => p.kinds[chars].fill(None),
            },
            _ => {}
        }
    }
    // 拡張ルビ。コードの中はそのまま文字列として扱う
    let mut i = 0;
    while i < p.chars.len() {
        i = match p.chars[i] {
            '｜' if p.kinds[i] != Some(Kind::Code) => {
                let end = p.chars.len();
                let base = p.kinds[i];
                paint_bar_ruby(p, i, end, base).unwrap_or(i + 1)
            }
            _ => i + 1,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRETNF: Options = Options { dialect: Dialect::PreTnf, symbol_forms: true };
    const PRETNF_NO_SYMBOLS: Options = Options { dialect: Dialect::PreTnf, symbol_forms: false };
    const MARKDOWN: Options = Options { dialect: Dialect::Markdown, symbol_forms: true };

    /// 色の付いた部分を `(文字列, 種類)` で返す。
    fn parts(line: &str, options: Options) -> Vec<(String, Kind)> {
        let chars: Vec<char> = line.chars().collect();
        highlight_line(line, options)
            .into_iter()
            .map(|span| (chars[span.range].iter().collect(), span.kind))
            .collect()
    }

    fn p(parts: &[(&str, Kind)]) -> Vec<(String, Kind)> {
        parts.iter().map(|(text, kind)| ((*text).to_owned(), *kind)).collect()
    }

    use Kind::*;

    #[test]
    fn plain_text_has_no_spans() {
        assert!(parts("吾輩は猫である。", PRETNF).is_empty());
        assert!(parts("", PRETNF).is_empty());
        assert!(parts("", MARKDOWN).is_empty());
    }

    #[test]
    fn all_ruby_forms() {
        let expected = p(&[("[", Delimiter), ("|", Delimiter), ("かんじ", Ruby), ("]", Delimiter)]);
        // 隣り合う区切りはまとまらない位置にある（ベース文字を挟む）
        assert_eq!(parts("[漢字|かんじ]", PRETNF), expected);
        assert_eq!(
            parts("前|漢字《かんじ》後", PRETNF),
            p(&[("|", Delimiter), ("《", Delimiter), ("かんじ", Ruby), ("》", Delimiter)])
        );
        assert_eq!(
            parts("｜漢字《かんじ》", PRETNF),
            p(&[("｜", Delimiter), ("《", Delimiter), ("かんじ", Ruby), ("》", Delimiter)])
        );
        assert_eq!(
            parts("[[rb:漢字 > かんじ]]", PRETNF),
            p(&[("[[rb:", Delimiter), (" > ", Delimiter), ("かんじ", Ruby), ("]]", Delimiter)])
        );
    }

    #[test]
    fn ruby_stays_on_when_symbol_forms_are_off() {
        assert_eq!(parts("[漢字|かんじ]", PRETNF_NO_SYMBOLS), parts("[漢字|かんじ]", PRETNF));
        assert_eq!(parts("《《傍点》》", PRETNF_NO_SYMBOLS).len(), 3);
    }

    #[test]
    fn rubymap_and_incomplete_ruby_are_not_highlighted() {
        for line in ["[漢字]", "[|かんじ]", "[漢字|]", "|《かんじ》", "|漢字《》", "|漢字", "[漢字|かんじ", "[[rb:漢字]]"] {
            let spans = parts(line, PRETNF_NO_SYMBOLS);
            assert!(!spans.iter().any(|(_, kind)| *kind == Ruby), "{line}: {spans:?}");
        }
        assert!(parts("[漢字]", PRETNF).is_empty());
    }

    #[test]
    fn tags_need_line_head_and_known_name() {
        assert_eq!(parts("#TITLE# 夢の残照", PRETNF), p(&[("#TITLE#", Tag)]));
        assert_eq!(parts("#挿絵:左# a.jpg", PRETNF_NO_SYMBOLS), p(&[("#挿絵:左#", Tag)]));
        assert_eq!(parts("#中央配置:終了#", PRETNF), p(&[("#中央配置:終了#", Tag)]));
        assert!(parts("#1 と #2 の話", PRETNF).is_empty());
        assert!(parts("本文 #TITLE#", PRETNF).is_empty());
        assert_eq!(parts("[newpage]", PRETNF_NO_SYMBOLS), p(&[("[newpage]", Tag)]));
        assert_eq!(parts("[chapter:第一話]", PRETNF_NO_SYMBOLS), p(&[("[chapter:第一話]", Tag)]));
        assert!(parts("文中[newpage]", PRETNF).is_empty());
    }

    #[test]
    fn symbol_forms_follow_the_option() {
        assert_eq!(parts("! 本文には書けないよ！", PRETNF), p(&[("! 本文には書けないよ！", Comment)]));
        assert_eq!(parts("-夢の残照", PRETNF), p(&[("-", Tag)]));
        assert_eq!(parts(";強い調べ", PRETNF), p(&[(";", Tag)]));
        assert_eq!(parts(":", PRETNF), p(&[(":", Tag)]));
        assert_eq!(parts("{", PRETNF), p(&[("{", Tag)]));
        assert!(parts(":それ以外", PRETNF).is_empty());
        assert!(parts("-", PRETNF).is_empty());
        assert_eq!(
            parts("これは\"傍点\"と'傍線'", PRETNF),
            p(&[("\"", Delimiter), ("傍点", Emphasis), ("\"", Delimiter), ("'", Delimiter), ("傍線", Sideline), ("'", Delimiter)])
        );
        for line in ["! コメント", "-夢", ":", "\"傍点\"", "'傍線'", "{", "*章"] {
            assert!(parts(line, PRETNF_NO_SYMBOLS).is_empty(), "{line}");
        }
    }

    #[test]
    fn unmatched_quote_and_comment_contents_stay_plain() {
        assert!(parts("it's a \"pen", PRETNF).is_empty());
        assert_eq!(parts("! [漢字|かんじ]", PRETNF), p(&[("! [漢字|かんじ]", Comment)]));
    }

    #[test]
    fn ruby_inside_emphasis_keeps_both() {
        assert_eq!(
            parts("《《前[字|じ]》》", PRETNF),
            p(&[("《《", Delimiter), ("前", Emphasis), ("[", Delimiter), ("字", Emphasis), ("|", Delimiter), ("じ", Ruby), ("]》》", Delimiter)])
        );
    }

    #[test]
    fn markdown_inline_elements() {
        assert_eq!(parts("普通の*強調*です", MARKDOWN), p(&[("*", Delimiter), ("強調", Emphasis), ("*", Delimiter)]));
        assert_eq!(parts("**太字**", MARKDOWN), p(&[("**", Delimiter), ("太字", Strong), ("**", Delimiter)]));
        assert_eq!(parts("`a[b|c]`", MARKDOWN), p(&[("`a[b|c]`", Code)]));
        assert_eq!(
            parts("[文字](http://x)", MARKDOWN),
            p(&[("[", Delimiter), ("](http://x)", Delimiter)])
        );
        // PreTNFの記法はMarkdownでは効かない
        assert!(parts("[漢字|かんじ]", MARKDOWN).is_empty());
        assert!(parts("! \"傍点\"", MARKDOWN).is_empty());
    }

    #[test]
    fn markdown_extended_ruby_except_in_code() {
        assert_eq!(
            parts("｜漢字《かんじ》", MARKDOWN),
            p(&[("｜", Delimiter), ("《", Delimiter), ("かんじ", Ruby), ("》", Delimiter)])
        );
        assert_eq!(parts("`｜漢字《かんじ》`", MARKDOWN), p(&[("`｜漢字《かんじ》`", Code)]));
        assert!(parts("|漢字《かんじ》", MARKDOWN).is_empty());
    }

    #[test]
    fn crlf_is_ignored_for_exact_matches() {
        assert_eq!(highlight_line(":\r", PRETNF), [Span { range: 0..1, kind: Tag }]);
    }

    #[test]
    fn dialect_follows_extension() {
        assert_eq!(Dialect::from_path(Some(Path::new("a.md"))), Dialect::Markdown);
        assert_eq!(Dialect::from_path(Some(Path::new("a.txt"))), Dialect::PreTnf);
        assert_eq!(Dialect::from_path(Some(Path::new("a"))), Dialect::PreTnf);
        assert_eq!(Dialect::from_path(None), Dialect::PreTnf);
    }

    fn absolute(h: &Highlighter) -> Vec<Span> {
        h.spans().collect()
    }

    #[test]
    fn update_matches_full_parse_after_any_edit() {
        let edits = [
            "",
            "あ\n!二行目\nう",
            "あ\n!二行目改\nう",
            "前\nあ\n!二行目改\nう",
            "前\nう",
            "前\n!\n!\n!\nう",
            "!\n!\nう\n",
            "\n\n",
            "!",
        ];
        let mut incremental = Highlighter::new(PRETNF);
        for text in edits {
            incremental.update(text);
            let mut fresh = Highlighter::new(PRETNF);
            fresh.update(text);
            assert_eq!(absolute(&incremental), absolute(&fresh), "{text:?}");
            assert_eq!(incremental.lines.len(), text.split('\n').count());
        }
    }

    #[test]
    fn spans_are_absolute_and_count_newlines() {
        let mut h = Highlighter::new(PRETNF);
        h.update("あ\n!い");
        assert_eq!(absolute(&h), [Span { range: 2..4, kind: Comment }]);
        h.update("あい\n!い");
        assert_eq!(absolute(&h), [Span { range: 3..5, kind: Comment }]);
    }

    #[test]
    fn only_changed_lines_are_reparsed() {
        let mut h = Highlighter::new(PRETNF);
        h.update("!一\n二\n!三");
        // 解析結果を壊しておき、解析し直されなければ残ることで確認する
        h.lines[0].spans.clear();
        h.lines[2].spans.clear();
        h.update("!一\n二改\n!三");
        assert!(h.lines[0].spans.is_empty() && h.lines[2].spans.is_empty());
        assert_eq!(h.lines[1].text, "二改");
    }
}
