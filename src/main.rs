use anyhow::{bail, Result};
use clap::{Args, Parser, Subcommand};
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use sha1::{Digest, Sha1};
use std::ffi::CStr;
use std::fs::File;
use std::path::PathBuf;
use std::{env, io, process};
use std::{
    fs,
    io::{stdout, BufRead, BufReader, Read, Write},
    path::Path,
};

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
            let path = Path::new(".git/objects").join(&hash[..2]).join(&hash[2..]);
            let object = fs::read(path)?;

            let mut z = BufReader::new(ZlibDecoder::new(object.as_slice()));
            let mut buf = Vec::new();
            z.read_until(b'\0', &mut buf).unwrap();

            let header = CStr::from_bytes_with_nul(&buf)?.to_str()?;
            let (object_type, size) = header.split_once(' ').unwrap();
            if object_type != "blob" {
                bail!("Unknown object type: {object_type}");
            }
            let size = size.parse::<usize>()?;

            buf.resize(size, 0);
            z.read_exact(&mut buf)?;

            stdout().write_all(&buf)?;
        }
        Command::HashObject(HashObjectArgs { path, .. }) => {
            let size = fs::metadata(&path)?.len();
            let mut file = File::open(path)?;

            let tmp_path = Path::new(".git/objects").join(format!("tmp-{}", process::id()));
            let tmp_file = File::create(&tmp_path)?;

            let mut writer = HashWriter {
                writer: ZlibEncoder::new(tmp_file, Compression::default()),
                hasher: Sha1::new(),
            };

            let header = format!("blob {}\0", size);

            writer.write(header.as_bytes())?;
            io::copy(&mut file, &mut writer)?;

            writer.writer.finish()?;
            let hash = format!("{:x}", writer.hasher.finalize());

            let dir_path = Path::new(".git/objects").join(&hash[..2]);
            fs::create_dir_all(&dir_path)?;

            fs::rename(tmp_path, dir_path.join(&hash[2..]))?;

            println!("{hash}");
        }
    }
    Ok(())
}

struct HashWriter<W> {
    writer: W,
    hasher: Sha1,
}

impl<W: Write> Write for HashWriter<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.writer.write(buf)?;
        self.hasher.update(&buf[..n]);
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}
