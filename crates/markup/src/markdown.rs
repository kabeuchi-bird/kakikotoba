//! Markdown → 共通AST。

use std::ops::Range;

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};

use crate::ast::{Block, BlockKind, Document, Inline, InlineKind};
use crate::pretnf::{Node, bar_ruby};

/// 組み立て中の傍点・太字。
struct Frame {
    bold: bool,
    start: usize,
    children: Vec<Inline>,
}

/// 文字列中の `｜漢字《かんじ》` をルビに分けて追加する。`start` は元ソースでの先頭位置。
fn push_text(out: &mut Vec<Inline>, text: &str, start: usize) {
    let chars: Vec<char> = text.chars().collect();
    let mut text_start = 0;
    let mut i = 0;
    let push_plain = |out: &mut Vec<Inline>, range: Range<usize>| {
        if !range.is_empty() {
            let content = chars[range.clone()].iter().collect();
            out.push(Inline { kind: InlineKind::Text(content), span: range.start + start..range.end + start });
        }
    };
    while i < chars.len() {
        if chars[i] == '｜'
            && let Some((Node::Ruby { whole, base, ruby }, next)) = bar_ruby(&chars, i, chars.len())
        {
            push_plain(out, text_start..i);
            out.push(Inline {
                kind: InlineKind::Ruby {
                    base: chars[base].iter().collect(),
                    ruby: chars[ruby].iter().collect(),
                },
                span: whole.start + start..whole.end + start,
            });
            i = next;
            text_start = next;
        } else {
            i += 1;
        }
    }
    push_plain(out, text_start..chars.len());
}

/// インラインの追加先。組み立て中の傍点・太字があればその中。
fn target<'a>(frames: &'a mut [Frame], inlines: &'a mut Vec<Inline>) -> &'a mut Vec<Inline> {
    match frames.last_mut() {
        Some(frame) => &mut frame.children,
        None => inlines,
    }
}

pub(crate) fn parse(text: &str) -> Document {
    let boundaries: Vec<usize> = text.char_indices().map(|(byte, _)| byte).collect();
    let to_char = |byte: usize| boundaries.partition_point(|&b| b < byte);

    let mut document = Document::default();
    let mut inlines: Vec<Inline> = Vec::new();
    let mut frames: Vec<Frame> = Vec::new();
    // コードブロックと表の文字列
    let mut raw: Option<String> = None;
    let mut list_depth = 0;

    for (event, range) in Parser::new_ext(text, Options::ENABLE_TABLES).into_offset_iter() {
        let span = to_char(range.start)..to_char(range.end);
        // 項目のテキストは段落に包まれないことがあるので、次のブロックの前に確定する
        let closes_item = matches!(
            event,
            Event::Rule
                | Event::End(TagEnd::Item | TagEnd::List(_))
                | Event::Start(Tag::List(_) | Tag::Paragraph | Tag::Heading { .. } | Tag::CodeBlock(_) | Tag::Table(_))
        );
        if let (true, Some(first), Some(last)) = (closes_item, inlines.first(), inlines.last()) {
            let span = first.span.start..last.span.end;
            let kind = BlockKind::ListItem { depth: list_depth };
            document.blocks.push(Block { kind, inlines: std::mem::take(&mut inlines), span });
        }
        match event {
            // 表のセル内の強調は文字列だけを残す
            Event::Start(Tag::Emphasis | Tag::Strong) | Event::End(TagEnd::Emphasis | TagEnd::Strong) if raw.is_some() => {}
            Event::Start(Tag::Emphasis) => frames.push(Frame { bold: false, start: span.start, children: Vec::new() }),
            Event::Start(Tag::Strong) => frames.push(Frame { bold: true, start: span.start, children: Vec::new() }),
            Event::End(TagEnd::Emphasis | TagEnd::Strong) => {
                let frame = frames.pop().expect("Startと対応する");
                let kind = if frame.bold { InlineKind::Bold(frame.children) } else { InlineKind::Emphasis(frame.children) };
                let inline = Inline { kind, span: frame.start..span.end };
                target(&mut frames, &mut inlines).push(inline);
            }
            Event::Start(Tag::CodeBlock(CodeBlockKind::Fenced(_) | CodeBlockKind::Indented) | Tag::Table(_)) => {
                raw = Some(String::new());
            }
            Event::End(TagEnd::CodeBlock | TagEnd::Table) => {
                let content = raw.take().unwrap_or_default();
                let lines = content.lines().map(str::to_owned).collect();
                document.blocks.push(Block { kind: BlockKind::Horizontal { lines }, inlines: Vec::new(), span });
            }
            Event::End(TagEnd::TableCell) => {
                if let Some(raw) = &mut raw {
                    raw.push_str(" | ");
                }
            }
            Event::End(TagEnd::TableHead | TagEnd::TableRow) => {
                if let Some(raw) = &mut raw {
                    raw.truncate(raw.trim_end_matches(" | ").len());
                    raw.push('\n');
                }
            }
            Event::Text(content) | Event::Code(content) if raw.is_some() => {
                if let Some(raw) = &mut raw {
                    raw.push_str(&content);
                }
            }
            Event::Text(content) => {
                push_text(target(&mut frames, &mut inlines), &content, span.start);
            }
            Event::Code(content) => {
                let inline = Inline { kind: InlineKind::Text(content.to_string()), span };
                target(&mut frames, &mut inlines).push(inline);
            }
            Event::HardBreak => {
                let inline = Inline { kind: InlineKind::LineBreak, span };
                target(&mut frames, &mut inlines).push(inline);
            }
            Event::End(TagEnd::Paragraph) => {
                let kind = if list_depth > 0 { BlockKind::ListItem { depth: list_depth } } else { BlockKind::Paragraph };
                document.blocks.push(Block { kind, inlines: std::mem::take(&mut inlines), span });
            }
            Event::End(TagEnd::Heading(_)) => {
                document.blocks.push(Block { kind: BlockKind::Chapter, inlines: std::mem::take(&mut inlines), span });
            }
            Event::Start(Tag::List(_)) => list_depth += 1,
            Event::End(TagEnd::List(_)) => list_depth -= 1,
            Event::Rule => document.blocks.push(Block { kind: BlockKind::Rule, inlines: Vec::new(), span }),
            _ => {}
        }
    }
    document
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(text: &str) -> Vec<BlockKind> {
        parse(text).blocks.into_iter().map(|b| b.kind).collect()
    }

    #[test]
    fn emphasis_in_table_does_not_leak_into_next_block() {
        let document = parse("| a |\n|---|\n| *x* |\n\n次の段落\n");
        assert_eq!(document.blocks.len(), 2);
        assert_eq!(document.blocks[0].kind, BlockKind::Horizontal { lines: vec!["a".to_owned(), "x".to_owned()] });
        assert_eq!(document.blocks[1].inlines.len(), 1);
        assert_eq!(kinds("| a |\n|---|\n| *x* |\n\n- 項目\n").len(), 2);
    }

    #[test]
    fn tight_list_item_stays_before_the_block_inside_it() {
        let item = BlockKind::ListItem { depth: 1 };
        let code = BlockKind::Horizontal { lines: vec!["code".to_owned()] };
        let document = parse("- a\n  ```\n  code\n  ```\n");
        assert_eq!(document.blocks.iter().map(|b| b.kind.clone()).collect::<Vec<_>>(), [item.clone(), code]);
        assert_eq!(document.blocks[0].span, 2..3);
        assert_eq!(kinds("- a\n  ***\n"), [item.clone(), BlockKind::Rule]);
        assert_eq!(kinds("- a\n  # 見出し\n"), [item, BlockKind::Chapter]);
    }

    #[test]
    fn blocks_map_to_common_ast() {
        assert_eq!(kinds("# 見出し\n\n本文\n\n---\n"), [BlockKind::Chapter, BlockKind::Paragraph, BlockKind::Rule]);
        assert_eq!(
            kinds("- 一\n  - 二\n- 三\n"),
            [BlockKind::ListItem { depth: 1 }, BlockKind::ListItem { depth: 2 }, BlockKind::ListItem { depth: 1 }]
        );
    }

    #[test]
    fn emphasis_is_dots_and_strong_is_bold() {
        let d = parse("あ*傍点*と**太字**");
        let inlines = &d.blocks[0].inlines;
        assert!(matches!(&inlines[1].kind, InlineKind::Emphasis(c) if c[0].kind == InlineKind::Text("傍点".into())));
        assert!(matches!(&inlines[3].kind, InlineKind::Bold(_)));
        assert_eq!(inlines[1].span, 1..5);
        assert_eq!(inlines[0].span, 0..1);
    }

    #[test]
    fn code_block_and_table_become_horizontal_lines() {
        assert_eq!(
            kinds("```\nfn a()\nb\n```\n"),
            [BlockKind::Horizontal { lines: vec!["fn a()".into(), "b".into()] }]
        );
        assert_eq!(
            kinds("|a|b|\n|-|-|\n|c|d|\n"),
            [BlockKind::Horizontal { lines: vec!["a | b".into(), "c | d".into()] }]
        );
    }

    #[test]
    fn extended_ruby_only_with_fullwidth_bar() {
        let d = parse("前｜字《じ》後 |字《じ》");
        let inlines = &d.blocks[0].inlines;
        assert_eq!(inlines[0].kind, InlineKind::Text("前".into()));
        assert_eq!(inlines[1].kind, InlineKind::Ruby { base: "字".into(), ruby: "じ".into() });
        assert_eq!(inlines[1].span, 1..6);
        assert_eq!(inlines[2].kind, InlineKind::Text("後 |字《じ》".into()));
    }

    #[test]
    fn soft_break_joins_and_hard_break_is_kept() {
        let d = parse("一\n二  \n三");
        let kinds: Vec<_> = d.blocks[0].inlines.iter().map(|i| &i.kind).collect();
        assert!(kinds.contains(&&InlineKind::LineBreak));
        assert_eq!(kinds.iter().filter(|k| matches!(k, InlineKind::Text(_))).count(), 3);
    }
}
