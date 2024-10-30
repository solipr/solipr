//! Tests on [Registry]

use std::io::Read;

use solipr_core::registry::{ContentHash, Registry};
use solipr_memory::registry::MemoryRegistry;
use solipr_persistent::registry::PersistentRegistry;
use tempfile::TempDir;

fn registry_checks(registry: impl Registry) {
    for _ in 0..1024 {
        read_a_non_written_value(&registry);
    }

    read_a_written_value(
        &registry,
        b"hello",
        "content:LPJNul-wow4m6DsqxbninhsWHlwfp0JecwQzYpOLmCQ",
    );

    read_a_written_value(
        &registry,
        b"world",
        "content:SG6kYiTRu0-2gPNPfJrZao8k7Ii-c-qOWmxlJg6cuKc",
    );
}

fn read_a_non_written_value(registry: &impl Registry) {
    let random_hash = ContentHash::new(rand::random());
    let read_content = registry.read(random_hash).unwrap();
    assert!(read_content.is_none(), "the content should not be found");
}

fn read_a_written_value(registry: &impl Registry, value: &[u8], expected_hash: &str) {
    let hash = registry.write(value).unwrap();
    assert_eq!(
        hash.to_string(),
        expected_hash,
        "the hash should be a sha-256 hash of the content"
    );
    let mut read_content = registry.read(hash).unwrap().unwrap();
    let mut buffer = Vec::new();
    read_content.read_to_end(&mut buffer).unwrap();
    assert_eq!(buffer, value, "the content should not change");
}

#[test]
fn memory_registry_checks() {
    registry_checks(MemoryRegistry::new());
}

#[test]
fn persistent_registry_checks() {
    let temp_dir = TempDir::new().unwrap();
    registry_checks(PersistentRegistry::new(temp_dir.path()));
    temp_dir.close().unwrap();
}
