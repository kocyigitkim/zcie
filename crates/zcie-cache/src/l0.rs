//! L0 in-process cache of query responses and result pages.

use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use lru::LruCache;
use zcie_types::{QueryResponse, ResultItem};

/// Lifetime of stored result pages for cursors.
pub const PAGE_TTL: Duration = Duration::from_secs(300);
/// Maximum number of stored result pages.
const MAX_PAGES: usize = 1000;

#[derive(Debug, Clone)]
struct Entry {
    response: QueryResponse,
    workspace_id: String,
    expires: Option<Instant>,
}

/// Results that a token budget left out, for a later cursor call.
#[derive(Debug, Clone)]
pub struct StoredPage {
    /// The first response without results.
    pub base: QueryResponse,
    /// The remaining results in rank order.
    pub remaining: Vec<ResultItem>,
    /// The workspace of the query.
    pub workspace_id: String,
    /// The principal of the query.
    pub principal_id: String,
    expires: Instant,
}

/// The L0 cache.
pub struct L0Cache {
    entries: Mutex<LruCache<String, Entry>>,
    pages: Mutex<HashMap<String, StoredPage>>,
}

impl L0Cache {
    /// Makes a cache with a capacity in entries.
    pub fn new(capacity: usize) -> Self {
        let capacity = NonZeroUsize::new(capacity.max(1)).unwrap_or(NonZeroUsize::MIN);
        Self { entries: Mutex::new(LruCache::new(capacity)), pages: Mutex::new(HashMap::new()) }
    }

    fn entries(&self) -> std::sync::MutexGuard<'_, LruCache<String, Entry>> {
        self.entries.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Returns a cached response. Expired entries are removed.
    pub fn get(&self, key: &str) -> Option<QueryResponse> {
        let mut entries = self.entries();
        let expired = entries.peek(key).and_then(|e| e.expires).is_some_and(|t| t <= Instant::now());
        if expired {
            entries.pop(key);
            return None;
        }
        entries.get(key).map(|e| e.response.clone())
    }

    /// Stores a response. `ttl` is used for negative results only.
    pub fn put(&self, key: String, workspace_id: &str, response: QueryResponse, ttl: Option<Duration>) {
        let entry = Entry { response, workspace_id: workspace_id.to_string(), expires: ttl.map(|t| Instant::now() + t) };
        self.entries().put(key, entry);
    }

    /// Removes all entries of a workspace.
    pub fn invalidate_workspace(&self, workspace_id: &str) {
        let mut entries = self.entries();
        let keys: Vec<String> = entries.iter().filter(|(_, e)| e.workspace_id == workspace_id).map(|(k, _)| k.clone()).collect();
        for key in keys {
            entries.pop(&key);
        }
        self.pages().retain(|_, page| page.workspace_id != workspace_id);
    }

    /// Removes all entries that contain one of the refs.
    pub fn invalidate_refs(&self, refs: &[String]) {
        if refs.is_empty() {
            return;
        }
        let mut entries = self.entries();
        let keys: Vec<String> = entries
            .iter()
            .filter(|(_, e)| e.response.results.iter().any(|r| refs.contains(&r.entity_ref)))
            .map(|(k, _)| k.clone())
            .collect();
        for key in keys {
            entries.pop(&key);
        }
    }

    /// Removes all entries.
    pub fn clear(&self) {
        self.entries().clear();
        self.pages().clear();
    }

    /// Returns the number of entries.
    pub fn len(&self) -> usize {
        self.entries().len()
    }

    /// Returns true if the cache has no entries.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn pages(&self) -> std::sync::MutexGuard<'_, HashMap<String, StoredPage>> {
        self.pages.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Stores remaining results for a cursor.
    pub fn put_page(&self, query_id: &str, base: QueryResponse, remaining: Vec<ResultItem>, workspace_id: &str, principal_id: &str) {
        let mut pages = self.pages();
        let now = Instant::now();
        pages.retain(|_, page| page.expires > now);
        if pages.len() >= MAX_PAGES {
            return;
        }
        let page = StoredPage {
            base,
            remaining,
            workspace_id: workspace_id.to_string(),
            principal_id: principal_id.to_string(),
            expires: now + PAGE_TTL,
        };
        pages.insert(query_id.to_string(), page);
    }

    /// Returns stored remaining results for a cursor.
    pub fn get_page(&self, query_id: &str) -> Option<StoredPage> {
        let pages = self.pages();
        pages.get(query_id).filter(|p| p.expires > Instant::now()).cloned()
    }
}
