use std::collections::{BTreeMap, VecDeque};

use jellyflow_core::NodeId;
use jellyflow_runtime::NodeInstanceKey;

pub(super) struct RetainedMountStore<T> {
    active: BTreeMap<NodeInstanceKey, T>,
    active_order: Vec<Option<NodeInstanceKey>>,
    active_order_slots: BTreeMap<NodeInstanceKey, usize>,
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
            active_order_slots: BTreeMap::new(),
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
        }
    }

    pub(super) fn retained_instance(&self, node: NodeId) -> Option<&NodeInstanceKey> {
        self.retained_instances.get(&node)
    }

    pub(super) fn get(&self, instance: &NodeInstanceKey) -> Option<&T> {
        self.active
            .get(instance)
            .or_else(|| self.parked.get(instance).map(|parked| &parked.value))
    }

    pub(super) fn get_mut(&mut self, instance: &NodeInstanceKey) -> Option<&mut T> {
        if let Some(active) = self.active.get_mut(instance) {
            return Some(active);
        }
        self.parked
            .get_mut(instance)
            .map(|parked| &mut parked.value)
    }

    pub(super) fn active_values(&self) -> impl Iterator<Item = &T> {
        self.active.values()
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = (&NodeInstanceKey, &T)> {
        self.active.iter().chain(
            self.parked
                .iter()
                .map(|(instance, parked)| (instance, &parked.value)),
        )
    }

    pub(super) fn active_in_order(&self) -> impl Iterator<Item = (&NodeInstanceKey, &T)> {
        self.active_order
            .iter()
            .flatten()
            .filter_map(|instance| self.active.get(instance).map(|value| (instance, value)))
    }

    pub(super) fn parked_instances(&self) -> impl Iterator<Item = &NodeInstanceKey> {
        self.parked.keys()
    }

    pub(super) fn take_active(&mut self) -> BTreeMap<NodeInstanceKey, T> {
        self.active_order.clear();
        self.active_order_slots.clear();
        std::mem::take(&mut self.active)
    }

    pub(super) fn restore(&mut self, instance: &NodeInstanceKey) -> Option<T> {
        let mut parked = self.parked.remove(instance)?;
        if parked.eligible_sequence.take().is_some() {
            self.eligible_parked_count = self.eligible_parked_count.saturating_sub(1);
        }
        self.restores = self.restores.saturating_add(1);
        Some(parked.value)
    }

    pub(super) fn insert_active(
        &mut self,
        instance: NodeInstanceKey,
        value: T,
    ) -> Option<EvictedMount<T>> {
        self.remove_active_order(&instance);
        let replaced = self.active.insert(instance.clone(), value);
        let slot = self.active_order.len();
        self.active_order.push(Some(instance.clone()));
        self.active_order_slots.insert(instance.clone(), slot);
        self.track_instance(&instance);
        replaced.map(|value| {
            self.evictions = self.evictions.saturating_add(1);
            EvictedMount { instance, value }
        })
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
        self.refresh_high_water();
    }

    pub(super) fn retire(&mut self, instance: &NodeInstanceKey) -> Option<RetiredMount<T>> {
        let value = if let Some(value) = self.active.remove(instance) {
            self.remove_active_order(instance);
            Some(value)
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
        let active = std::mem::take(&mut self.active).into_values();
        let parked = std::mem::take(&mut self.parked)
            .into_values()
            .map(|parked| parked.value);
        let retired = active.chain(parked).collect::<Vec<_>>();
        self.retained_instances.clear();
        self.active_order.clear();
        self.active_order_slots.clear();
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

    fn remove_active_order(&mut self, instance: &NodeInstanceKey) {
        let Some(slot) = self.active_order_slots.remove(instance) else {
            return;
        };
        if let Some(entry) = self.active_order.get_mut(slot) {
            *entry = None;
        }
    }

    fn refresh_high_water(&mut self) {
        let strong_instances = self.active.len().saturating_add(self.parked.len());
        self.strong_instance_high_water = self.strong_instance_high_water.max(strong_instances);
    }
}
