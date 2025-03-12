//! A plugin that manages binary files in Solipr.

#![allow(clippy::too_many_arguments)]

use std::collections::BTreeSet;

wit_bindgen::generate!({
    path: "../../wit",
    world: "document-plugin",
});

/// The plugin that manages binary files in Solipr.
struct Plugin;

/// The byte sequence used to mark a conflict in the document.
const CONFLICT_MARKER: &[u8] = &[
    0x11, 0x99, 0x13, 0xAD, 0x8F, 0xE3, 0x76, 0xE1, 0xDD, 0x53, 0xF7, 0x7B, 0x76, 0xB0, 0xBC, 0x58,
    0xA5, 0xE5, 0x26, 0x9E, 0x54, 0x7A, 0x94, 0x12, 0xE9, 0x34, 0x5B, 0xEC, 0x50, 0xFF, 0x4C, 0x1C,
    0x90, 0x6A, 0xE9, 0x1E, 0x22, 0x56, 0x5B, 0x5D, 0x9C, 0xB2, 0x46, 0xB1, 0x50, 0xF0, 0x3D, 0x99,
    0x94, 0xAF, 0x3F, 0x69, 0xE7, 0x39, 0x90, 0x8B, 0xDA, 0x1A, 0xFB, 0x71, 0x04, 0xB3, 0x8D, 0xAE,
];

impl Guest for Plugin {
    fn render_document(
        registry: &ReadRegistry,
        document: &Document,
        store: &ReadKvStore,
        renderer: &Renderer,
    ) {
        let contents = store
            .keys(b"")
            .into_iter()
            .map(|(change_hash, _)| String::from_utf8(change_hash).expect("invalid change hash"))
            .map(|change_hash| document.get_change(&change_hash).expect("head not found"))
            .flat_map(|change| change.used_contents)
            .collect::<BTreeSet<_>>();
        if contents.len() > 1 {
            renderer.render_bytes(CONFLICT_MARKER);
            renderer.render_bytes(contents.len().to_be_bytes().as_slice());
            for content_hash in &contents {
                renderer.render_bytes(
                    registry
                        .size(content_hash)
                        .expect("content not found")
                        .to_be_bytes()
                        .as_slice(),
                );
            }
        }
        for content_hash in &contents {
            renderer.render_content(content_hash);
        }
    }

    fn calculate_diff(
        registry: &WriteRegistry,
        document: &Document,
        store: &ReadKvStore,
        target_content: String,
    ) -> Option<Change> {
        let mut contents = BTreeSet::new();
        let header = registry
            .read(&target_content, 0, Some(CONFLICT_MARKER.len() as u64))
            .unwrap();
        if header == CONFLICT_MARKER {
            let conflict_count = usize::from_be_bytes(
                registry
                    .read(&target_content, CONFLICT_MARKER.len() as u64, Some(4))
                    .unwrap()
                    .try_into()
                    .expect("invalid conflict count"), // TODO: Handle this error
            );
            let mut position = CONFLICT_MARKER.len() as u64 + 2 + conflict_count as u64 * 8;
            for i in 0..conflict_count as u64 {
                let size = u64::from_be_bytes(
                    registry
                        .read(
                            &target_content,
                            CONFLICT_MARKER.len() as u64 + 2 + i * 8,
                            Some(8),
                        )
                        .unwrap()
                        .try_into()
                        .expect("invalid conflict size"), // TODO: Handle this error
                );
                contents.insert(registry.cut(&target_content, position, Some(size)).unwrap());
                position += size;
            }
        } else {
            contents.insert(target_content);
        }
        let current_heads = store
            .keys(b"")
            .into_iter()
            .map(|(change_hash, _)| String::from_utf8(change_hash).expect("invalid change hash"))
            .collect::<BTreeSet<String>>();
        let current_contents = current_heads
            .iter()
            .map(|change_hash| document.get_change(change_hash).expect("head not found"))
            .flat_map(|change| change.used_contents)
            .collect::<BTreeSet<String>>();
        if contents == current_contents {
            None
        } else {
            Some(Change {
                dependencies: current_heads.into_iter().collect(),
                used_contents: contents.into_iter().collect(),
                plugin_data: Vec::new(),
            })
        }
    }

    fn apply_change(
        _registry: &ReadRegistry,
        _document: &Document,
        store: &WriteKvStore,
        change_hash: String,
        change: Change,
    ) {
        store.write(change_hash.as_bytes(), Some(b""));
        for dependency in change.dependencies {
            store.write(dependency.as_bytes(), None);
        }
    }

    fn unapply_change(
        _registry: &ReadRegistry,
        document: &Document,
        store: &WriteKvStore,
        change_hash: String,
        change: Change,
    ) {
        store.write(change_hash.as_bytes(), None);
        for dependency in change.dependencies {
            if document.dependent_changes(&dependency).is_empty() {
                store.write(dependency.as_bytes(), Some(b""));
            }
        }
    }
}

export!(Plugin);
