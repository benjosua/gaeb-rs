use std::collections::{HashMap, VecDeque};

pub trait CacheBackend {
    fn get(&mut self, key: &str) -> Option<String>;
    fn put(&mut self, key: String, value: String);
    fn delete(&mut self, key: &str) -> bool;
    fn clear(&mut self);
    fn keys(&self) -> Vec<String>;
    fn close(&mut self) {}
}

#[derive(Clone, Debug)]
pub struct InMemoryCache {
    maxsize: usize,
    values: HashMap<String, String>,
    order: VecDeque<String>,
}

impl Default for InMemoryCache {
    fn default() -> Self {
        Self::new(10_000)
    }
}

impl InMemoryCache {
    pub fn new(maxsize: usize) -> Self {
        Self {
            maxsize,
            values: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    fn touch(&mut self, key: &str) {
        self.order.retain(|entry| entry != key);
        self.order.push_back(key.to_owned());
    }
}

impl CacheBackend for InMemoryCache {
    fn get(&mut self, key: &str) -> Option<String> {
        let value = self.values.get(key).cloned();
        if value.is_some() {
            self.touch(key);
        }
        value
    }

    fn put(&mut self, key: String, value: String) {
        if self.maxsize == 0 {
            return;
        }
        self.values.insert(key.clone(), value);
        self.touch(&key);
        while self.values.len() > self.maxsize {
            if let Some(oldest) = self.order.pop_front() {
                self.values.remove(&oldest);
            } else {
                break;
            }
        }
    }

    fn delete(&mut self, key: &str) -> bool {
        self.order.retain(|entry| entry != key);
        self.values.remove(key).is_some()
    }

    fn clear(&mut self) {
        self.values.clear();
        self.order.clear();
    }

    fn keys(&self) -> Vec<String> {
        self.order.iter().cloned().collect()
    }
}
