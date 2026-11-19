//! Agregacion pura del resumen de una ejecucion (design D-23).

use std::collections::BTreeMap;

use crate::domain::model::{CleanupOutcome, EpochMillis, ScanId};

use super::model::{
    CategorySummary, CleanupCategory, CleanupPreview, CleanupStatus, CleanupSummary, PreviewId,
    ReasonCount,
};

/// Acumulador de resultados item por item.
pub(super) struct Tally {
    preview_id: PreviewId,
    scan_id: ScanId,
    total_items: u64,
    processed: u64,
    moved_count: u64,
    moved_bytes: u64,
    skipped_locked: u64,
    skipped_protected: u64,
    skipped_cloud: u64,
    errors: u64,
    reasons: BTreeMap<(&'static str, String), u64>,
    categories: Vec<CategorySummary>,
}

impl Tally {
    pub(super) fn new(preview: &CleanupPreview) -> Self {
        Self {
            preview_id: preview.id(),
            scan_id: preview.scan_id(),
            total_items: preview.items().len() as u64,
            processed: 0,
            moved_count: 0,
            moved_bytes: 0,
            skipped_locked: 0,
            skipped_protected: 0,
            skipped_cloud: 0,
            errors: 0,
            reasons: BTreeMap::new(),
            categories: preview
                .categories()
                .iter()
                .map(|c| CategorySummary {
                    category: c.category,
                    moved_count: 0,
                    moved_bytes: 0,
                    skipped_count: 0,
                    error_count: 0,
                })
                .collect(),
        }
    }

    pub(super) fn processed(&self) -> u64 {
        self.processed
    }

    pub(super) fn moved(&self) -> (u64, u64) {
        (self.moved_count, self.moved_bytes)
    }

    pub(super) fn total(&self) -> u64 {
        self.total_items
    }

    fn category_mut(&mut self, category: CleanupCategory) -> &mut CategorySummary {
        let index = self
            .categories
            .iter()
            .position(|c| c.category == category)
            .unwrap_or_else(|| {
                self.categories.push(CategorySummary {
                    category,
                    moved_count: 0,
                    moved_bytes: 0,
                    skipped_count: 0,
                    error_count: 0,
                });
                self.categories.len() - 1
            });
        &mut self.categories[index]
    }

    /// Anota el resultado de un item; `detail` es `None` solo si se movio.
    pub(super) fn record(
        &mut self,
        category: CleanupCategory,
        outcome: CleanupOutcome,
        detail: Option<&str>,
        size_bytes: u64,
    ) {
        self.processed += 1;
        match outcome {
            CleanupOutcome::MovedToTrash => {
                self.moved_count += 1;
                self.moved_bytes += size_bytes;
                let entry = self.category_mut(category);
                entry.moved_count += 1;
                entry.moved_bytes += size_bytes;
                return;
            }
            CleanupOutcome::SkippedLocked => {
                self.skipped_locked += 1;
                self.category_mut(category).skipped_count += 1;
            }
            CleanupOutcome::SkippedProtected => {
                self.skipped_protected += 1;
                self.category_mut(category).skipped_count += 1;
            }
            CleanupOutcome::SkippedCloud => {
                self.skipped_cloud += 1;
                self.category_mut(category).skipped_count += 1;
            }
            CleanupOutcome::Error | CleanupOutcome::RecycleBinEmptied => {
                self.errors += 1;
                self.category_mut(category).error_count += 1;
            }
        }
        let key = (outcome.as_str(), detail.unwrap_or_default().to_string());
        *self.reasons.entry(key).or_insert(0) += 1;
    }

    pub(super) fn finish(
        self,
        status: CleanupStatus,
        started_at: EpochMillis,
        finished_at: EpochMillis,
    ) -> CleanupSummary {
        let mut reasons: Vec<ReasonCount> = self
            .reasons
            .into_iter()
            .filter_map(|((outcome, detail), count)| {
                CleanupOutcome::parse(outcome).map(|outcome| ReasonCount {
                    outcome,
                    detail,
                    count,
                })
            })
            .collect();
        // Estable: mas frecuentes primero; el desempate ya viene ordenado por el mapa.
        reasons.sort_by_key(|reason| std::cmp::Reverse(reason.count));
        CleanupSummary {
            preview_id: self.preview_id,
            scan_id: self.scan_id,
            status,
            started_at,
            finished_at,
            total_items: self.total_items,
            processed: self.processed,
            not_processed: self.total_items.saturating_sub(self.processed),
            moved_count: self.moved_count,
            moved_bytes: self.moved_bytes,
            skipped_locked: self.skipped_locked,
            skipped_protected: self.skipped_protected,
            skipped_cloud: self.skipped_cloud,
            errors: self.errors,
            reasons,
            categories: self.categories,
        }
    }
}
