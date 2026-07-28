use std::collections::{BTreeMap, BTreeSet};
use std::ops::Bound::{Excluded, Unbounded};

use jellyflow_core::core::{CanvasPoint, CanvasRect};

const SPATIAL_BASE_CELL_SIZE: f32 = 256.0;
const MAX_SPATIAL_CELL_LEVEL: u8 = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct SpatialCell {
    pub level: u8,
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum SpatialQuerySource {
    Cell(SpatialCell),
    Level(u8),
}

#[derive(Debug)]
pub(super) struct SpatialQueryCursor {
    bounds: CanvasRect,
    max_cells: i64,
    last_level: Option<u8>,
    state: SpatialQueryCursorState,
}

impl SpatialQueryCursor {
    pub(super) fn new(bounds: CanvasRect, max_cells: i64) -> Self {
        Self {
            bounds,
            max_cells,
            last_level: None,
            state: SpatialQueryCursorState::FindLevel,
        }
    }
}

#[derive(Debug)]
enum SpatialQueryCursorState {
    FindLevel,
    Cells(SpatialCellRange),
    WholeLevel(u8),
    Exhausted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SpatialQueryStep {
    LevelInitialized,
    Source(SpatialQuerySource),
    Exhausted,
}

#[derive(Debug)]
struct SpatialCellRange {
    level: u8,
    min_x: i32,
    next_x: i32,
    next_y: i32,
    max_x: i32,
    max_y: i32,
}

impl SpatialCellRange {
    fn new(bounds: CanvasRect, level: u8, limit: i64) -> Option<Self> {
        let (min_x, min_y, max_x, max_y) = spatial_cell_bounds(bounds, level, limit)?;
        Some(Self {
            level,
            min_x,
            next_x: min_x,
            next_y: min_y,
            max_x,
            max_y,
        })
    }

    fn advance(&mut self) -> (SpatialCell, bool) {
        let cell = SpatialCell {
            level: self.level,
            x: self.next_x,
            y: self.next_y,
        };
        let exhausted = self.next_x == self.max_x && self.next_y == self.max_y;
        if !exhausted {
            if self.next_x == self.max_x {
                self.next_x = self.min_x;
                self.next_y = self.next_y.saturating_add(1);
            } else {
                self.next_x = self.next_x.saturating_add(1);
            }
        }
        (cell, exhausted)
    }
}

#[derive(Debug)]
pub(super) struct HierarchicalSpatialIndex<Key> {
    cells: BTreeMap<SpatialCell, BTreeSet<Key>>,
    entries: BTreeMap<Key, Vec<SpatialCell>>,
    levels: BTreeMap<u8, BTreeSet<Key>>,
}

impl<Key> Default for HierarchicalSpatialIndex<Key> {
    fn default() -> Self {
        Self {
            cells: BTreeMap::new(),
            entries: BTreeMap::new(),
            levels: BTreeMap::new(),
        }
    }
}

impl<Key: Copy + Ord> HierarchicalSpatialIndex<Key> {
    pub fn upsert(&mut self, key: Key, cells: Vec<SpatialCell>) {
        self.remove(key);
        let level = cells.first().map_or(0, |cell| cell.level);
        self.levels.entry(level).or_default().insert(key);
        for cell in &cells {
            self.cells.entry(*cell).or_default().insert(key);
        }
        self.entries.insert(key, cells);
    }

    pub fn remove(&mut self, key: Key) -> bool {
        let Some(cells) = self.entries.remove(&key) else {
            return false;
        };
        if let Some(level) = cells.first().map(|cell| cell.level) {
            let remove_level = self.levels.get_mut(&level).is_some_and(|entries| {
                entries.remove(&key);
                entries.is_empty()
            });
            if remove_level {
                self.levels.remove(&level);
            }
        }
        for cell in cells {
            let remove_cell = self.cells.get_mut(&cell).is_some_and(|entries| {
                entries.remove(&key);
                entries.is_empty()
            });
            if remove_cell {
                self.cells.remove(&cell);
            }
        }
        true
    }

    pub fn clear(&mut self) {
        self.cells.clear();
        self.entries.clear();
        self.levels.clear();
    }

    pub fn advance_query(&self, cursor: &mut SpatialQueryCursor) -> SpatialQueryStep {
        match &mut cursor.state {
            SpatialQueryCursorState::FindLevel => {
                let next_level = cursor.last_level.map_or_else(
                    || self.levels.keys().next().copied(),
                    |last| {
                        self.levels
                            .range((Excluded(last), Unbounded))
                            .next()
                            .map(|(&level, _)| level)
                    },
                );
                let Some(level) = next_level else {
                    cursor.state = SpatialQueryCursorState::Exhausted;
                    return SpatialQueryStep::Exhausted;
                };
                cursor.last_level = Some(level);
                cursor.state = SpatialCellRange::new(cursor.bounds, level, cursor.max_cells)
                    .map_or(
                        SpatialQueryCursorState::WholeLevel(level),
                        SpatialQueryCursorState::Cells,
                    );
                SpatialQueryStep::LevelInitialized
            }
            SpatialQueryCursorState::Cells(cells) => {
                let (cell, exhausted) = cells.advance();
                if exhausted {
                    cursor.state = SpatialQueryCursorState::FindLevel;
                }
                SpatialQueryStep::Source(SpatialQuerySource::Cell(cell))
            }
            SpatialQueryCursorState::WholeLevel(level) => {
                let level = *level;
                cursor.state = SpatialQueryCursorState::FindLevel;
                SpatialQueryStep::Source(SpatialQuerySource::Level(level))
            }
            SpatialQueryCursorState::Exhausted => SpatialQueryStep::Exhausted,
        }
    }

    pub fn source(&self, source: SpatialQuerySource) -> Option<&BTreeSet<Key>> {
        match source {
            SpatialQuerySource::Cell(cell) => self.cells.get(&cell),
            SpatialQuerySource::Level(level) => self.levels.get(&level),
        }
    }
}

pub(super) fn bounded_spatial_cells_for_rect(
    bounds: CanvasRect,
    max_cells: i64,
) -> Vec<SpatialCell> {
    for level in 0..=MAX_SPATIAL_CELL_LEVEL {
        if let Some(cells) = spatial_cells_for_rect(bounds, level, max_cells) {
            return cells;
        }
    }
    unreachable!("the coarsest spatial grid covers every finite f32 rectangle")
}

pub(super) fn spatial_cells_for_rect(
    bounds: CanvasRect,
    level: u8,
    limit: i64,
) -> Option<Vec<SpatialCell>> {
    let (min_x, min_y, max_x, max_y) = spatial_cell_bounds(bounds, level, limit)?;
    let width = i64::from(max_x) - i64::from(min_x) + 1;
    let height = i64::from(max_y) - i64::from(min_y) + 1;
    let mut cells = Vec::with_capacity((width * height) as usize);
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            cells.push(SpatialCell { level, x, y });
        }
    }
    Some(cells)
}

fn spatial_cell_bounds(bounds: CanvasRect, level: u8, limit: i64) -> Option<(i32, i32, i32, i32)> {
    if limit <= 0 || !bounds.is_positive_finite() {
        return None;
    }
    let cell_size = spatial_cell_size(level);
    let min_x = spatial_cell_coord(f64::from(bounds.origin.x), cell_size);
    let min_y = spatial_cell_coord(f64::from(bounds.origin.y), cell_size);
    let max_x = spatial_cell_coord(
        f64::from(bounds.origin.x) + f64::from(bounds.size.width),
        cell_size,
    );
    let max_y = spatial_cell_coord(
        f64::from(bounds.origin.y) + f64::from(bounds.size.height),
        cell_size,
    );
    let width = i64::from(max_x) - i64::from(min_x) + 1;
    let height = i64::from(max_y) - i64::from(min_y) + 1;
    (width > 0 && height > 0 && i128::from(width) * i128::from(height) <= i128::from(limit))
        .then_some((min_x, min_y, max_x, max_y))
}

pub(super) fn spatial_cell_size(level: u8) -> f64 {
    f64::from(SPATIAL_BASE_CELL_SIZE) * 16.0_f64.powi(i32::from(level))
}

pub(super) fn spatial_cell_coords(point: CanvasPoint, cell_size: f64) -> (i32, i32) {
    (
        spatial_cell_coord(f64::from(point.x), cell_size),
        spatial_cell_coord(f64::from(point.y), cell_size),
    )
}

fn spatial_cell_coord(value: f64, cell_size: f64) -> i32 {
    (value / cell_size).floor() as i32
}

pub(super) const fn max_spatial_cell_level() -> u8 {
    MAX_SPATIAL_CELL_LEVEL
}

/// Includes one level check and at most `max_cells_per_level` sources per occupied level,
/// followed by the terminal exhausted-level check.
pub(super) const fn max_spatial_query_setup_work(max_cells_per_level: u64) -> u64 {
    (MAX_SPATIAL_CELL_LEVEL as u64 + 1) * (max_cells_per_level + 1) + 1
}
