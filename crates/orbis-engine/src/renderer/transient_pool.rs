//! Generic allocator for resources whose lifetimes are bounded by render-graph passes.
//!
//! The pool owns resources across frames and aliases a slot within a frame only
//! when the previous resource's last use precedes the next resource's first use.
//! The caller is responsible for deriving accurate lifetimes from the compiled
//! pass order and for ensuring encoded work is submitted in that order.

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TransientRequest<D> {
    pub(crate) descriptor: D,
    pub(crate) first_use: usize,
    pub(crate) last_use: usize,
}

impl<D> TransientRequest<D> {
    pub(crate) fn new(descriptor: D, first_use: usize, last_use: usize) -> Self {
        assert!(first_use <= last_use, "transient resource lifetime is inverted");
        Self { descriptor, first_use, last_use }
    }
}

struct Entry<D, R> {
    descriptor: D,
    resource: R,
}

/// Retains allocated resources and reuses compatible slots when their pass
/// lifetimes do not overlap. D should contain every property that affects
/// resource compatibility (for example extent, format, mip count and usage).
pub(crate) struct TransientResourcePool<D, R> {
    entries: Vec<Entry<D, R>>,
}

impl<D, R> Default for TransientResourcePool<D, R> {
    fn default() -> Self {
        Self { entries: Vec::new() }
    }
}

impl<D: Eq + Clone, R> TransientResourcePool<D, R> {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Assigns each request to a pool slot and creates resources only when no
    /// compatible slot is available. Returned indices correspond to the input
    /// request order and can be used with get/get_mut.
    ///
    /// Slots are considered available at the start of each allocation call,
    /// representing a new frame. Within a frame, a slot can be reused only
    /// when previous_last_use < next_first_use.
    pub(crate) fn allocate_frame(
        &mut self,
        requests: &[TransientRequest<D>],
        mut create: impl FnMut(&D) -> R,
    ) -> Vec<usize> {
        let mut order: Vec<usize> = (0..requests.len()).collect();
        order.sort_by_key(|&index| (requests[index].first_use, requests[index].last_use));

        let mut assignments = vec![usize::MAX; requests.len()];
        let mut slot_last_use: Vec<Option<usize>> = vec![None; self.entries.len()];

        for request_index in order {
            let request = &requests[request_index];
            assert!(
                request.first_use <= request.last_use,
                "transient resource lifetime is inverted"
            );

            let reusable = self.entries.iter().enumerate().find_map(|(slot, entry)| {
                let lifetime_ended = slot_last_use
                    .get(slot)
                    .copied()
                    .flatten()
                    .map_or(true, |last_use| last_use < request.first_use);
                (lifetime_ended && entry.descriptor == request.descriptor).then_some(slot)
            });

            let slot = match reusable {
                Some(slot) => slot,
                None => {
                    let slot = self.entries.len();
                    self.entries.push(Entry {
                        descriptor: request.descriptor.clone(),
                        resource: create(&request.descriptor),
                    });
                    slot_last_use.push(None);
                    slot
                }
            };

            slot_last_use[slot] = Some(request.last_use);
            assignments[request_index] = slot;
        }

        assignments
    }

    pub(crate) fn get(&self, slot: usize) -> Option<&R> {
        self.entries.get(slot).map(|entry| &entry.resource)
    }

    pub(crate) fn get_mut(&mut self, slot: usize) -> Option<&mut R> {
        self.entries.get_mut(slot).map(|entry| &mut entry.resource)
    }

    /// Number of retained resource allocations, including currently unused slots.
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reuses_compatible_slots_for_non_overlapping_lifetimes() {
        let requests = [
            TransientRequest::new("rgba8", 0, 1),
            TransientRequest::new("rgba8", 2, 4),
            TransientRequest::new("rgba8", 4, 5),
        ];
        let mut created = 0;
        let mut pool = TransientResourcePool::new();

        let slots = pool.allocate_frame(&requests, |_| {
            created += 1;
            created
        });

        assert_eq!(slots, vec![0, 0, 1]);
        assert_eq!(created, 2);
        assert_eq!(pool.len(), 2);
    }

    #[test]
    fn overlapping_lifetimes_get_distinct_slots() {
        let requests = [
            TransientRequest::new("rgba8", 0, 2),
            TransientRequest::new("rgba8", 2, 3),
        ];
        let mut pool = TransientResourcePool::new();
        let slots = pool.allocate_frame(&requests, |_| ());
        assert_ne!(slots[0], slots[1]);
    }

    #[test]
    fn incompatible_descriptors_never_alias() {
        let requests = [
            TransientRequest::new("rgba8", 0, 0),
            TransientRequest::new("r32uint", 1, 1),
        ];
        let mut pool = TransientResourcePool::new();
        let slots = pool.allocate_frame(&requests, |_| ());
        assert_ne!(slots[0], slots[1]);
        assert_eq!(pool.len(), 2);
    }

    #[test]
    fn retained_slots_are_reused_on_later_frames() {
        let mut pool = TransientResourcePool::new();
        let requests = [TransientRequest::new(32_u32, 0, 1)];
        let first = pool.allocate_frame(&requests, |size| *size);
        let second = pool.allocate_frame(&requests, |_| panic!("should reuse slot"));

        assert_eq!(first, second);
        assert_eq!(pool.get(first[0]), Some(&32));
        assert_eq!(pool.len(), 1);
    }

    #[test]
    fn assignments_follow_request_order_not_lifetime_sort_order() {
        let requests = [
            TransientRequest::new("late", 4, 5),
            TransientRequest::new("early", 0, 1),
        ];
        let mut pool = TransientResourcePool::new();
        let slots = pool.allocate_frame(&requests, str::to_owned);

        assert_eq!(pool.get(slots[0]).map(String::as_str), Some("late"));
        assert_eq!(pool.get(slots[1]).map(String::as_str), Some("early"));
    }

    #[test]
    fn mutable_access_updates_retained_resource() {
        let mut pool = TransientResourcePool::new();
        let slots = pool.allocate_frame(&[TransientRequest::new(1, 0, 0)], |_| 5);
        *pool.get_mut(slots[0]).unwrap() = 7;
        assert_eq!(pool.get(slots[0]), Some(&7));
    }

    #[test]
    #[should_panic(expected = "transient resource lifetime is inverted")]
    fn rejects_inverted_lifetime() {
        let _ = TransientRequest::new("rgba8", 3, 2);
    }
}
