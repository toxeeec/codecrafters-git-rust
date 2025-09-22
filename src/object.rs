use std::{
    ffi::CStr,
    fs::File,
    io::{BufRead, BufReader, Read},
};

use anyhow::{bail, Result};
use flate2::read::ZlibDecoder;

pub(crate) enum Object<R> {
    Blob(R),
    Tree(TreeObject),
}

pub(crate) struct TreeObject(Vec<u8>);

pub(crate) struct TreeObjectIterator<'a>(&'a [u8]);

pub(crate) struct TreeObjectEntry<'a> {
    pub(crate) mode: &'a str,
    pub(crate) name: &'a [u8],
    hash: &'a [u8; 20],
}

impl Object<()> {
    pub(crate) fn from_hash(hash: &str) -> Result<Object<impl BufRead>> {
        let path = format!(".git/objects/{}/{}", &hash[..2], &hash[2..]);
        let file = File::open(path)?;

        let mut z = BufReader::new(ZlibDecoder::new(file));
        let mut buf = Vec::new();
        z.read_until(b'\0', &mut buf).unwrap();

        let header = CStr::from_bytes_with_nul(&buf)?.to_str()?;
        let (object_type, size) = header.split_once(' ').unwrap();
        let size = size.parse::<usize>()?;

        match object_type {
            "blob" => Ok(Object::Blob(z.take(size as u64))),
            "tree" => {
                buf.resize(size, 0);
                z.read_exact(&mut buf)?;
                Ok(Object::Tree(TreeObject(buf)))
            }
            _ => bail!("Unknown object type: {object_type}"),
        }
    }
}

impl<'a> IntoIterator for &'a TreeObject {
    type Item = TreeObjectEntry<'a>;
    type IntoIter = TreeObjectIterator<'a>;

    fn into_iter(self) -> Self::IntoIter {
        TreeObjectIterator(&self.0)
    }
}

impl<'a> Iterator for TreeObjectIterator<'a> {
    type Item = TreeObjectEntry<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        let Some(space_pos) = self.0.iter().position(|&b| b == b' ') else {
            return None;
        };
        let mode = str::from_utf8(&self.0[..space_pos]).unwrap();
        self.0 = &self.0[space_pos + 1..];

        let Some(null_pos) = self.0.iter().position(|&b| b == b'\0') else {
            return None;
        };
        let name = &self.0[..null_pos];
        self.0 = &self.0[null_pos + 1..];

        let Some((hash, rest)) = self.0.split_first_chunk::<20>() else {
            return None;
        };
        self.0 = rest;

        Some(TreeObjectEntry { mode, name, hash })
    }
}

impl TreeObjectEntry<'_> {
    pub(crate) fn typ(&self) -> &str {
        match self.mode {
            "100644" | "100755" | "120000" => "blob",
            "40000" => "tree",
            _ => panic!("Unknown mode: {}", self.mode),
        }
    }

    pub(crate) fn hash(&self) -> String {
        self.hash.iter().map(|b| format!("{:02x}", b)).collect()
    }
}
