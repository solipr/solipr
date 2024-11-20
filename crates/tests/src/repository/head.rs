//! Tests for the [HeadExt] trait extention.

use std::collections::HashSet;

use solipr_core::change::{Change, ChangeContent, FileId, LineId};
use solipr_core::registry::{self, Registry};
use solipr_core::repository::head::HeadExt;
use solipr_core::repository::{Repository, RepositoryId, RepositoryManager};
use solipr_memory::registry::MemoryRegistry;
use solipr_persistent::repository::PersistentRepositoryManager;
use solipr_stack::StackVec;
use tempfile::TempDir;

pub fn head_line_existence_checks<'manager>(
    repository: &mut impl Repository<'manager>,
    registry: &impl Registry,
) {
    let file_id = FileId::unique();
    let line_id = LineId::unique();

    assert_eq!(
        repository.line_existence(file_id, line_id).unwrap(),
        Some(false),
    );
    assert_eq!(repository.existing_lines(file_id).unwrap(), HashSet::new());

    let first_change = Change {
        replace: StackVec::new(),
        content: ChangeContent::LineExistence {
            file_id,
            line_id,
            existence: true,
        },
    };
    repository.apply(first_change).unwrap();
    assert_eq!(
        repository.line_existence(file_id, line_id).unwrap(),
        Some(true)
    );
    assert_eq!(
        repository.existing_lines(file_id).unwrap(),
        HashSet::from_iter([line_id])
    );

    let change = Change {
        replace: StackVec::new(),
        content: ChangeContent::LineExistence {
            file_id,
            line_id,
            existence: false,
        },
    };
    repository.apply(change).unwrap();
    assert_eq!(repository.line_existence(file_id, line_id).unwrap(), None,);
    assert_eq!(
        repository.existing_lines(file_id).unwrap(),
        HashSet::from_iter([line_id])
    );

    repository.unapply(change.calculate_hash()).unwrap();
    assert_eq!(
        repository.line_existence(file_id, line_id).unwrap(),
        Some(true),
    );
    assert_eq!(
        repository.existing_lines(file_id).unwrap(),
        HashSet::from_iter([line_id])
    );

    let mut replace = StackVec::new();
    replace.push(first_change.calculate_hash());
    let replace_change = Change {
        replace,
        content: ChangeContent::LineExistence {
            file_id,
            line_id,
            existence: false,
        },
    };
    repository.apply(replace_change).unwrap();
    assert_eq!(
        repository.line_existence(file_id, line_id).unwrap(),
        Some(false),
    );
    assert_eq!(repository.existing_lines(file_id).unwrap(), HashSet::new());

    repository.unapply(first_change.calculate_hash()).unwrap();
    assert_eq!(
        repository.line_existence(file_id, line_id).unwrap(),
        Some(false),
    );
    assert_eq!(repository.existing_lines(file_id).unwrap(), HashSet::new());
}

#[test]
fn persistent_head_line_existence_checks() {
    let temp_dir = TempDir::new().unwrap();
    let manager = PersistentRepositoryManager::create(temp_dir.path()).unwrap();
    let registry = MemoryRegistry::new();
    for _ in 0..1024 {
        let mut repository = manager.open_write(RepositoryId::create_new()).unwrap();
        head_line_existence_checks(&mut repository, &registry);
        repository.commit().unwrap();
    }
    temp_dir.close().unwrap();
}
