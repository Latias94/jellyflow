use std::collections::{BTreeMap, VecDeque};

use jellyflow_core::NodeId;
use jellyflow_runtime::view::NodeInstanceKey;

pub(super) struct RetainedMountStore<T> {
    active: BTreeMap<NodeInstanceKey, ActiveMount<T>>,
    active_order: Vec<NodeInstanceKey>,
    parked: BTreeMap<NodeInstanceKey, ParkedMount<T>>,
    retained_instances: BTreeMap<NodeId, NodeInstanceKey>,
    eligible_lru: VecDeque<(u64, NodeInstanceKey)>,
    eligible_parked_count: usize,
    next_park_sequence: u64,
    eligible_parked_capacity: usize,
    restores: u64,
    evictions: u64,
    hard_retirements: u64,
    strong_instance_high_water: usize,
    reconcile_epoch: u64,
    #[cfg(test)]
    active_insertions: u64,
    #[cfg(test)]
    active_detachments: u64,
}

struct ActiveMount<T> {
    value: T,
    seen_epoch: u64,
}

struct ParkedMount<T> {
    value: T,
    eligible_sequence: Option<u64>,
}

pub(super) struct RetiredMount<T> {
    pub(super) instance: NodeInstanceKey,
    pub(super) value: T,
}

pub(super) struct EvictedMount<T> {
    pub(super) instance: NodeInstanceKey,
    pub(super) value: T,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct RetainedMountSnapshot {
    pub(super) active_nodes: usize,
    pub(super) parked_nodes: usize,
    pub(super) restores: u64,
    pub(super) evictions: u64,
    pub(super) hard_retirements: u64,
    pub(super) eligible_parked_nodes: usize,
    pub(super) eligible_lru_entries: usize,
    pub(super) strong_instances: usize,
    pub(super) strong_instance_high_water: usize,
}

impl<T> RetainedMountStore<T> {
    pub(super) fn new(eligible_parked_capacity: usize) -> Self {
        Self {
            active: BTreeMap::new(),
            active_order: Vec::new(),
            parked: BTreeMap::new(),
            retained_instances: BTreeMap::new(),
            eligible_lru: VecDeque::new(),
            eligible_parked_count: 0,
            next_park_sequence: 1,
            eligible_parked_capacity,
            restores: 0,
            evictions: 0,
            hard_retirements: 0,
            strong_instance_high_water: 0,
            reconcile_epoch: 0,
            #[cfg(test)]
            active_insertions: 0,
            #[cfg(test)]
            active_detachments: 0,
        }
    }

    pub(super) fn retained_instance(&self, node: NodeId) -> Option<&NodeInstanceKey> {
        self.retained_instances.get(&node)
    }

    pub(super) fn get(&self, instance: &NodeInstanceKey) -> Option<&T> {
        self.active
            .get(instance)
            .map(|active| &active.value)
            .or_else(|| self.parked.get(instance).map(|parked| &parked.value))
    }

    pub(super) fn get_mut(&mut self, instance: &NodeInstanceKey) -> Option<&mut T> {
        if let Some(active) = self.active.get_mut(instance) {
            return Some(&mut active.value);
        }
        self.parked
            .get_mut(instance)
            .map(|parked| &mut parked.value)
    }

    pub(super) fn active_values(&self) -> impl Iterator<Item = &T> {
        self.active.values().map(|active| &active.value)
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = (&NodeInstanceKey, &T)> {
        self.active
            .iter()
            .map(|(instance, active)| (instance, &active.value))
            .chain(
                self.parked
                    .iter()
                    .map(|(instance, parked)| (instance, &parked.value)),
            )
    }

    pub(super) fn active_in_order(&self) -> impl Iterator<Item = (&NodeInstanceKey, &T)> {
        self.active_order.iter().filter_map(|instance| {
            self.active
                .get(instance)
                .map(|active| (instance, &active.value))
        })
    }

    pub(super) fn parked_instances(&self) -> impl Iterator<Item = &NodeInstanceKey> {
        self.parked.keys()
    }

    pub(super) fn begin_reconcile(&mut self) {
        if self.reconcile_epoch == u64::MAX {
            for active in self.active.values_mut() {
                active.seen_epoch = 0;
            }
            self.reconcile_epoch = 1;
        } else {
            self.reconcile_epoch += 1;
        }
        self.active_order.clear();
    }

    pub(super) fn visit_active(&mut self, instance: &NodeInstanceKey) -> bool {
        let Some(active) = self.active.get_mut(instance) else {
            return false;
        };
        if active.seen_epoch != self.reconcile_epoch {
            active.seen_epoch = self.reconcile_epoch;
            self.active_order.push(instance.clone());
        }
        true
    }

    pub(super) fn restore_active(&mut self, instance: &NodeInstanceKey) -> bool {
        if self.active.contains_key(instance) {
            return self.visit_active(instance);
        }
        let Some(mut parked) = self.parked.remove(instance) else {
            return false;
        };
        if parked.eligible_sequence.take().is_some() {
            self.eligible_parked_count = self.eligible_parked_count.saturating_sub(1);
        }
        self.restores = self.restores.saturating_add(1);
        self.active.insert(
            instance.clone(),
            ActiveMount {
                value: parked.value,
                seen_epoch: self.reconcile_epoch,
            },
        );
        self.active_order.push(instance.clone());
        #[cfg(test)]
        {
            self.active_insertions = self.active_insertions.saturating_add(1);
        }
        true
    }

    pub(super) fn insert_active(
        &mut self,
        instance: NodeInstanceKey,
        value: T,
    ) -> Option<EvictedMount<T>> {
        let was_seen = self
            .active
            .get(&instance)
            .is_some_and(|active| active.seen_epoch == self.reconcile_epoch);
        let replaced = self.active.insert(
            instance.clone(),
            ActiveMount {
                value,
                seen_epoch: self.reconcile_epoch,
            },
        );
        if !was_seen {
            self.active_order.push(instance.clone());
        }
        #[cfg(test)]
        {
            self.active_insertions = self.active_insertions.saturating_add(1);
        }
        self.track_instance(&instance);
        replaced.map(|active| {
            self.evictions = self.evictions.saturating_add(1);
            EvictedMount {
                instance,
                value: active.value,
            }
        })
    }

    pub(super) fn take_unseen_active(&mut self) -> Vec<(NodeInstanceKey, T)> {
        let reconcile_epoch = self.reconcile_epoch;
        let detached = self
            .active
            .extract_if(.., |_, active| active.seen_epoch != reconcile_epoch)
            .map(|(instance, active)| (instance, active.value))
            .collect::<Vec<_>>();
        #[cfg(test)]
        {
            self.active_detachments = self
                .active_detachments
                .saturating_add(u64::try_from(detached.len()).unwrap_or(u64::MAX));
        }
        detached
    }

    pub(super) fn park(
        &mut self,
        instance: NodeInstanceKey,
        value: T,
        protected: bool,
    ) -> Option<EvictedMount<T>> {
        let replaced = self.parked.insert(
            instance.clone(),
            ParkedMount {
                value,
                eligible_sequence: None,
            },
        );
        let replaced = replaced.map(|parked| {
            if parked.eligible_sequence.is_some() {
                self.eligible_parked_count = self.eligible_parked_count.saturating_sub(1);
            }
            self.evictions = self.evictions.saturating_add(1);
            EvictedMount {
                instance: instance.clone(),
                value: parked.value,
            }
        });
        self.track_instance(&instance);
        if !protected {
            self.mark_parked_eligible(&instance);
        }
        replaced
    }

    pub(super) fn refresh_eligibility(&mut self, mut is_protected: impl FnMut(&T) -> bool) {
        let mut newly_protected = Vec::new();
        let mut newly_eligible = Vec::new();
        for (instance, parked) in &self.parked {
            match (
                parked.eligible_sequence.is_some(),
                is_protected(&parked.value),
            ) {
                (true, true) => newly_protected.push(instance.clone()),
                (false, false) => newly_eligible.push(instance.clone()),
                (true, false) | (false, true) => {}
            }
        }

        for instance in newly_protected {
            if self
                .parked
                .get_mut(&instance)
                .and_then(|parked| parked.eligible_sequence.take())
                .is_some()
            {
                self.eligible_parked_count = self.eligible_parked_count.saturating_sub(1);
            }
        }

        for instance in newly_eligible {
            self.mark_parked_eligible(&instance);
        }
        self.compact_eligible_lru_if_needed();
    }

    pub(super) fn evict_eligible_overflow(&mut self) -> Vec<EvictedMount<T>> {
        let mut evicted = Vec::new();
        while self.eligible_parked_count > self.eligible_parked_capacity {
            let Some((sequence, instance)) = self.eligible_lru.pop_front() else {
                break;
            };
            let is_current = self
                .parked
                .get(&instance)
                .is_some_and(|parked| parked.eligible_sequence == Some(sequence));
            if !is_current {
                continue;
            }
            if let Some(parked) = self.parked.remove(&instance) {
                self.eligible_parked_count = self.eligible_parked_count.saturating_sub(1);
                self.untrack_instance(&instance);
                self.evictions = self.evictions.saturating_add(1);
                evicted.push(EvictedMount {
                    instance,
                    value: parked.value,
                });
            }
        }
        evicted
    }

    pub(super) fn finish_reconcile(&mut self) {
        debug_assert_eq!(self.active_order.len(), self.active.len());
        self.refresh_high_water();
    }

    pub(super) fn retire(&mut self, instance: &NodeInstanceKey) -> Option<RetiredMount<T>> {
        let value = if let Some(active) = self.active.remove(instance) {
            self.active_order.retain(|current| current != instance);
            Some(active.value)
        } else {
            self.parked.remove(instance).map(|mut parked| {
                if parked.eligible_sequence.take().is_some() {
                    self.eligible_parked_count = self.eligible_parked_count.saturating_sub(1);
                }
                parked.value
            })
        }?;
        Some(self.retire_detached(instance.clone(), value))
    }

    pub(super) fn retire_detached(
        &mut self,
        instance: NodeInstanceKey,
        value: T,
    ) -> RetiredMount<T> {
        self.untrack_instance(&instance);
        self.hard_retirements = self.hard_retirements.saturating_add(1);
        RetiredMount { instance, value }
    }

    pub(super) fn retire_all(&mut self) -> Vec<T> {
        let retired_count = self.active.len().saturating_add(self.parked.len());
        let active = std::mem::take(&mut self.active)
            .into_values()
            .map(|active| active.value);
        let parked = std::mem::take(&mut self.parked)
            .into_values()
            .map(|parked| parked.value);
        let retired = active.chain(parked).collect::<Vec<_>>();
        self.retained_instances.clear();
        self.active_order.clear();
        self.eligible_lru.clear();
        self.eligible_parked_count = 0;
        self.next_park_sequence = 1;
        self.hard_retirements = self
            .hard_retirements
            .saturating_add(u64::try_from(retired_count).unwrap_or(u64::MAX));
        retired
    }

    pub(super) fn snapshot(&self) -> RetainedMountSnapshot {
        let strong_instances = self.active.len().saturating_add(self.parked.len());
        RetainedMountSnapshot {
            active_nodes: self.active.len(),
            parked_nodes: self.parked.len(),
            restores: self.restores,
            evictions: self.evictions,
            hard_retirements: self.hard_retirements,
            eligible_parked_nodes: self.eligible_parked_count,
            eligible_lru_entries: self.eligible_lru.len(),
            strong_instances,
            strong_instance_high_water: self.strong_instance_high_water,
        }
    }

    fn mark_parked_eligible(&mut self, instance: &NodeInstanceKey) {
        let Some(parked) = self.parked.get(instance) else {
            return;
        };
        if parked.eligible_sequence.is_some() {
            return;
        }
        let sequence = self.allocate_park_sequence();
        if let Some(parked) = self.parked.get_mut(instance) {
            parked.eligible_sequence = Some(sequence);
        }
        self.eligible_lru.push_back((sequence, instance.clone()));
        self.eligible_parked_count = self.eligible_parked_count.saturating_add(1);
    }

    fn allocate_park_sequence(&mut self) -> u64 {
        if self.next_park_sequence == u64::MAX {
            self.resequence_eligible_lru();
        }
        let sequence = self.next_park_sequence;
        self.next_park_sequence = self.next_park_sequence.saturating_add(1);
        sequence
    }

    fn compact_eligible_lru_if_needed(&mut self) {
        let maximum_entries = self
            .eligible_parked_count
            .saturating_mul(2)
            .saturating_add(self.eligible_parked_capacity);
        if self.eligible_lru.len() > maximum_entries {
            self.resequence_eligible_lru();
        }
    }

    fn resequence_eligible_lru(&mut self) {
        let mut eligible = self
            .parked
            .iter()
            .filter_map(|(instance, parked)| {
                parked
                    .eligible_sequence
                    .map(|sequence| (sequence, instance.clone()))
            })
            .collect::<Vec<_>>();
        eligible.sort_by_key(|(sequence, _)| *sequence);
        self.eligible_lru.clear();
        for (index, (_, instance)) in eligible.into_iter().enumerate() {
            let sequence = u64::try_from(index)
                .unwrap_or(u64::MAX - 1)
                .saturating_add(1);
            if let Some(parked) = self.parked.get_mut(&instance) {
                parked.eligible_sequence = Some(sequence);
            }
            self.eligible_lru.push_back((sequence, instance));
        }
        self.next_park_sequence = u64::try_from(self.eligible_lru.len())
            .unwrap_or(u64::MAX - 1)
            .saturating_add(1);
    }

    fn track_instance(&mut self, instance: &NodeInstanceKey) {
        self.retained_instances
            .insert(instance.node(), instance.clone());
    }

    fn untrack_instance(&mut self, instance: &NodeInstanceKey) {
        if self.retained_instances.get(&instance.node()) == Some(instance) {
            self.retained_instances.remove(&instance.node());
        }
    }

    fn refresh_high_water(&mut self) {
        let strong_instances = self.active.len().saturating_add(self.parked.len());
        self.strong_instance_high_water = self.strong_instance_high_water.max(strong_instances);
    }

    #[cfg(test)]
    pub(super) fn active_reconcile_usage(&self) -> (u64, u64) {
        (self.active_insertions, self.active_detachments)
    }
}
