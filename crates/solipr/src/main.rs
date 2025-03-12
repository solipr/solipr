//! A CLI interface to use Solipr.

use std::fs;
use std::io::{self, Write, stdin, stdout};
use std::path::PathBuf;

use anyhow::{Context, bail};
use clap::{Parser, Subcommand};
use solipr::identifier::{ChangeHash, DocumentId, PluginHash};
use solipr::plugin::{PluginReadDocument, PluginWriteDocument, RenderBlock};
use solipr::repository::Repository;
use solipr::storage::Registry;

/// The CLI arguments for Solipr.
#[derive(Parser)]
struct Cli {
    /// The command to execute.
    #[command(subcommand)]
    command: Commands,
}

/// The commands that can be executed by Solipr.
#[derive(Subcommand)]
enum Commands {
    /// Initialize a new repository in the current directory.
    Init,

    /// Register a new plugin and return its identifier.
    Register {
        /// The path to the plugin file to register.
        path: PathBuf,
    },

    /// Create a new document using the given plugin.
    Create {
        /// The identifier of the plugin to use for creating the new document.
        plugin: PluginHash,
    },

    /// Calculate a change that updates a document to look like the standard
    /// input.
    Diff {
        /// The identifier of the document to diff against.
        document: DocumentId,
    },

    /// Render a document to the console.
    Render {
        /// The identifier of the document to render.
        document: DocumentId,
    },

    /// Apply a change to a document.
    Apply {
        /// The identifier of the document to apply the change to.
        document: DocumentId,

        /// The identifier of the change to apply.
        change: ChangeHash,
    },

    /// Unapply a change from a document.
    Unapply {
        /// The identifier of the document to unapply the change from.
        document: DocumentId,

        /// The identifier of the change to unapply.
        change: ChangeHash,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Init => {
            if fs::exists(".solipr")? {
                bail!("already in a solipr repository");
            }
            fs::create_dir(".solipr")?;
            fs::create_dir(".solipr/registry")?;
            Repository::open(".solipr/repository")?;
        }
        other => {
            if !fs::exists(".solipr")? {
                bail!("not in a solipr repository");
            }
            let registry = Registry::open(".solipr/registry");
            let repository = Repository::open(".solipr/repository")?;
            match other {
                Commands::Init => unreachable!(),
                Commands::Register { path } => {
                    let bytes = &fs::read(path)?;
                    let bytes = wit_component::ComponentEncoder::default()
                        .module(bytes)?
                        .encode()?;
                    let hash = registry.write(&bytes[..])?;
                    println!("{}", PluginHash::from(hash));
                }
                Commands::Create { plugin } => {
                    println!("{}", DocumentId::new(plugin));
                }
                Commands::Diff { document } => {
                    let target_content = registry.write(stdin())?;
                    let repository = repository.read()?;
                    let document = repository.open(document)?;
                    let mut document = PluginReadDocument::new(registry.clone(), document)?;
                    let change = document.calculate_diff(target_content)?;
                    match change {
                        Some(change) => {
                            let change_hash = registry.write(&borsh::to_vec(&change)?[..])?;
                            println!("{}", ChangeHash::from(change_hash));
                        }
                        None => println!("there is no difference"),
                    }
                }
                Commands::Render { document } => {
                    let repository = repository.read()?;
                    let document = repository.open(document)?;
                    let mut document = PluginReadDocument::new(registry.clone(), document)?;
                    let content = document.render_document()?;
                    let mut stdout = stdout();
                    for block in content {
                        match block {
                            RenderBlock::Bytes(bytes) => stdout.write_all(&bytes)?,
                            RenderBlock::Content(content_hash) => {
                                let Some(mut result) = registry.read(content_hash)? else {
                                    bail!("content not found: {content_hash}");
                                };
                                io::copy(&mut result, &mut stdout)?;
                            }
                        }
                    }
                }
                Commands::Apply { document, change } => {
                    let repository = repository.write()?;
                    let document = repository.open(document)?;
                    let mut document = PluginWriteDocument::new(registry.clone(), document)?;
                    let change = borsh::from_reader(
                        &mut registry
                            .read(change.into())?
                            .context(format!("change not found: {change}"))?,
                    )?;
                    match document.apply(&change)? {
                        Ok(()) => println!("Change applied"),
                        Err(needed_dependencies) => {
                            bail!("dependencies need to be applied first: {needed_dependencies:#?}")
                        }
                    }
                    drop(document);
                    repository.commit()?;
                }
                Commands::Unapply { document, change } => {
                    let repository = repository.write()?;
                    let document = repository.open(document)?;
                    let mut document = PluginWriteDocument::new(registry, document)?;

                    match document.unapply(change)? {
                        Ok(()) => println!("Change unapplied"),
                        Err(dependents) => {
                            bail!("some changes depends on this one: {dependents:#?}")
                        }
                    }
                    drop(document);
                    repository.commit()?;
                }
            }
        }
    }
    Ok(())
}
