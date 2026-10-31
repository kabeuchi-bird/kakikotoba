//! 方言別パーサ → 共通AST（各ノードに元ソースのspanを保持）

pub mod ast;
pub mod highlight;
mod markdown;
mod pretnf;

use ast::Document;
use highlight::{Dialect, Options};

/// 文書全体を共通ASTへ解析する。
pub fn parse(text: &str, options: Options) -> Document {
    match options.dialect {
        Dialect::PreTnf => pretnf::parse(text, options.symbol_forms),
        Dialect::Markdown => markdown::parse(text),
    }
}
