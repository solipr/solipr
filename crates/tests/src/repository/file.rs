//! Tests for the [FileExt] trait extention.

use std::collections::HashSet;

use solipr_core::change::FileId;
use solipr_core::registry::Registry;
use solipr_core::repository::Repository;
use solipr_core::repository::file::{File, FileExt};

pub fn one_file_checks<'manager>(
    repository: &mut impl Repository<'manager>,
    registry: &impl Registry,
) {
    let file_content = b"Foo\nBar\nCar";
    let file_id = FileId::unique();
    let file = File::parse(registry, &file_content[..]).unwrap();
    for change in repository.file_diff(registry, file_id, &file).unwrap() {
        repository.apply(change).unwrap();
    }
    let file = repository.render(file_id).unwrap();
    let mut final_content = Vec::new();
    file.write(registry, &mut final_content).unwrap();
    assert_eq!(final_content, file_content);

    // Test a change to the repository and unappling it
    let new_content_1 = b"Foo\nBar\nDavid\nCar";
    let file = File::parse(registry, &new_content_1[..]).unwrap();
    let changes_1 = repository.file_diff(registry, file_id, &file).unwrap();
    for change in changes_1.iter().copied() {
        repository.apply(change).unwrap();
    }
    let file = repository.render(file_id).unwrap();
    let mut final_content = Vec::new();
    file.write(registry, &mut final_content).unwrap();
    assert_eq!(final_content, new_content_1);
    for change in changes_1.iter() {
        repository.unapply(change.calculate_hash()).unwrap();
    }

    // Test an other change to the repository and unappling it
    let new_content_2 = b"Foo\nBar\nFrancis\nCar";
    let file = File::parse(registry, &new_content_2[..]).unwrap();
    let changes_2 = repository.file_diff(registry, file_id, &file).unwrap();
    for change in changes_2.iter().copied() {
        repository.apply(change).unwrap();
    }
    let file = repository.render(file_id).unwrap();
    let mut final_content = Vec::new();
    file.write(registry, &mut final_content).unwrap();
    assert_eq!(final_content, new_content_2);
    for change in changes_2.iter() {
        repository.unapply(change.calculate_hash()).unwrap();
    }

    // Apply the two change togethers
    for change in changes_1.iter().copied() {
        repository.apply(change).unwrap();
    }
    for change in changes_2.iter().copied() {
        repository.apply(change).unwrap();
    }
    let file = repository.render(file_id).unwrap();
    let mut final_content = Vec::new();
    file.write(registry, &mut final_content).unwrap();
    let first_possibility =
        b"Foo\nBar\n<<<<<<< CONFLICT\nDavid\n=======\nFrancis\n>>>>>>> CONFLICT\nCar";
    let second_possibility =
        b"Foo\nBar\n<<<<<<< CONFLICT\nFrancis\n=======\nDavid\n>>>>>>> CONFLICT\nCar";
    assert!(final_content == first_possibility || final_content == second_possibility);

    // If there is no changes to the output it should not create changes
    let file = File::parse(registry, &first_possibility[..]).unwrap();
    assert_eq!(
        repository.file_diff(registry, file_id, &file).unwrap(),
        HashSet::new()
    );
    let file = File::parse(registry, &second_possibility[..]).unwrap();
    assert_eq!(
        repository.file_diff(registry, file_id, &file).unwrap(),
        HashSet::new()
    );

    // Adding a line is not a problem even when there is a conflict
    let new_content_3 =
        b"Foo\nBar\n<<<<<<< CONFLICT\nDavid\n=======\nFrancis\n>>>>>>> CONFLICT\nCar\nHello";
    let file = File::parse(registry, &new_content_3[..]).unwrap();
    let changes_3 = repository.file_diff(registry, file_id, &file).unwrap();
    for change in changes_3.iter().copied() {
        repository.apply(change).unwrap();
    }
    let file = repository.render(file_id).unwrap();
    let mut final_content = Vec::new();
    file.write(registry, &mut final_content).unwrap();
    assert_eq!(final_content, new_content_3);

    // Removing a change can resolve the conflict
    for change in changes_2.iter() {
        repository.unapply(change.calculate_hash()).unwrap();
    }
    let file = repository.render(file_id).unwrap();
    let mut final_content = Vec::new();
    file.write(registry, &mut final_content).unwrap();
    assert_eq!(final_content, b"Foo\nBar\nDavid\nCar\nHello");

    // We reapply the conflict to check for conflict resolution after
    for change in changes_2.iter().copied() {
        repository.apply(change).unwrap();
    }
    let file = repository.render(file_id).unwrap();
    let mut final_content = Vec::new();
    file.write(registry, &mut final_content).unwrap();
    let first_possibility =
        b"Foo\nBar\n<<<<<<< CONFLICT\nDavid\n=======\nFrancis\n>>>>>>> CONFLICT\nCar\nHello";
    let second_possibility =
        b"Foo\nBar\n<<<<<<< CONFLICT\nFrancis\n=======\nDavid\n>>>>>>> CONFLICT\nCar\nHello";
    assert!(final_content == first_possibility || final_content == second_possibility);

    // Conflict resolution
    let file_content = b"Foo\nBar\nDavid\nCar\nHello";
    let file = File::parse(registry, &file_content[..]).unwrap();
    for change in repository.file_diff(registry, file_id, &file).unwrap() {
        repository.apply(change).unwrap();
    }
    let file = repository.render(file_id).unwrap();
    let mut final_content = Vec::new();
    file.write(registry, &mut final_content).unwrap();
    assert_eq!(final_content, file_content);

    // Adding a line for latter checks (deletion and addition)
    let new_content_4 = b"Foo\nBar\nDavid\nFun\nCar\nHello";
    let file = File::parse(registry, &new_content_4[..]).unwrap();
    let changes_4 = repository.file_diff(registry, file_id, &file).unwrap();
    for change in changes_4.iter().copied() {
        repository.apply(change).unwrap();
    }
    let file = repository.render(file_id).unwrap();
    let mut final_content = Vec::new();
    file.write(registry, &mut final_content).unwrap();
    assert_eq!(final_content, new_content_4);
    for change in changes_4.iter() {
        repository.unapply(change.calculate_hash()).unwrap();
    }

    // Removing lines to make a conflict
    let new_content_5 = b"Foo\nBar\nHello";
    let file = File::parse(registry, &new_content_5[..]).unwrap();
    let changes_5 = repository.file_diff(registry, file_id, &file).unwrap();
    for change in changes_5.iter().copied() {
        repository.apply(change).unwrap();
    }
    let file = repository.render(file_id).unwrap();
    let mut final_content = Vec::new();
    file.write(registry, &mut final_content).unwrap();
    assert_eq!(final_content, new_content_5);
    for change in changes_5.iter() {
        repository.unapply(change.calculate_hash()).unwrap();
    }

    // Apply the two changes
    for change in changes_4.iter().copied() {
        repository.apply(change).unwrap();
    }
    for change in changes_5.iter().copied() {
        repository.apply(change).unwrap();
    }
    let file = repository.render(file_id).unwrap();
    let mut final_content = Vec::new();
    file.write(registry, &mut final_content).unwrap();
    let first_possibility =
        b"Foo\nBar\n<<<<<<< CONFLICT\nDavid\nFun\nCar\n=======\n>>>>>>> CONFLICT\nHello";
    let second_possibility =
        b"Foo\nBar\n<<<<<<< CONFLICT\n=======\nDavid\nFun\nCar\n>>>>>>> CONFLICT\nHello";
    assert!(final_content == first_possibility || final_content == second_possibility);

    // If there is no changes to the output it should not create changes
    let file = File::parse(registry, &first_possibility[..]).unwrap();
    assert_eq!(
        repository.file_diff(registry, file_id, &file).unwrap(),
        HashSet::new()
    );
    let file = File::parse(registry, &second_possibility[..]).unwrap();
    assert_eq!(
        repository.file_diff(registry, file_id, &file).unwrap(),
        HashSet::new()
    );

    // Resolve the conflict
    let file_content = b"Foo\nBar\nCar\nFun\nDavid\nHello";
    let file = File::parse(registry, &file_content[..]).unwrap();
    for change in repository.file_diff(registry, file_id, &file).unwrap() {
        repository.apply(change).unwrap();
    }
    let file = repository.render(file_id).unwrap();
    let mut final_content = Vec::new();
    file.write(registry, &mut final_content).unwrap();
    assert_eq!(final_content, file_content);
}
