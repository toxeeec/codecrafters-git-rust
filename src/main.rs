mod object;
mod tree_entry;

use anyhow::{bail, Result};
use clap::{Args, Parser, Subcommand};
use fallible_iterator::FallibleIterator;
use std::path::{Path, PathBuf};
use std::{env, io};
use std::{
    fs,
    io::{stdout, Write},
};

use crate::object::{write_blob, write_commit, write_tree, Kind, Object, TreeIterator};

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Init,
    CatFile(CatFileArgs),
    HashObject(HashObjectArgs),
    LsTree(LsTreeArgs),
    WriteTree,
    CommitTree(CommitTreeArgs),
}

#[derive(Args)]
struct CatFileArgs {
    #[arg(short, required = true)]
    pretty_print: bool,
    hash: String,
}

#[derive(Args)]
struct HashObjectArgs {
    #[arg(short, required = true)]
    write: bool,
    path: PathBuf,
}

#[derive(Args)]
struct LsTreeArgs {
    #[arg(long)]
    name_only: bool,
    hash: String,
}

#[derive(Args)]
struct CommitTreeArgs {
    hash: String,
    #[arg(short)]
    message: String,
    #[arg(short)]
    parent: Option<String>,
}

fn main() -> Result<()> {
    let args = Cli::parse();

    match &args.command {
        Command::Init => {
            fs::create_dir(".git")?;
            fs::create_dir(".git/objects")?;
            fs::create_dir(".git/refs")?;
            fs::write(".git/HEAD", "ref: refs/heads/main\n")?;
            println!(
                "Initialized empty Git repository in {}",
                env::current_dir()?.display()
            );
        }
        Command::CatFile(CatFileArgs { hash, .. }) => {
            let mut object = Object::read(hash)?;
            match object.kind {
                Kind::Blob => io::copy(&mut object.reader, &mut stdout().lock())?,
                _ => bail!("Not a blob"),
            };
        }
        Command::HashObject(HashObjectArgs { path, .. }) => {
            let hash = write_blob(path)?;
            println!("{}", hex::encode(hash));
        }
        Command::LsTree(LsTreeArgs { name_only, hash }) => {
            let object = Object::read(hash)?;
            match object.kind {
                Kind::Tree => {
                    let mut stdout = stdout().lock();
                    TreeIterator::new(object.reader).for_each(|entry| {
                        if *name_only {
                            stdout.write_all(&entry.name)?
                        } else {
                            write!(
                                stdout,
                                "{:06o} {} {}\t",
                                entry.mode,
                                entry.object_type(),
                                hex::encode(entry.hash)
                            )?;
                            stdout.write_all(&entry.name)?;
                        }
                        writeln!(stdout, "")?;
                        Ok(())
                    })?;
                }
                _ => bail!("Not a tree"),
            }
        }
        Command::WriteTree => {
            let hash = write_tree(Path::new("."))?;
            println!("{}", hex::encode(hash));
        }
        Command::CommitTree(CommitTreeArgs {
            hash,
            message,
            parent,
        }) => {
            let hash = write_commit(hash, message, parent.as_deref())?;
            println!("{}", hex::encode(hash));
        }
    }
    Ok(())
}
