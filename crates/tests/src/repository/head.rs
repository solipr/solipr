//! Tests on the heads in a [Repository].

use std::collections::HashSet;

use solipr_core::change::{Change, ChangeContent, FileId, LineId};
use solipr_core::repository::Repository;
use solipr_stack::StackVec;

use super::{read_repository_check, write_repository_check};

#[test]
fn empty_repository_should_not_contain_lines() {
    read_repository_check(|repository| {
        let file_id = FileId::unique();
        let line_id = LineId::unique();

        assert_eq!(
            repository.line_existence(file_id, line_id).unwrap(),
            Some(false)
        );
        assert_eq!(repository.existing_lines(file_id).unwrap(), HashSet::new());
    })
}

#[test]
fn head_line_existence_checks() {
    write_repository_check(|repository| {
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
    });
}
