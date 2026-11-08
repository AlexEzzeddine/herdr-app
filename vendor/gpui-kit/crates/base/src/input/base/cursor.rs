use std::ops::{Range, RangeBounds};

/// A selection in the text, represented by start and end byte indices.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
pub struct Selection {
    pub start: usize,
    pub end: usize,
}

impl Selection {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    pub fn len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }

    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }

    /// Clears the selection, setting start and end to 0.
    pub fn clear(&mut self) {
        self.start = 0;
        self.end = 0;
    }

    /// Checks if the given offset is within the selection range.
    pub fn contains(&self, offset: usize) -> bool {
        offset >= self.start && offset < self.end
    }
}

impl From<Range<usize>> for Selection {
    fn from(value: Range<usize>) -> Self {
        Self::new(value.start, value.end)
    }
}
impl From<Selection> for Range<usize> {
    fn from(value: Selection) -> Self {
        value.start..value.end
    }
}
impl RangeBounds<usize> for Selection {
    fn start_bound(&self) -> std::ops::Bound<&usize> {
        std::ops::Bound::Included(&self.start)
    }

    fn end_bound(&self) -> std::ops::Bound<&usize> {
        std::ops::Bound::Excluded(&self.end)
    }
}

use gpui::Pixels;

use super::selection::CursorId;

#[derive(Debug, Copy, Clone, PartialEq)]
pub(super) struct CursorSelection {
    pub(super) id: CursorId,
    pub(super) start: usize,
    pub(super) end: usize,
    pub(super) reversed: bool,
    pub(super) column_anchor: Option<(Pixels, usize)>,
}

impl CursorSelection {
    pub(super) fn new(id: CursorId, start: usize, end: usize) -> Self {
        Self {
            id,
            start,
            end,
            reversed: false,
            column_anchor: None,
        }
    }

    pub(super) fn len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }

    pub(super) fn is_empty(&self) -> bool {
        self.start == self.end
    }

    pub(super) fn clear(&mut self) {
        self.start = 0;
        self.end = 0;
    }

    pub(super) fn contains(&self, offset: usize) -> bool {
        offset >= self.start && offset < self.end
    }

    pub(super) fn cursor_offset(&self) -> usize {
        if self.reversed { self.start } else { self.end }
    }

    pub(super) fn place_at(&mut self, offset: usize, column_anchor: Option<(Pixels, usize)>) {
        self.start = offset;
        self.end = offset;
        self.reversed = false;
        self.column_anchor = column_anchor;
    }

    pub(super) fn is_collapsed(&self) -> bool {
        self.is_empty()
    }
}

impl From<Range<usize>> for CursorSelection {
    fn from(value: Range<usize>) -> Self {
        Self::new(CursorId::default(), value.start, value.end)
    }
}

impl From<CursorSelection> for Range<usize> {
    fn from(value: CursorSelection) -> Self {
        value.start..value.end
    }
}

impl RangeBounds<usize> for CursorSelection {
    fn start_bound(&self) -> std::ops::Bound<&usize> {
        std::ops::Bound::Included(&self.start)
    }

    fn end_bound(&self) -> std::ops::Bound<&usize> {
        std::ops::Bound::Excluded(&self.end)
    }
}

pub(super) struct Selections {
    selections: Vec<CursorSelection>,
    next_id: usize,
    /// HERDR: bumped by every mutating method but [`Self::active_mut`], so
    /// the owner can tell cheaply that the selections may have changed (see
    /// `InputBaseState`'s selection-change report).
    generation: u64,
    /// HERDR: the active selection before the first [`Self::active_mut`]
    /// since [`Self::mark_seen`]. Many callers write it with the value it
    /// already has (`reversed = false` on every click), so the write counts
    /// as a change only if the value differs ([`Self::generation`]).
    active_before: Option<CursorSelection>,
}

/// HERDR: [`Selections::stamp`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct SelectionsStamp {
    counter: u64,
    active: CursorSelection,
}

impl Selections {
    pub(super) fn new() -> Self {
        Self {
            selections: vec![CursorSelection::new(CursorId::new(0), 0, 0)],
            next_id: 1,
            generation: 0,
            active_before: None,
        }
    }

    /// Returns the active selection.
    pub(super) fn active(&self) -> &CursorSelection {
        self.selections
            .first()
            .expect("Selections always has at least one selection")
    }

    /// Returns a mutable reference to the active selection.
    pub(super) fn active_mut(&mut self) -> &mut CursorSelection {
        if self.active_before.is_none() {
            self.active_before = Some(*self.active());
        }
        self.selections
            .first_mut()
            .expect("Selections always has at least one selection")
    }

    /// HERDR: changes whenever a mutating method ran or the active
    /// selection was written with a new value, since [`Self::mark_seen`]
    /// (a method may not have changed anything; equal generations mean
    /// nothing changed). Compare it only with the value `mark_seen`
    /// returned: before the next mark it can count a changed active
    /// selection as +1 and then repeat that number after a real change. A
    /// caller that doesn't mark compares [`Self::stamp`]s instead.
    pub(super) fn generation(&self) -> u64 {
        let active_changed = self.active_before.is_some_and(|before| before != *self.active());
        self.generation + u64::from(active_changed)
    }

    /// HERDR: the selections now, for comparing later without
    /// [`Self::mark_seen`]: equal stamps mean nothing changed since. The
    /// counter only grows, and the active selection is compared by value,
    /// so writing it back to what it was compares equal.
    pub(super) fn stamp(&self) -> SelectionsStamp {
        SelectionsStamp { counter: self.generation, active: *self.active() }
    }

    /// HERDR: the owner has seen the selections at [`Self::generation`]:
    /// later writes of the active selection compare against it as it is now.
    pub(super) fn mark_seen(&mut self) -> u64 {
        self.generation = self.generation();
        self.active_before = None;
        self.generation
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = &CursorSelection> {
        self.selections.iter()
    }

    /// Returns the number of selections (always `>= 1`).
    pub(super) fn len(&self) -> usize {
        self.selections.len()
    }

    /// Returns true when there is exactly one selection.
    pub(super) fn is_single(&self) -> bool {
        self.selections.len() == 1
    }

    /// Generates a new unique cursor id.
    pub(super) fn generate_id(&mut self) -> CursorId {
        let id = CursorId::new(self.next_id);
        self.next_id += 1;
        id
    }

    /// Adds an additional selection.
    pub(super) fn add(&mut self, selection: CursorSelection) {
        self.generation += 1;
        self.selections.push(selection);
    }

    /// Replaces all selections. Ignores an empty vec to keep the
    /// "always at least one selection" invariant.
    pub(super) fn replace_all(&mut self, selections: Vec<CursorSelection>) {
        if !selections.is_empty() {
            self.generation += 1;
            self.selections = selections;
        }
    }

    /// Removes every selection except the active one (index 0).
    pub(super) fn remove_all_but_active(&mut self) {
        self.generation += 1;
        self.selections.truncate(1);
    }

    /// Merges overlapping selections.
    ///
    /// Selections are sorted by start and folded together when they overlap.
    /// The active selection is preserved, propagated onto
    /// the merged result if it was absorbed, and re-fronted afterwards.
    pub(super) fn merge_overlapping(&mut self) {
        if self.selections.len() <= 1 {
            return;
        }

        self.generation += 1;
        self.selections = merged(std::mem::take(&mut self.selections));
    }
}

/// HERDR: `selections` (the active one first) merged as
/// [`Selections::merge_overlapping`] does: sorted by start, overlapping or
/// adjacent ones folded together, the active one carried onto the result
/// it was folded into and put first again. A function of the list alone
/// (the active one is found by position, not id), so a caller can merge
/// selections it hasn't installed yet: `set_selections` compares the result
/// with the current selections before giving them ids.
pub(super) fn merged(mut selections: Vec<CursorSelection>) -> Vec<CursorSelection> {
    if selections.len() <= 1 {
        return selections;
    }
    let active = selections[0];
    // Stable, so selections with equal starts keep their order.
    selections.sort_by_key(|s| s.start);

    let mut merged: Vec<CursorSelection> = Vec::with_capacity(selections.len());
    let mut active_at = None;
    let mut active_seen = false;
    for selection in selections {
        // The first selection equal to the active one is it: the sort is
        // stable and it came first.
        let is_active = !active_seen && selection == active;
        active_seen |= is_active;
        let last_at = merged.len().wrapping_sub(1);
        if let Some(last) = merged.last_mut() {
            if selection.start <= last.end {
                // Overlapping or adjacent, extend the last one.
                let did_merge = selection.start != last.start || selection.end != last.end;
                last.end = last.end.max(selection.end);
                if is_active {
                    last.id = selection.id;
                    last.reversed = selection.reversed;
                    active_at = Some(last_at);
                }
                // Reset the column anchor on a real merge.
                if did_merge {
                    last.column_anchor = None;
                }
                continue;
            }
        }
        if is_active {
            active_at = Some(merged.len());
        }
        merged.push(selection);
    }

    // Re-front the active selection so it stays at index 0.
    if let Some(pos) = active_at {
        merged.swap(0, pos);
    }
    merged
}

impl Default for Selections {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::Position;
    use gpui::px;

    // HERDR: `merged` finds the active selection by position, so it
    // merges selections that share an id (`set_selections` gives ids only
    // after merging), carrying the active one onto what it was folded into.
    #[test]
    fn merged_carries_the_active_selection_without_distinct_ids() {
        let id = CursorId::new(0);
        let mut active = CursorSelection::new(id, 5, 10);
        active.reversed = true;
        let result = merged(vec![active, CursorSelection::new(id, 0, 5), CursorSelection::new(id, 20, 20)]);
        assert_eq!(result.iter().map(|s| (s.start, s.end)).collect::<Vec<_>>(), vec![(0, 10), (20, 20)]);
        assert!(result[0].reversed, "the active selection's side is kept");
        let result = merged(vec![CursorSelection::new(id, 20, 20), CursorSelection::new(id, 0, 5)]);
        assert_eq!((result[0].start, result[0].end), (20, 20), "the active one stays first");
    }

    #[test]
    fn selection_keeps_its_public_range_api() {
        fn assert_eq<T: Eq>() {}

        assert_eq::<Selection>();
        let selection = Selection::new(2, 5);
        assert_eq!(selection, Selection { start: 2, end: 5 });
        assert_eq!(Range::<usize>::from(selection), 2..5);
    }

    #[test]
    fn test_line_column_from_to() {
        assert_eq!(
            Position::new(1, 2),
            Position {
                line: 1,
                character: 2
            }
        );
    }

    #[test]
    fn test_cursor_offset_reversed() {
        let mut sel = CursorSelection::new(CursorId::new(0), 5, 10);
        assert_eq!(sel.cursor_offset(), 10);
        sel.reversed = true;
        assert_eq!(sel.cursor_offset(), 5);
    }

    #[test]
    fn test_place_at() {
        let mut sel = CursorSelection::new(CursorId::new(0), 5, 10);
        sel.reversed = true;
        sel.place_at(7, Some((px(12.), 3)));
        assert_eq!(sel.start, 7);
        assert_eq!(sel.end, 7);
        assert!(sel.is_collapsed());
        assert!(!sel.reversed);
        assert_eq!(sel.column_anchor, Some((px(12.), 3)));
    }

    #[test]
    fn test_selections_never_empty() {
        let selections = Selections::new();
        assert_eq!(selections.len(), 1);
        assert_eq!(selections.active().id, CursorId::new(0));

        let default = Selections::default();
        assert_eq!(default.len(), 1);
    }

    #[test]
    fn test_selections_active_mut() {
        let mut selections = Selections::new();
        selections.active_mut().place_at(4, None);
        assert_eq!(selections.active().cursor_offset(), 4);
    }

    #[test]
    fn test_selections_merge_overlapping() {
        let mut selections = Selections::new();

        let id1 = selections.generate_id();
        let id2 = selections.generate_id();
        let id3 = selections.generate_id();

        // id1 is the active selection (index 0).
        selections.replace_all(vec![
            CursorSelection::new(id1, 0, 10),
            CursorSelection::new(id2, 5, 15), // Overlaps with the first.
            CursorSelection::new(id3, 20, 30), // Non-overlapping.
        ]);

        selections.merge_overlapping();

        // After merge: (0, 15) and (20, 30).
        assert_eq!(selections.len(), 2);
        // The active selection stays at index 0 and carries its id.
        assert_eq!(selections.active().id, id1);
        assert_eq!(
            (selections.active().start, selections.active().end),
            (0, 15)
        );

        let ranges: Vec<_> = selections.iter().map(|s| (s.start, s.end)).collect();
        assert!(ranges.contains(&(0, 15)));
        assert!(ranges.contains(&(20, 30)));
    }

    #[test]
    fn merging_preserves_the_active_selection_direction() {
        let mut selections = Selections::new();
        let active_id = selections.generate_id();
        let other_id = selections.generate_id();
        let mut active = CursorSelection::new(active_id, 5, 15);
        active.reversed = true;
        let other = CursorSelection::new(other_id, 0, 10);
        selections.replace_all(vec![active, other]);

        selections.merge_overlapping();

        assert_eq!(selections.active().id, active_id);
        assert!(selections.active().reversed);
    }
}
