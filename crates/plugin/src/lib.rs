pub use {base64, sha3, wit_bindgen as wit};

#[macro_export]
macro_rules! generate {
    ($folder:expr) => {
        $crate::wit::generate!({
            path: $folder,
            world: "plugin",
            runtime_path: "solipr_plugin::wit::rt",
            bitflags_path: "solipr_plugin::wit::bitflags",
        });

        impl Change {
            pub fn calculate_hash(&self) -> String {
                use $crate::base64::prelude::{Engine, BASE64_URL_SAFE_NO_PAD};
                use $crate::sha3::{Digest, Sha3_256};

                let mut hasher = Sha3_256::new();
                for used_content in &self.used_contents {
                    hasher.update(used_content.as_bytes());
                }
                hasher.update(&self.plugin_data);
                let result = hasher.finalize();
                BASE64_URL_SAFE_NO_PAD.encode(result)
            }
        }
    };
}
