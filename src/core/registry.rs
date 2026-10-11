use std::collections::HashMap;
use std::sync::OnceLock;

use super::MusicSource;

pub type SourceFactory = fn() -> Box<dyn MusicSource>;

fn registry() -> &'static HashMap<&'static str, SourceFactory> {
    static REGISTRY: OnceLock<HashMap<&'static str, SourceFactory>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut m = HashMap::new();
        m.insert(
            "fivesing",
            crate::sources::fivesing::create as SourceFactory,
        );
        m.insert("kugou", crate::sources::kugou::create as SourceFactory);
        m.insert("kuwo", crate::sources::kuwo::create as SourceFactory);
        m.insert("migu", crate::sources::migu::create as SourceFactory);
        m.insert("netease", crate::sources::netease::create as SourceFactory);
        m.insert("qq", crate::sources::qq::create as SourceFactory);
        m.insert("meting", crate::sources::meting::create as SourceFactory);
        m
    })
}

pub fn build(name: &str) -> Option<Box<dyn MusicSource>> {
    registry().get(name).map(|f| f())
}

pub fn available_sources() -> Vec<&'static str> {
    registry().keys().copied().collect()
}
