/// Work performed while merging revisioned geometry-impact records.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GeometryImpactMergeWork {
    patches_merged: u64,
    records_merged: u64,
    global_fallbacks: u64,
}

impl GeometryImpactMergeWork {
    pub(crate) const fn new(
        patches_merged: u64,
        records_merged: u64,
        global_fallbacks: u64,
    ) -> Self {
        Self {
            patches_merged,
            records_merged,
            global_fallbacks,
        }
    }

    /// Returns the number of revisioned patches admitted into the impact merge.
    #[must_use]
    pub const fn patches_merged(self) -> u64 {
        self.patches_merged
    }

    /// Returns the number of affected-geometry records merged from those patches.
    #[must_use]
    pub const fn records_merged(self) -> u64 {
        self.records_merged
    }

    /// Returns the number of merges promoted to global invalidation for safety.
    #[must_use]
    pub const fn global_fallbacks(self) -> u64 {
        self.global_fallbacks
    }
}
