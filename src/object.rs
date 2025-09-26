use anyhow::{bail, Result};
use fallible_iterator::FallibleIterator;
use flate2::{read::ZlibDecoder, write::ZlibEncoder, Compression};
use sha1::{Digest, Sha1};
use std::fmt::Write as FmtWrite;
use std::{
    collections::BTreeSet,
    ffi::CStr,
    fmt,
    fs::File,
    io::{self, BufRead, BufReader, Cursor, Read, Write},
    process,
    time::{SystemTime, UNIX_EPOCH},
};
use std::{fs, path::Path};
use time::{macros::format_description, OffsetDateTime};

use crate::tree_entry::TreeEntry;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    Blob,
    Tree,
    Commit,
}

#[derive(Debug)]
pub(crate) struct Object<R> {
    pub(crate) kind: Kind,
    pub(crate) reader: R,
    pub(crate) size: u64,
}

#[derive(Debug)]
pub(crate) struct TreeIterator<R> {
    reader: R,
    scratch: Vec<u8>,
}

#[derive(Debug)]
struct HashWriter<W> {
    writer: W,
    hasher: Sha1,
}

impl TryFrom<&str> for Kind {
    type Error = anyhow::Error;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "blob" => Ok(Kind::Blob),
            "tree" => Ok(Kind::Tree),
            "commit" => Ok(Kind::Commit),
            _ => bail!("Unkown kind: {}", value),
        }
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Kind::Blob => f.write_str("blob"),
            Kind::Tree => f.write_str("tree"),
            Kind::Commit => f.write_str("commit"),
        }
    }
}

impl Object<()> {
    pub(crate) fn read(hash: &str) -> Result<Object<impl BufRead>> {
        let path = format!(".git/objects/{}/{}", &hash[..2], &hash[2..]);
        let file = File::open(path)?;

        let mut z = BufReader::new(ZlibDecoder::new(file));
        let mut buf = Vec::new();
        z.read_until(b'\0', &mut buf)?;

        let header = CStr::from_bytes_with_nul(&buf)?.to_str()?;
        let (kind, size) = header.split_once(' ').unwrap();
        let kind = kind.try_into()?;
        let size = size.parse::<u64>()?;

        Ok(Object {
            kind,
            reader: z.take(size),
            size,
        })
    }
}

impl<R: Read> Object<R> {
    fn write(mut self) -> Result<[u8; 20]> {
        let tmp_path = format!(".git/objects/tmp-{}", process::id());
        let tmp_file = File::create(&tmp_path)?;

        let mut writer = HashWriter {
            writer: ZlibEncoder::new(tmp_file, Compression::default()),
            hasher: Sha1::new(),
        };

        let header = format!("{} {}\0", self.kind, self.size);
        writer.write_all(header.as_bytes())?;
        io::copy(&mut self.reader, &mut writer)?;

        writer.writer.finish()?;
        let hash = writer.hasher.finalize();
        let hash_hex = hex::encode(hash);

        let dir_path = Path::new(".git/objects").join(&hash_hex[..2]);
        fs::create_dir_all(&dir_path)?;

        fs::rename(tmp_path, dir_path.join(&hash_hex[2..]))?;

        Ok(hash.into())
    }
}

impl<R> TreeIterator<R> {
    pub(crate) fn new(reader: R) -> TreeIterator<R> {
        Self {
            reader,
            scratch: Vec::with_capacity(6),
        }
    }
}

impl<R: BufRead> FallibleIterator for TreeIterator<R> {
    type Item = TreeEntry;
    type Error = anyhow::Error;

    fn next(&mut self) -> Result<Option<Self::Item>, Self::Error> {
        self.scratch.clear();
        let n = self.reader.read_until(b' ', &mut self.scratch)?;
        if n == 0 {
            return Ok(None);
        }

        let mode = str::from_utf8(&self.scratch[..self.scratch.len() - 1])?;
        let mode = mode.try_into()?;

        let mut name = Vec::new();
        self.reader.read_until(b'\0', &mut name)?;
        let name = CStr::from_bytes_with_nul(&name)?.to_bytes();

        let mut hash = [0; 20];
        self.reader.read_exact(&mut hash)?;

        Ok(Some(TreeEntry::new(mode, name, hash)))
    }
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

pub(crate) fn write_blob(path: &Path) -> Result<[u8; 20]> {
    let file = File::open(path)?;
    let metadata = file.metadata()?;
    let object = Object {
        kind: Kind::Blob,
        reader: file,
        size: metadata.len(),
    };

    object.write()
}

pub(crate) fn write_tree(path: &Path) -> Result<[u8; 20]> {
    let dir = fs::read_dir(path)?;

    let mut entries = BTreeSet::new();

    for entry in dir {
        let entry = entry?;
        let name = entry.file_name();
        if name == ".git" {
            continue;
        }

        let mode = (&entry).try_into()?;
        let name = name.as_encoded_bytes();

        let file_type = entry.file_type()?;
        let hash = if file_type.is_dir() {
            write_tree(&entry.path())?
        } else {
            write_blob(&entry.path())?
        };

        entries.insert(TreeEntry::new(mode, name, hash));
    }

    let mut buf = Vec::new();
    for entry in entries {
        buf.extend_from_slice(format!("{:o}", entry.mode).as_bytes());
        buf.push(b' ');
        buf.extend_from_slice(&entry.name);
        buf.push(b'\0');
        buf.extend_from_slice(&entry.hash);
    }

    let object = Object {
        kind: Kind::Tree,
        size: buf.len() as u64,
        reader: Cursor::new(buf),
    };

    object.write()
}

pub(crate) fn write_commit(hash: &str, message: &str, parent: Option<&str>) -> Result<[u8; 20]> {
    let object = Object::read(hash)?;
    if object.kind != Kind::Tree {
        bail!("Not a tree");
    }
    let mut buf = String::new();

    writeln!(buf, "tree {hash}")?;

    if let Some(parent) = parent {
        writeln!(buf, "parent {parent}")?;
    }

    let timestamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();

    let local = OffsetDateTime::now_local()?;
    let timezone = local.format(&format_description!(
        "[offset_hour sign:mandatory][offset_minute]"
    ))?;

    writeln!(
        buf,
        "author toxeeec <bartosz.kapciak@gmail.com> {timestamp} {timezone}",
    )?;
    writeln!(
        buf,
        "commiter toxeeec <bartosz.kapciak@gmail.com> {timestamp} {timezone}",
    )?;
    writeln!(buf, "")?;
    writeln!(buf, "{message}")?;

    let object = Object {
        kind: Kind::Commit,
        size: buf.len() as u64,
        reader: Cursor::new(buf),
    };

    object.write()
}
