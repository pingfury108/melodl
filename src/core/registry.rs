use std::collections::HashMap;
use std::sync::OnceLock;

use super::MusicSource;

pub type SourceFactory = fn() -> Box<dyn MusicSource>;

fn registry() -> &'static HashMap<&'static str, SourceFactory> {
    static REGISTRY: OnceLock<HashMap<&'static str, SourceFactory>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut m = HashMap::new();
        m.insert("fivesing", crate::sources::fivesing::create as SourceFactory);
        m
    })
}

pub fn build(name: &str) -> Option<Box<dyn MusicSource>> {
    registry().get(name).map(|f| f())
}

pub fn available_sources() -> Vec<&'static str> {
    registry().keys().copied().collect()
}
