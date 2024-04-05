use std::collections::VecDeque;
use std::fmt::{Debug, Formatter};

use twilight_model::id::marker::MessageMarker;
use twilight_model::id::Id;

type MessageId = Id<MessageMarker>;

/// A cache mapping a source message ID to its reply, if the bot sent one.
///
/// This cache is backed by a ring buffer of 128 ID pairs, and as it reaches
/// capacity, the eldest element is evicted. Once an entry has been written,
/// it cannot be removed until it gets popped out by a newer one.
/// 
/// A `ReplyCache` instance can be acquired via `[Default::default()]`.
pub struct ReplyCache(VecDeque<(MessageId, MessageId)>);

impl ReplyCache {
    pub fn with_capacity(capacity: usize) -> Self {
        Self(VecDeque::with_capacity(capacity))
    }

    #[inline]
    fn search(&self, source: MessageId) -> Result<usize, usize> {
        self.0
            .binary_search_by_key(&source, |&(source, _reply)| source)
    }

    /// Gets the ID of the reply message to the provided source message via
    /// binary search. 
    pub fn get_reply(&self, source: MessageId) -> Option<MessageId> {
        self.search(source).ok().map(|idx| self.0[idx].1)
    }

    /// Inserts an ID pair of a source message to the reply message the bot sent.
    /// If the cache is at capacity, the eldest entry is evicted. If there is
    /// already a mapping from the source ID, the new entry is not applied.
    pub fn insert(&mut self, source: MessageId, reply: MessageId) {
        // If at capacity, remove the eldest entry
        if self.0.len() == self.0.capacity() {
            self.0.pop_front();
        }

        // Fast path: messages almost always come in order, so we check to
        // see if we can just insert to the back
        if let Some(&(head_source, _head_reply)) = self.0.back() {
            if head_source <= source {
                self.0.push_back((source, reply));
                return;
            }
        }

        if let Err(target_idx) = self.search(source) {
            self.0.insert(target_idx, (source, reply));
        } else {
            tracing::warn!(
                "Tried to double-map {source}: {old:?} -> {new}",
                old = self.get_reply(source),
                new = reply,
            );
        }
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.0.len()
    }
}

impl Debug for ReplyCache {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FixCache")
            .field("size", &self.0.len())
            .field("state", &self.0)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::ReplyCache;

    #[test]
    fn cache() {
        /// Util for getting a snowflake from a literal
        macro_rules! id {
            (0) => {
                compile_error!("Snowflakes cannot be 0")
            };

            ($id:literal) => {
                // SAFETY: compiler ensures that 0 is never passed to this branch
                unsafe { super::MessageId::new_unchecked($id) }
            };
        }

        let mut cache = ReplyCache::with_capacity(3);

        // Out of order insertion (2 then 1)
        cache.insert(id!(2), id!(12));
        assert_eq!(cache.get_reply(id!(2)), Some(id!(12)));
        assert_eq!(cache.len(), 1);

        cache.insert(id!(1), id!(11));
        assert_eq!(cache.get_reply(id!(1)), Some(id!(11)));
        assert_eq!(cache.len(), 2);

        cache.insert(id!(3), id!(13));
        assert_eq!(cache.get_reply(id!(3)), Some(id!(13)));
        assert_eq!(cache.len(), 3);

        // At capacity -- test eviction
        cache.insert(id!(4), id!(14));
        assert_eq!(cache.get_reply(id!(4)), Some(id!(14)));

        assert_eq!(cache.len(), 3, "Cache did not evict an entry at capacity");
        assert_eq!(cache.get_reply(id!(1)), None, "Cache did not evict eldest");
    }
}
