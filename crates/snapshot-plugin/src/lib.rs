use base64::prelude::{BASE64_URL_SAFE_NO_PAD, Engine};
use sha3::{Digest, Sha3_256};

wit_bindgen::generate!({
    path: "../../wit",
    world: "file-plugin",
});

impl Change {
    pub fn calculate_hash(&self) -> String {
        let mut hasher = Sha3_256::new();
        for used_content in &self.used_contents {
            hasher.update(used_content.as_bytes());
        }
        hasher.update(&self.plugin_data);
        let result = hasher.finalize();
        BASE64_URL_SAFE_NO_PAD.encode(result)
    }
}

struct Plugin;

impl Guest for Plugin {
    fn render_file() {
        todo!()
    }

    fn calculate_diff(content_hash: String) -> Option<Change> {
        todo!()
    }

    fn apply_change(change: Change) {
        todo!()
    }

    fn unapply_change(change: Change) {
        todo!()
    }
}

export!(Plugin);
