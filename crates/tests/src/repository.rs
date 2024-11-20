//! Tests on [Repository].

use solipr_core::registry::Registry;
use solipr_core::repository::{Repository, RepositoryId, RepositoryManager};
use solipr_memory::registry::MemoryRegistry;
use solipr_persistent::registry::PersistentRegistry;
use solipr_persistent::repository::PersistentRepositoryManager;
use tempfile::TempDir;

mod file;
mod head;

fn repository_manager_checks(manager: impl RepositoryManager, registry: impl Registry) {
    for _ in 0..1024 {
        let repository_id = RepositoryId::create_new();
        let mut repository = manager.open_write(repository_id).unwrap();
        repository_checks(&mut repository, &registry);
        repository.commit().unwrap();
    }
}

fn repository_checks<'manager>(
    repository: &mut impl Repository<'manager>,
    registry: &impl Registry,
) {
    file::one_file_checks(repository, registry);
}

#[test]
fn persistent_repository_memory_registry_checks() {
    let temp_dir = TempDir::new().unwrap();
    repository_manager_checks(
        PersistentRepositoryManager::create(temp_dir.path()).unwrap(),
        MemoryRegistry::new(),
    );
    temp_dir.close().unwrap();
}

#[test]
fn persistent_repository_persistent_registry_checks() {
    let temp_dir = TempDir::new().unwrap();
    repository_manager_checks(
        PersistentRepositoryManager::create(temp_dir.path().join("manager")).unwrap(),
        PersistentRegistry::new(temp_dir.path().join("registry")),
    );
    temp_dir.close().unwrap();
}
