//! HERDR: whole-row decorations (a background and a gutter bar), set by the
//! application under a key so independent owners don't replace each other.

use std::collections::{BTreeMap, HashMap};
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

/// Every key's decorations, flattened to a per-row lookup for painting.
#[derive(Default)]
pub(crate) struct LineDecorations {
    by_key: BTreeMap<SharedString, Vec<LineDecoration>>,
    rows: HashMap<usize, RowPaint>,
}

impl LineDecorations {
    /// Replace `key`'s decorations; an empty list removes the key. Rows past
    /// `line_count` are dropped. Returns whether anything changed.
    pub(crate) fn set(
        &mut self,
        key: SharedString,
        decorations: Vec<LineDecoration>,
        line_count: usize,
    ) -> bool {
        let decorations: Vec<LineDecoration> = decorations
            .into_iter()
            .filter_map(|mut decoration| {
                decoration.rows.end = decoration.rows.end.min(line_count);
                let paints = decoration.background.is_some() || decoration.gutter.is_some();
                (paints && !decoration.rows.is_empty()).then_some(decoration)
            })
            .collect();
        let changed = if decorations.is_empty() {
            self.by_key.remove(&key).is_some()
        } else if self.by_key.get(&key) == Some(&decorations) {
            false
        } else {
            self.by_key.insert(key, decorations);
            true
        };
        if changed {
            self.rebuild();
        }
        changed
    }

    /// Later decorations win, within a key and across keys (in key order).
    fn rebuild(&mut self) {
        self.rows.clear();
        for decoration in self.by_key.values().flatten() {
            for row in decoration.rows.clone() {
                let paint = self.rows.entry(row).or_default();
                if decoration.background.is_some() {
                    paint.background = decoration.background;
                }
                if decoration.gutter.is_some() {
                    paint.gutter = decoration.gutter;
                }
            }
        }
    }

    pub(crate) fn row(&self, row: usize) -> Option<RowPaint> {
        self.rows.get(&row).copied()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.rows.is_empty()
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
            10,
        ));
        assert!(decorations.set(
            "b".into(),
            vec![LineDecoration::new(2..4).with_background(green).with_gutter(green)],
            10,
        ));
        assert_eq!(decorations.row(0).unwrap().background, Some(red));
        assert_eq!(decorations.row(2).unwrap(), RowPaint { background: Some(green), gutter: Some(green) });
        assert_eq!(decorations.row(4), None);

        assert!(!decorations.set("a".into(), vec![LineDecoration::new(0..3).with_background(red)], 10));
        assert!(decorations.set("b".into(), vec![], 10));
        assert_eq!(decorations.row(2).unwrap(), RowPaint { background: Some(red), gutter: None });
    }

    #[test]
    fn rows_past_the_end_and_empty_decorations_are_dropped() {
        let red = hsla(0., 1., 0.5, 1.);
        let mut decorations = LineDecorations::default();
        decorations.set("a".into(), vec![LineDecoration::new(3..usize::MAX).with_gutter(red)], 5);
        assert!(decorations.row(4).is_some());
        assert!(decorations.row(5).is_none());
        assert!(!decorations.set("b".into(), vec![LineDecoration::new(0..2)], 5));
        assert!(!decorations.is_empty());
    }
}
