use std::collections::{BTreeSet, HashSet};

use solipr::plugin::kv_store::{kv_keys, kv_write};
use solipr::plugin::repository::{change_read, content_cut, content_read_at, content_size};

solipr_plugin::generate!("../../wit");

struct Plugin;

const CONFLICT_MARKER: &[u8] = &[
    0x11, 0x99, 0x13, 0xAD, 0x8F, 0xE3, 0x76, 0xE1, 0xDD, 0x53, 0xF7, 0x7B, 0x76, 0xB0, 0xBC, 0x58,
    0xA5, 0xE5, 0x26, 0x9E, 0x54, 0x7A, 0x94, 0x12, 0xE9, 0x34, 0x5B, 0xEC, 0x50, 0xFF, 0x4C, 0x1C,
    0x90, 0x6A, 0xE9, 0x1E, 0x22, 0x56, 0x5B, 0x5D, 0x9C, 0xB2, 0x46, 0xB1, 0x50, 0xF0, 0x3D, 0x99,
    0x94, 0xAF, 0x3F, 0x69, 0xE7, 0x39, 0x90, 0x8B, 0xDA, 0x1A, 0xFB, 0x71, 0x04, 0xB3, 0x8D, 0xAE,
];

impl Guest for Plugin {
    fn render_file() {
        let contents = kv_keys(b"heads/")
            .into_iter()
            .map(|change_hash| String::from_utf8(change_hash).expect("invalid change hash"))
            .map(|change_hash| change_read(&change_hash).expect("head not found"))
            .flat_map(|change| change.used_contents)
            .collect::<BTreeSet<String>>();
        if contents.len() > 1 {
            render_bytes(CONFLICT_MARKER);
            render_bytes((contents.len() as u16).to_be_bytes().as_slice());
            for content_hash in &contents {
                render_bytes(
                    content_size(content_hash)
                        .expect("content not found")
                        .to_be_bytes()
                        .as_slice(),
                );
            }
        }
        for content_hash in &contents {
            render_content(content_hash);
        }
    }

    fn calculate_diff(content_hash: String) -> Option<Change> {
        let mut contents = BTreeSet::new();
        let header = content_read_at(&content_hash, 0, CONFLICT_MARKER.len() as u64).unwrap();
        if header == CONFLICT_MARKER {
            let conflict_count = u16::from_be_bytes(
                content_read_at(&content_hash, CONFLICT_MARKER.len() as u64, 2)
                    .unwrap()
                    .try_into()
                    .expect("invalid conflict count"),
            );
            let mut position = CONFLICT_MARKER.len() as u64 + 2 + conflict_count as u64 * 8;
            for i in 0..conflict_count as u64 {
                let size = u64::from_be_bytes(
                    content_read_at(&content_hash, CONFLICT_MARKER.len() as u64 + 2 + i * 8, 8)
                        .unwrap()
                        .try_into()
                        .expect("invalid conflict size"),
                );
                contents.insert(content_cut(&content_hash, position, Some(size)).unwrap());
                position += size;
            }
        } else {
            contents.insert(content_hash);
        }
        let current_heads = kv_keys(b"heads/")
            .into_iter()
            .map(|change_hash| String::from_utf8(change_hash).expect("invalid change hash"))
            .collect::<HashSet<String>>();
        let current_contents = current_heads
            .iter()
            .map(|change_hash| change_read(change_hash).expect("head not found"))
            .flat_map(|change| change.used_contents)
            .collect::<BTreeSet<String>>();
        if contents == current_contents {
            None
        } else {
            Some(Change {
                used_contents: contents.into_iter().collect(),
                plugin_data: borsh::to_vec(&current_heads).unwrap(),
            })
        }
    }

    fn apply_change(change: Change) {
        let change_hash = change.calculate_hash();
        let replace: HashSet<String> =
            borsh::from_slice(&change.plugin_data).expect("invalid plugin data");
        for replace_hash in replace {
            kv_write(format!("heads/{}", replace_hash).as_bytes(), None);
            kv_write(
                format!("reverse/{replace_hash}/{change_hash}").as_bytes(),
                Some(b""),
            );
        }
        if kv_keys(format!("reverse/{change_hash}/").as_bytes()).is_empty() {
            kv_write(format!("heads/{}", change_hash).as_bytes(), Some(b""));
        }
    }

    fn unapply_change(change: Change) {
        let change_hash = change.calculate_hash();
        let replace: HashSet<String> =
            borsh::from_slice(&change.plugin_data).expect("invalid plugin data");
        for replace_hash in replace {
            kv_write(
                format!("reverse/{replace_hash}/{change_hash}").as_bytes(),
                None,
            );
            if kv_keys(format!("reverse/{replace_hash}/").as_bytes()).is_empty() {
                kv_write(format!("heads/{}", replace_hash).as_bytes(), Some(b""));
            }
        }
        kv_write(format!("heads/{}", change_hash).as_bytes(), None);
    }
}

export!(Plugin);
