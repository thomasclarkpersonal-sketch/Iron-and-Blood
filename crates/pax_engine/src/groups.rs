//! Compressed row lists ("CSR") grouping table rows by a dense key.
//!
//! Systems often need "all POPs of profession *c* in province *p*". Building
//! the grouping with a counting sort each tick is `O(rows + keys)`, needs no
//! hashing, and lists members in ascending row order, so every consumer
//! iterates them in a deterministic order.

/// Rows grouped by key: members of key `k` are `members[offsets[k]..offsets[k + 1]]`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Groups {
    offsets: Vec<u32>,
    members: Vec<u32>,
}

impl Groups {
    /// Groups rows `0..keys.len()` by `keys[row]`, which must be `< key_count`.
    pub fn build(key_count: usize, keys: &[usize]) -> Groups {
        let mut offsets = vec![0u32; key_count + 1];
        for &k in keys {
            offsets[k + 1] += 1;
        }
        for k in 0..key_count {
            offsets[k + 1] += offsets[k];
        }
        let mut cursor = offsets.clone();
        let mut members = vec![0u32; keys.len()];
        for (row, &k) in keys.iter().enumerate() {
            members[cursor[k] as usize] = row as u32;
            cursor[k] += 1;
        }
        Groups { offsets, members }
    }

    /// Rows with key `k`, in ascending order.
    pub fn members(&self, k: usize) -> &[u32] {
        &self.members[self.offsets[k] as usize..self.offsets[k + 1] as usize]
    }

    pub fn key_count(&self) -> usize {
        self.offsets.len() - 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_in_row_order() {
        let g = Groups::build(3, &[2, 0, 2, 1, 0]);
        assert_eq!(g.members(0), &[1, 4]);
        assert_eq!(g.members(1), &[3]);
        assert_eq!(g.members(2), &[0, 2]);
        assert_eq!(g.key_count(), 3);
    }
}
