//! ページモードの状態。組版結果を画面へ配置する。Qtに依存しない。

use std::path::{Path, PathBuf};

use kakikotoba_layout::page::{PageSettings, PagedDocument};
use kakikotoba_layout::{Font, Rect};
use kakikotoba_markup::ast::Document;

const SCREEN_PADDING: f32 = 16.0;

/// 見開きの片側の画面上の配置。
pub struct PlacedPage {
    /// 空白ページなら `None`
    pub page: Option<usize>,
    pub x: f32,
    pub y: f32,
}

pub struct PageView {
    pub document: PagedDocument,
    spread: usize,
    directory: Option<PathBuf>,
}

impl PageView {
    pub fn new(
        document: &Document,
        serif: &Font,
        sans: &Font,
        settings: PageSettings,
        source: usize,
        directory: Option<PathBuf>,
    ) -> Self {
        let document = PagedDocument::new(document, serif, sans, settings);
        let spread = document.spread_of_page(document.page_of_source(source));
        Self { document, spread, directory }
    }

    /// 1emの画面上の長さ。
    pub fn em(&self, width: f32, height: f32) -> f32 {
        let (width, height) = (width - 2.0 * SCREEN_PADDING, height - 2.0 * SCREEN_PADDING);
        (height / self.document.page_height).min(width / (2.0 * self.document.page_width)).max(0.1)
    }

    /// 見開きの `[右ページ, 左ページ]` の配置。
    pub fn placed_pages(&self, width: f32, height: f32) -> [PlacedPage; 2] {
        let em = self.em(width, height);
        let page_width = self.document.page_width * em;
        let y = (height - self.document.page_height * em) / 2.0;
        let (right, left) = self.document.spread_pages(self.spread);
        [
            PlacedPage { page: right, x: width / 2.0, y },
            PlacedPage { page: left, x: width / 2.0 - page_width, y },
        ]
    }

    /// 画面上のクリック位置に対応する元ソースの位置。
    pub fn hit_test(&self, x: f32, y: f32, width: f32, height: f32) -> Option<usize> {
        let em = self.em(width, height);
        let page_width = self.document.page_width * em;
        let page_height = self.document.page_height * em;
        self.placed_pages(width, height).into_iter().find_map(|placed| {
            let inside = (placed.x..placed.x + page_width).contains(&x) && (placed.y..placed.y + page_height).contains(&y);
            let page = placed.page.filter(|_| inside)?;
            self.document.hit_test(page, (x - placed.x) / em, (y - placed.y) / em)
        })
    }

    pub fn next_spread(&mut self) {
        self.spread = (self.spread + 1).min(self.document.spread_count() - 1);
    }

    pub fn previous_spread(&mut self) {
        self.spread = self.spread.saturating_sub(1);
    }

    pub fn first_spread(&mut self) {
        self.spread = 0;
    }

    pub fn last_spread(&mut self) {
        self.spread = self.document.spread_count() - 1;
    }

    /// 表示中の見開きの先頭にある元ソースの位置。設定変更後も同じ箇所を開くために使う。
    pub fn current_source(&self) -> Option<usize> {
        let (right, left) = self.document.spread_pages(self.spread);
        [right, left]
            .into_iter()
            .flatten()
            .find_map(|page| self.document.pages[page].stops.first())
            .map(|stop| stop.source)
    }

    /// 表示中のページ番号の説明（例: `2-3 / 120`）。
    pub fn label(&self) -> String {
        let total = self.document.pages.len();
        match self.document.spread_pages(self.spread) {
            (Some(right), Some(left)) => format!("{}-{} / {total}", right + 1, left + 1),
            (Some(page), None) | (None, Some(page)) => format!("{} / {total}", page + 1),
            (None, None) => format!("0 / {total}"),
        }
    }

    /// 挿絵のパス。相対パスは文書のあるディレクトリを基準にする。
    pub fn image_path(&self, name: &str) -> PathBuf {
        match &self.directory {
            Some(directory) if Path::new(name).is_relative() => directory.join(name),
            _ => PathBuf::from(name),
        }
    }

    /// ページの大きさ（画面上）。
    pub fn page_rect(&self, placed: &PlacedPage, em: f32) -> Rect {
        Rect {
            x: placed.x,
            y: placed.y,
            width: self.document.page_width * em,
            height: self.document.page_height * em,
        }
    }
}
