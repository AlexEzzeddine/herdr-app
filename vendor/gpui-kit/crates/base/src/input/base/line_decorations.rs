//! HERDR: whole-row decorations (a background and a gutter bar), set by the
//! application under a key so independent owners don't replace each other.

use std::collections::BTreeMap;
use std::ops::Range;

use gpui::{Hsla, SharedString};

/// A decoration over whole buffer rows: a fill behind their text and line
/// numbers, and a bar at the gutter's left edge.
///
/// Rows are fixed: they don't move when text is inserted or deleted above
/// them, so an owner that tracks edits sets them again.
#[derive(Clone, Debug, PartialEq)]
pub struct LineDecoration {
    /// 0-based buffer rows, end exclusive.
    pub rows: Range<usize>,
    /// Fill behind the rows, under the active line, selections and text.
    pub background: Option<Hsla>,
    /// A bar at the gutter's left edge; needs line numbers.
    pub gutter: Option<Hsla>,
}

impl LineDecoration {
    /// A decoration over `rows` with nothing painted yet.
    pub fn new(rows: Range<usize>) -> Self {
        Self {
            rows,
            background: None,
            gutter: None,
        }
    }

    pub fn with_background(mut self, color: Hsla) -> Self {
        self.background = Some(color);
        self
    }

    pub fn with_gutter(mut self, color: Hsla) -> Self {
        self.gutter = Some(color);
        self
    }
}

/// What one row paints, after every key is applied.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct RowPaint {
    pub background: Option<Hsla>,
    pub gutter: Option<Hsla>,
}

/// Every key's decorations, kept as ranges and resolved per visible row when
/// painting, so a set costs the number of decorations, not of decorated rows.
#[derive(Default)]
pub(crate) struct LineDecorations {
    by_key: BTreeMap<SharedString, Vec<LineDecoration>>,
}

impl LineDecorations {
    /// Replace `key`'s decorations; an empty list removes the key. Rows past
    /// the end of the text are kept (the text may still grow) and simply
    /// don't paint. Returns whether anything changed.
    pub(crate) fn set(&mut self, key: SharedString, decorations: Vec<LineDecoration>) -> bool {
        let decorations: Vec<LineDecoration> = decorations
            .into_iter()
            .filter(|decoration| {
                let paints = decoration.background.is_some() || decoration.gutter.is_some();
                paints && !decoration.rows.is_empty()
            })
            .collect();
        if decorations.is_empty() {
            self.by_key.remove(&key).is_some()
        } else if self.by_key.get(&key) == Some(&decorations) {
            false
        } else {
            self.by_key.insert(key, decorations);
            true
        }
    }

    /// The paint of each of `rows` (sorted ascending, as laid out: folds
    /// skip rows), one entry per row. Later decorations win, within a key
    /// and across keys (in key order). Only the given rows are resolved, so
    /// a fold over many rows costs nothing.
    pub(crate) fn resolve(&self, rows: &[usize]) -> Vec<Option<RowPaint>> {
        let mut out = vec![None; rows.len()];
        for decoration in self.by_key.values().flatten() {
            let start = rows.partition_point(|&row| row < decoration.rows.start);
            let end = rows.partition_point(|&row| row < decoration.rows.end);
            for paint in &mut out[start..end] {
                let paint = paint.get_or_insert_with(RowPaint::default);
                if decoration.background.is_some() {
                    paint.background = decoration.background;
                }
                if decoration.gutter.is_some() {
                    paint.gutter = decoration.gutter;
                }
            }
        }
        out
    }

    #[cfg(test)]
    pub(crate) fn row(&self, row: usize) -> Option<RowPaint> {
        self.resolve(&[row])[0]
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.by_key.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::hsla;

    #[test]
    fn keys_combine_and_later_decorations_win() {
        let red = hsla(0., 1., 0.5, 1.);
        let green = hsla(0.33, 1., 0.5, 1.);
        let mut decorations = LineDecorations::default();
        assert!(decorations.set(
            "a".into(),
            vec![LineDecoration::new(0..3).with_background(red)],
        ));
        assert!(decorations.set(
            "b".into(),
            vec![LineDecoration::new(2..4).with_background(green).with_gutter(green)],
        ));
        assert_eq!(decorations.row(0).unwrap().background, Some(red));
        assert_eq!(decorations.row(2).unwrap(), RowPaint { background: Some(green), gutter: Some(green) });
        assert_eq!(decorations.row(4), None);

        assert!(!decorations.set("a".into(), vec![LineDecoration::new(0..3).with_background(red)]));
        assert!(decorations.set("b".into(), vec![]));
        assert_eq!(decorations.row(2).unwrap(), RowPaint { background: Some(red), gutter: None });
    }

    #[test]
    fn rows_past_the_end_are_kept_and_empty_decorations_dropped() {
        let red = hsla(0., 1., 0.5, 1.);
        let mut decorations = LineDecorations::default();
        decorations.set("a".into(), vec![LineDecoration::new(3..usize::MAX).with_gutter(red)]);
        assert!(decorations.row(2).is_none());
        assert!(decorations.row(1_000_000).is_some(), "kept for text that grows later");
        assert!(!decorations.set("b".into(), vec![LineDecoration::new(0..2)]));
        assert!(!decorations.set("c".into(), vec![LineDecoration::new(4..4).with_gutter(red)]));
        assert!(!decorations.is_empty());
    }

    #[test]
    fn resolve_covers_only_the_asked_rows() {
        let red = hsla(0., 1., 0.5, 1.);
        let mut decorations = LineDecorations::default();
        decorations.set("a".into(), vec![LineDecoration::new(0..usize::MAX).with_background(red)]);
        let rows = decorations.resolve(&[100, 101, 5000]);
        assert_eq!(rows.len(), 3);
        assert!(rows.iter().all(|row| row.unwrap().background == Some(red)));
        decorations.set("a".into(), vec![LineDecoration::new(3..10).with_background(red)]);
        // Rows 4..9 folded away: only the laid-out rows are resolved.
        let rows = decorations.resolve(&[2, 3, 10, 11]);
        assert_eq!(rows.iter().map(Option::is_some).collect::<Vec<_>>(), [false, true, false, false]);
    }
}
