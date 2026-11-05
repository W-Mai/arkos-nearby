use serde::{Deserialize, Serialize};

pub const MAX_WINDOW_ITEMS: usize = 128;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Grid {
    pub columns: usize,
    pub rows: usize,
}

impl Default for Grid {
    fn default() -> Self {
        Self {
            columns: 3,
            rows: 1,
        }
    }
}

impl Grid {
    pub fn valid(self) -> bool {
        self.columns > 0
            && self.rows > 0
            && self
                .columns
                .checked_mul(self.rows)
                .is_some_and(|capacity| capacity <= MAX_WINDOW_ITEMS)
    }

    pub fn capacity(self) -> usize {
        self.columns * self.rows
    }

    pub fn first(self, selected: usize) -> usize {
        selected / self.capacity() * self.capacity()
    }

    pub fn navigate(self, selected: usize, count: usize, action: &str) -> usize {
        if count == 0 {
            return 0;
        }
        let selected = selected.min(count - 1);
        let first = self.first(selected);
        let local = selected - first;
        let row = local / self.columns;
        let column = local % self.columns;
        let next = match action {
            "up" if row > 0 => Some(selected - self.columns),
            "up" if first > 0 => Some(first - self.columns + column),
            "down" if row + 1 < self.rows && selected + self.columns < count => {
                Some(selected + self.columns)
            }
            "down" if first + self.capacity() < count => Some(first + self.capacity() + column),
            "down" if row + 1 < self.rows => Some(count - 1),
            "left" if column > 0 => Some(selected - 1),
            "right" if column + 1 < self.columns => (selected + 1 < count).then_some(selected + 1),
            "left" if first > 0 => {
                Some(first - self.capacity() + row * self.columns + self.columns - 1)
            }
            "right" if first + self.capacity() < count => {
                Some(first + self.capacity() + row * self.columns)
            }
            "page_up" if first > 0 => Some(selected - self.capacity()),
            "page_down" if first + self.capacity() < count => Some(selected + self.capacity()),
            _ => None,
        };
        next.unwrap_or(selected).min(count - 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeSet, VecDeque};

    #[test]
    fn bounded_grids_reach_every_entry_and_preserve_page_coordinates() {
        for columns in 1..=4 {
            for rows in 1..=7 {
                let grid = Grid { columns, rows };
                assert!(grid.valid());
                for count in 1..=35 {
                    let mut seen = BTreeSet::from([0]);
                    let mut queue = VecDeque::from([0]);
                    while let Some(selected) = queue.pop_front() {
                        for action in ["up", "down", "left", "right", "page_up", "page_down"] {
                            let next = grid.navigate(selected, count, action);
                            assert!(next < count);
                            assert!(next - grid.first(next) < grid.capacity());
                            if seen.insert(next) {
                                queue.push_back(next);
                            }
                        }
                    }
                    assert_eq!(seen.len(), count);
                }
            }
        }
        assert!(
            !Grid {
                columns: 0,
                rows: 1
            }
            .valid()
        );
        assert!(
            !Grid {
                columns: usize::MAX,
                rows: 2
            }
            .valid()
        );
        assert!(
            !Grid {
                columns: 3,
                rows: MAX_WINDOW_ITEMS
            }
            .valid()
        );
        let grid = Grid {
            columns: 3,
            rows: 2,
        };
        assert_eq!(grid.navigate(2, 8, "right"), 6);
        assert_eq!(grid.navigate(5, 8, "right"), 7);
        assert_eq!(grid.navigate(6, 8, "left"), 2);
        assert_eq!(grid.navigate(4, 8, "up"), 1);
        assert_eq!(grid.navigate(3, 8, "down"), 6);
        assert_eq!(grid.navigate(6, 8, "up"), 3);
        assert_eq!(grid.navigate(4, 8, "down"), 7);
        assert_eq!(grid.navigate(6, 8, "down"), 7);
    }
}
