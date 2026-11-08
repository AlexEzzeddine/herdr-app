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
    /// Fill behind the rows, over the active line and under selections and text.
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
    /// Applied in key order, later keys winning: byte order, which for
    /// the UTF-8 keys is code-point order (what scripts are told: JS
    /// `sort()` order, uppercase before lowercase).
    by_key: BTreeMap<SharedString, KeyDecorations>,
}

/// One key's decorations, indexed so painting finds those over the visible
/// rows without walking the rest.
struct KeyDecorations {
    /// In the order they were set: later ones win.
    list: Vec<LineDecoration>,
    /// Indices into `list`, sorted by `rows.start`.
    by_start: Vec<usize>,
    /// `max_end[i]`: the largest `rows.end` among `by_start[..=i]`. It never
    /// decreases, so the first decoration that can reach a row is found by
    /// binary search even when ranges nest or overlap.
    max_end: Vec<usize>,
}

impl KeyDecorations {
    fn new(list: Vec<LineDecoration>) -> Self {
        let mut by_start: Vec<usize> = (0..list.len()).collect();
        by_start.sort_by_key(|&i| list[i].rows.start);
        let max_end = by_start
            .iter()
            .scan(0, |max, &i| {
                *max = (*max).max(list[i].rows.end);
                Some(*max)
            })
            .collect();
        Self { list, by_start, max_end }
    }

    /// Indices into `list` of the decorations that overlap `first..=last`,
    /// in `by_start` order.
    fn overlapping(&self, first: usize, last: usize) -> impl Iterator<Item = usize> + '_ {
        let from = self.max_end.partition_point(|&end| end <= first);
        let to = self.by_start.partition_point(|&i| self.list[i].rows.start <= last);
        self.by_start[from..to.max(from)]
            .iter()
            .copied()
            .filter(move |&i| self.list[i].rows.end > first)
    }
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
        } else if self.by_key.get(&key).is_some_and(|set| set.list == decorations) {
            false
        } else {
            self.by_key.insert(key, KeyDecorations::new(decorations));
            true
        }
    }

    /// The paint of each of `rows` (sorted ascending, as laid out: folds
    /// skip rows), one entry per row. Later decorations win, within a key
    /// and across keys (in key order). Only the given rows are resolved, so
    /// a fold over many rows costs nothing, and only the decorations that
    /// reach from the first to the last of them are looked at.
    pub(crate) fn resolve(&self, rows: &[usize]) -> Vec<Option<RowPaint>> {
        let mut out = vec![None; rows.len()];
        let (Some(&first), Some(&last)) = (rows.first(), rows.last()) else {
            return out;
        };
        let mut hits = Vec::new();
        for set in self.by_key.values() {
            hits.clear();
            hits.extend(set.overlapping(first, last));
            // Apply in the order they were set, so later ones win.
            hits.sort_unstable();
            for decoration in hits.iter().map(|&i| &set.list[i]) {
                Self::paint(decoration, rows, &mut out);
            }
        }
        out
    }

    fn paint(decoration: &LineDecoration, rows: &[usize], out: &mut [Option<RowPaint>]) {
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

    /// The documented order across keys: JS `sort()` order, uppercase
    /// before lowercase, not the order they were set in.
    #[test]
    fn keys_apply_in_sort_order_uppercase_first() {
        let red = hsla(0., 1., 0.5, 1.);
        let green = hsla(0.33, 1., 0.5, 1.);
        let mut decorations = LineDecorations::default();
        decorations.set("diff".into(), vec![LineDecoration::new(0..1).with_background(red)]);
        decorations.set("Review".into(), vec![LineDecoration::new(0..1).with_background(green)]);
        assert_eq!(decorations.row(0).unwrap().background, Some(red), "\"diff\" sorts after \"Review\"");
        decorations.set("review".into(), vec![LineDecoration::new(0..1).with_background(green)]);
        assert_eq!(decorations.row(0).unwrap().background, Some(green), "\"review\" sorts after \"diff\"");
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
    fn overlapping_and_nested_decorations_are_found_and_keep_their_order() {
        let red = hsla(0., 1., 0.5, 1.);
        let green = hsla(0.33, 1., 0.5, 1.);
        let blue = hsla(0.66, 1., 0.5, 1.);
        let mut decorations = LineDecorations::default();
        decorations.set(
            "a".into(),
            vec![
                // Set first but starts last: still painted under the others.
                LineDecoration::new(50..60).with_background(red),
                LineDecoration::new(0..100).with_background(green),
                LineDecoration::new(10..12).with_background(blue),
                LineDecoration::new(20..30).with_gutter(red),
            ],
        );
        let rows: Vec<usize> = (40..70).collect();
        let resolved = decorations.resolve(&rows);
        assert_eq!(resolved[0].unwrap().background, Some(green), "only the long range reaches 40");
        assert_eq!(resolved[0].unwrap().gutter, None);
        assert_eq!(resolved[15].unwrap().background, Some(green), "55: green was set after red");
        assert_eq!(decorations.row(11).unwrap().background, Some(blue));
        assert_eq!(decorations.row(25).unwrap(), RowPaint { background: Some(green), gutter: Some(red) });
        assert_eq!(decorations.row(100), None);
        assert!(decorations.resolve(&[]).is_empty());
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
