//! Tests on the registry of a [RepositoryManager].

use std::io::Read;

use solipr_core::repository::{ContentHash, RepositoryManager};

use super::manager_check;

#[test]
fn read_a_non_written_content() {
    manager_check(|manager| {
        let random_hash = ContentHash::new(rand::random());
        let read_content = manager.read_content(random_hash).unwrap();
        assert!(read_content.is_none(), "the content should not be found");
    });
}

#[test]
fn read_a_written_content() {
    manager_check(|manager| {
        let hash = manager.write_content(&b"hello"[..]).unwrap();
        assert_eq!(
            hash.to_string(),
            "content:LPJNul-wow4m6DsqxbninhsWHlwfp0JecwQzYpOLmCQ",
            "the hash should be a sha-256 hash of the content"
        );
        let mut read_content = manager.read_content(hash).unwrap().unwrap();
        let mut buffer = Vec::new();
        read_content.read_to_end(&mut buffer).unwrap();
        assert_eq!(buffer, b"hello", "the content should not change");
    });
}
