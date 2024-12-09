wit_bindgen::generate!({
    path: "../../wit",
    world: "file-plugin",
});

struct Plugin;

impl Guest for Plugin {
    fn render_file() -> Result<Vec<u8>, String> {
        todo!()
    }

    fn calculate_diff(new_content: Vec<u8>) -> Result<Vec<u8>, String> {
        todo!()
    }

    fn apply_change(change: Vec<u8>) -> Result<(), String> {
        todo!()
    }

    fn unapply_change(change_hash: String) -> Result<(), String> {
        todo!()
    }
}

export!(Plugin);
