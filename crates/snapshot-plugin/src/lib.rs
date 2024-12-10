wit_bindgen::generate!({
    path: "../../wit",
    world: "file-plugin",
});

struct Plugin;

impl Guest for Plugin {
    fn render_file() {
        todo!()
    }

    fn calculate_diff(content_hash: String) -> Change {
        todo!()
    }

    fn apply_change(change: Change) {
        let timestamp = u128::from_be_bytes(
            change
                .plugin_data
                .try_into()
                .expect("invalid change timestamp"),
        );
        todo!()
    }

    fn unapply_change(change: Change) {
        todo!()
    }
}

export!(Plugin);
