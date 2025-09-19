use anyhow::bail;
use anyhow::ensure;
use anyhow::Result;
use flate2::read::ZlibDecoder;
use std::env;
use std::ffi::CStr;
use std::fs;
use std::io::stdout;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Read;
use std::io::Write;
use std::path::Path;

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();
    ensure!(args.len() > 1, "Usage: git <command>");

    match args[1].as_str() {
        "init" => {
            fs::create_dir(".git")?;
            fs::create_dir(".git/objects")?;
            fs::create_dir(".git/refs")?;
            fs::write(".git/HEAD", "ref: refs/heads/main\n")?;
            println!(
                "Initialized empty Git repository in {}",
                env::current_dir()?.display()
            );
        }
        "cat-file" => {
            let hash = &args[3];
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
        _ => {
            bail!("unknown command: {}", args[1]);
        }
    }

    Ok(())
}
