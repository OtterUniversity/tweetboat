use std::collections::VecDeque;
use std::fmt::{Debug, Formatter};

use twilight_model::id::marker::MessageMarker;
use twilight_model::id::Id;

type MessageId = Id<MessageMarker>;

/// A cache mapping a *source* message ID to its *reply*, if the bot sent one.
///
/// This cache is backed by a ring buffer of a fixed number of ID pairs, and as
/// it reaches capacity, the eldest element is evicted. Once an entry has been
/// written, it cannot be removed until it gets popped out by a newer one.
///
/// # Pending Entries
/// As soon as a message that needs processing is received, it is entered into
/// the cache in a [pending] state. This allows the bot to know that a message
/// is relevant in the edit event, and to suppress embeds if the unfurler gets
/// back to us before our reply has been sent out.
///
/// Upon entering a pending state, an [InsertToken] is provided, which can be
/// used once the reply has gone through to finish off the entry. Entries are
/// allowed to stay in the pending state forever if the reply fails -- they will
/// just eventually be evicted from cache once they end up at its tail. When a
/// message is deleted, it may also be returned to a pending state via the
/// [take_entry] method.
///
/// [pending]: CacheEntry::Pending
/// [take_entry]: ReplyCache::take_entry
pub struct ReplyCache(VecDeque<(MessageId, CacheEntry)>);

impl ReplyCache {
    pub fn with_capacity(capacity: usize) -> Self {
        Self(VecDeque::with_capacity(capacity))
    }

    #[inline]
    fn search(&self, source: MessageId) -> Result<usize, usize> {
        self.0
            .binary_search_by_key(&source, |&(source, _reply)| source)
    }

    /// Files a [CacheEntry::Pending] value for the given source message. If the
    /// entry is free, an [InsertToken] is returned. If there is another value
    /// in the source message's slot, `[None]` is returned.
    pub fn file_pending(&mut self, source: MessageId) -> Option<InsertToken> {
        if self.0.len() == self.0.capacity() {
            self.0.pop_front();
        }

        // Fast path: messages generally come in order, so we check the tail to
        // see if we can just append
        if let Some(&(back_source, _entry)) = self.0.back() {
            if back_source <= source {
                let idx = self.0.len(); // The push increments len by 1
                self.0.push_back((source, CacheEntry::Pending));
                return Some(InsertToken { source, idx });
            }
        }

        // Err means we have an open slot to insert into
        if let Err(idx) = self.search(source) {
            self.0.insert(idx, (source, CacheEntry::Pending));
            Some(InsertToken { source, idx })
        } else {
            None
        }
    }

    /// Completes an insertion into the cache after a reply has been sent.
    pub fn insert(&mut self, token: InsertToken, reply: MessageId) {
        // The token stores the index it was at when it was made, check if it's
        // still there
        if let Some(&token_match) = self.0.get(token.idx) {
            if token_match.0 == token.source {
                self.0[token.idx] = (token.source, CacheEntry::Filled(reply));
                return;
            }
        }

        // Fallthrough: another entry has been added since we got the token
        let idx = self.search(token.source);
        let idx = idx.map_or_else(|ok| ok, |err| err);
        self.0[idx] = (token.source, CacheEntry::Filled(reply));
    }

    /// Gets an entry from the cache from the provided source message ID by
    /// binary searching the backing vector.
    pub fn get_entry(&self, source: MessageId) -> Option<CacheEntry> {
        self.search(source).ok().map(|idx| self.0[idx].1)
    }

    /// Gets an entry from the cache, invalidating it after it has been returned.
    pub fn take_entry(&mut self, source: MessageId) -> Option<CacheEntry> {
        if let Ok(idx) = self.search(source) {
            let (_, entry) = self.0[idx];
            self.0[idx] = (source, CacheEntry::Pending);
            Some(entry)
        } else {
            None
        }
    }

    /// Test fixture used to check that cache eviction is working.
    #[cfg(test)]
    fn len(&self) -> usize {
        self.0.len()
    }
}

impl Debug for ReplyCache {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReplyCache")
            .field("size", &self.0.len())
            .field("state", &self.0)
            .finish()
    }
}

/// An entry in the [ReplyCache].
// Thanks to niches this enum does not change the size of the cache at all!
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CacheEntry {
    /// An incomplete entry that has not received a reply. This state is also
    /// used for deleted messages.
    Pending,
    /// A filled entry pointing to the bot's reply message ID.
    Filled(MessageId),

}

/// A token indicating that a message has been received and needs a reply but
/// the reply has not yet been sent.
///
/// This class contains the source message ID and the speculative index of
/// where the entry will end up.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct InsertToken {
    source: MessageId,
    idx: usize,
}

#[cfg(test)]
mod tests {
    use super::{CacheEntry, ReplyCache};

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

        let mut cache = ReplyCache::with_capacity(4);

        // Out of order insertion (2 then 1)
        let token = cache.file_pending(id!(2)).unwrap();
        assert_eq!(cache.get_entry(id!(2)), Some(CacheEntry::Pending));

        cache.insert(token, id!(12));
        assert_eq!(cache.get_entry(id!(2)), Some(CacheEntry::Filled(id!(12))));

        let token = cache.file_pending(id!(1)).unwrap();
        assert_eq!(cache.get_entry(id!(1)), Some(CacheEntry::Pending));

        cache.insert(token, id!(11));
        assert_eq!(cache.get_entry(id!(1)), Some(CacheEntry::Filled(id!(11))));

        // Quick succession: 2 messages come in before the 1st can be replied
        // to, and they are out of order
        let token_4 = cache.file_pending(id!(4)).unwrap(); // idx: 2
        let token_3 = cache.file_pending(id!(3)).unwrap(); // idx: 3

        // Insert them back in order
        cache.insert(token_3, id!(13));
        cache.insert(token_4, id!(14));

        assert_eq!(cache.get_entry(id!(3)), Some(CacheEntry::Filled(id!(13))));
        assert_eq!(cache.get_entry(id!(4)), Some(CacheEntry::Filled(id!(14))));

        assert_eq!(cache.len(), 4);

        let token = cache.file_pending(id!(5)).unwrap();
        cache.insert(token, id!(15));

        // Hit capacity, evicted
        assert_eq!(cache.len(), 4);
        assert_eq!(cache.get_entry(id!(5)), Some(CacheEntry::Filled(id!(15))));
    }
}
