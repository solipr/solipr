//! Tests on [Repository].

use solipr_core::repository::{Repository, RepositoryId, RepositoryManager};
use solipr_persistent::repository::{PersistentRepository, PersistentRepositoryManager};
use tempfile::TempDir;

mod registry;

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
