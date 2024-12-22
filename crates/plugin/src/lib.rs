pub use {base64, sha3, wit_bindgen as wit};

/// The `generate!` macro generates the bindings to define a plugin for Solipr.
///
/// The macro takes a single argument, which is a string literal representing
/// the path to the folder containing the WIT schema file of the plugin.
#[macro_export]
macro_rules! generate {
    () => {
        $crate::wit::generate!({
            inline: r"
                package solipr:plugin@0.1.0;

                /// Interface for a key-value store.
                ///
                /// Each file has its own key-value store, so the plugin can only access the data of the current file being processed.
                interface kv-store {
                    /// Reads the value associated with the given key.
                    ///
                    /// # Arguments
                    /// * `key` - A list of bytes representing the key.
                    ///
                    /// # Returns
                    /// An optional list of bytes representing the value. If the key does not exist, returns `None`.
                    kv-read: func(key: list<u8>) -> option<list<u8>>;

                    /// Writes a value to the given key.
                    ///
                    /// # Arguments
                    /// * `key` - A list of bytes representing the key.
                    /// * `value` - An optional list of bytes representing the value. If `None`, the key is deleted.
                    kv-write: func(key: list<u8>, value: option<list<u8>>);

                    /// Retrieves all keys that match the given prefix.
                    ///
                    /// # Arguments
                    /// * `prefix` - A list of bytes representing the prefix.
                    ///
                    /// # Returns
                    /// A list of lists of bytes, where each inner list represents a key that matches the prefix (with the prefix cutted out).
                    kv-keys: func(prefix: list<u8>) -> list<list<u8>>;
                }

                /// Interface for a repository.
                ///
                /// A repository is a storage for file contents and changes.
                ///
                /// Only the changes of the current file being processed are accessible but any file content can be read and written.
                interface repository {
                    /// Represents a change in the file.
                    record change {
                        /// The hash of the content used by the change.
                        ///
                        /// This is used to clean up unused contents if the change is deleted.
                        ///
                        /// This is also used to download the content if it is not available locally.
                        used-contents: list<string>,

                        /// The data of the change.
                        ///
                        /// This is simply a list of bytes, it is up to the plugin to write it and interpret it.
                        ///
                        /// This data should contain all the information needed to apply the
                        /// change to any repository even on a different machine.
                        plugin-data: list<u8>,
                    }

                    /// Returns the change associated with the given hash.
                    ///
                    /// # Arguments
                    /// * `change-hash` - A string representing the hash of the change.
                    ///
                    /// # Returns
                    /// An optional change. If the change does not exist, returns `None`.
                    change-read: func(change-hash: string) -> option<change>;

                    /// Reads the content associated with the given hash.
                    ///
                    /// # Arguments
                    /// * `content-hash` - A string representing the hash of the content to read.
                    ///
                    /// # Returns
                    /// An optional list of bytes representing the content. If the content does not exist, returns `None`.
                    content-read: func(content-hash: string) -> option<list<u8>>;

                    /// Reads a part of the content associated with the given hash.
                    ///
                    /// # Arguments
                    /// * `content-hash` - A string representing the hash of the content to read.
                    /// * `start` - The start offset of the part to read.
                    /// * `length` - The length of the part to read.
                    ///
                    /// # Returns
                    /// An optional list of bytes representing the content.
                    /// If the content does not exist, returns `None`.
                    content-read-at: func(content-hash: string, start: u64, length: u64) -> option<list<u8>>;

                    /// Returns the size of the content associated with the given hash.
                    ///
                    /// # Arguments
                    /// * `content-hash` - A string representing the hash of the content.
                    ///
                    /// # Returns
                    /// An optional u64 representing the size of the content. If the content does not exist, returns `None`.
                    content-size: func(content-hash: string) -> option<u64>;

                    /// Saves the given content.
                    ///
                    /// # Arguments
                    /// * `content-data` - A list of bytes representing the content to save.
                    ///
                    /// # Returns
                    /// A string representing the hash of the saved content.
                    content-save: func(content-data: list<u8>) -> string;

                    /// Creates a new content by cutting a part of an existing content.
                    ///
                    /// # Arguments
                    /// * `content-hash` - A string representing the hash of the content to cut.
                    /// * `start` - The start offset of the part to cut.
                    /// * `length` - The length of the part to cut, or `None` to cut until the end of the content.
                    ///
                    /// # Returns
                    /// An optional string representing the hash of the new content. If the content to cut does not exist, returns `None`.
                    content-cut: func(content-hash: string, start: u64, length: option<u64>) -> option<string>;
                }

                /// The plugin interface.
                world plugin {
                    use repository.{change};

                    import kv-store;
                    import repository;

                    /// Renders the given bytes to the output.
                    ///
                    /// This function can only be used in the `render-file` function.
                    ///
                    /// # Arguments
                    /// * `bytes` - A list of bytes to render.
                    import render-bytes: func(bytes: list<u8>);

                    /// Renders the content associated with the given hash to the output.
                    ///
                    /// This function can only be used in the `render-file` function.
                    ///
                    /// # Arguments
                    /// * `content-hash` - A string representing the hash of the content to render.
                    import render-content: func(content-hash: string);

                    /// Renders the file processed by the plugin to the output.
                    ///
                    /// This function should use the `render-bytes` and `render-content` functions to render the file.
                    export render-file: func();

                    /// Calculates the difference between the current file state and the given content hash.
                    ///
                    /// # Arguments
                    /// * `content-hash` - A string representing the hash of the content to compare with.
                    ///
                    /// # Returns
                    /// An optional change representing the difference. If there is no difference, returns `None`.
                    export calculate-diff: func(content-hash: string) -> option<change>;

                    /// Applies the given change to the file.
                    ///
                    /// Applying the same changes in any order should always result in the same file state.
                    ///
                    /// # Arguments
                    /// * `change` - The change to apply.
                    export apply-change: func(change: change);

                    /// Unapplies the given change from the file.
                    ///
                    /// After unapplying a change, the file should be in the same state as before applying the change.
                    ///
                    /// # Arguments
                    /// * `change` - The change to unapply.
                    export unapply-change: func(change: change);
                }
            ",
            world: "plugin",
            runtime_path: "solipr_plugin::wit::rt",
            bitflags_path: "solipr_plugin::wit::bitflags",
        });

        impl Change {
            /// Calculate the hash of the change.
            ///
            /// The hash is the base64 encoding of the SHA3-256 hash of the
            /// concatenation of the used contents and the plugin data.
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
