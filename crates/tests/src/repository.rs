//! Tests on [Repository].

use std::io::Read;

use solipr_core::repository::{ContentHash, Repository, RepositoryId, RepositoryManager};
use solipr_persistent::repository::{PersistentRepository, PersistentRepositoryManager};
use tempfile::TempDir;

fn manager_check(test: fn(PersistentRepositoryManager)) {
    let temp_dir = TempDir::new().unwrap();
    let manager = PersistentRepositoryManager::create(temp_dir.path()).unwrap();
    test(manager);
    temp_dir.close().unwrap();
}

#[expect(unused, reason = "will be usefull in the future")]
fn read_repository_check(test: fn(PersistentRepository)) {
    let temp_dir = TempDir::new().unwrap();
    let manager = PersistentRepositoryManager::create(temp_dir.path()).unwrap();
    let repository = manager.open_read(RepositoryId::create_new()).unwrap();
    test(repository);
    temp_dir.close().unwrap();
}

#[expect(unused, reason = "will be usefull in the future")]
fn write_repository_check(test: fn(&mut PersistentRepository)) {
    let temp_dir = TempDir::new().unwrap();
    let manager = PersistentRepositoryManager::create(temp_dir.path()).unwrap();
    let mut repository = manager.open_write(RepositoryId::create_new()).unwrap();
    test(&mut repository);
    repository.commit().unwrap();
    temp_dir.close().unwrap();
}

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
