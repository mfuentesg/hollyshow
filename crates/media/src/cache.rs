use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use tokio::time::Instant;

/// Small in-memory cache with a time to live and a bounded size.
pub struct TtlCache<V: Clone> {
    ttl: Duration,
    max_entries: usize,
    map: Mutex<HashMap<String, (Instant, V)>>,
}

impl<V: Clone> TtlCache<V> {
    pub fn new(ttl: Duration, max_entries: usize) -> Self {
        Self {
            ttl,
            max_entries: max_entries.max(1),
            map: Mutex::new(HashMap::new()),
        }
    }

    pub fn get(&self, key: &str) -> Option<V> {
        let mut map = self.map.lock().unwrap_or_else(|e| e.into_inner());
        match map.get(key) {
            Some((at, v)) if at.elapsed() < self.ttl => Some(v.clone()),
            Some(_) => {
                map.remove(key);
                None
            }
            None => None,
        }
    }

    pub fn put(&self, key: String, value: V) {
        let mut map = self.map.lock().unwrap_or_else(|e| e.into_inner());
        if map.len() >= self.max_entries && !map.contains_key(&key) {
            let ttl = self.ttl;
            map.retain(|_, (at, _)| at.elapsed() < ttl);
            if map.len() >= self.max_entries {
                if let Some(oldest) = map
                    .iter()
                    .min_by_key(|(_, (at, _))| *at)
                    .map(|(k, _)| k.clone())
                {
                    map.remove(&oldest);
                }
            }
        }
        map.insert(key, (Instant::now(), value));
    }

    pub fn clear(&self) {
        self.map.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }

    pub fn len(&self) -> usize {
        self.map.lock().unwrap_or_else(|e| e.into_inner()).len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn entries_expire_after_ttl() {
        let c = TtlCache::new(Duration::from_secs(60), 8);
        c.put("a".into(), 1);
        assert_eq!(c.get("a"), Some(1));
        tokio::time::advance(Duration::from_secs(59)).await;
        assert_eq!(c.get("a"), Some(1));
        tokio::time::advance(Duration::from_secs(2)).await;
        assert_eq!(c.get("a"), None);
        assert!(c.is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn evicts_oldest_when_full() {
        let c = TtlCache::new(Duration::from_secs(600), 2);
        c.put("a".into(), 1);
        tokio::time::advance(Duration::from_secs(1)).await;
        c.put("b".into(), 2);
        tokio::time::advance(Duration::from_secs(1)).await;
        c.put("c".into(), 3);
        assert_eq!(c.len(), 2);
        assert_eq!(c.get("a"), None);
        assert_eq!(c.get("c"), Some(3));
    }
}
