use anyhow::{bail, Result};
use std::{cmp::Ordering, fmt, fs::DirEntry, os::unix::fs::PermissionsExt};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum TreeEntryMode {
    NormalFile = 0o100644,
    ExecutableFile = 0o100755,
    Symlink = 0o120000,
    Directory = 0o40000,
}

#[derive(Debug)]
pub(crate) struct TreeEntry {
    pub(crate) mode: TreeEntryMode,
    pub(crate) name: Box<[u8]>,
    pub(crate) hash: [u8; 20],
}

impl TreeEntry {
    pub(crate) fn new(mode: TreeEntryMode, name: &[u8], hash: [u8; 20]) -> Self {
        Self {
            mode,
            name: name.into(),
            hash,
        }
    }

    pub(crate) fn object_type(&self) -> &'static str {
        match self.mode {
            TreeEntryMode::Directory => "tree",
            _ => "blob",
        }
    }
}

impl PartialEq for TreeEntry {
    fn eq(&self, other: &Self) -> bool {
        if self.mode.is_directory() && !other.mode.is_directory() {
            return false;
        }
        self.name == other.name
    }
}

impl Eq for TreeEntry {}

impl Ord for TreeEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        let name1 = &self.name;
        let name2 = &other.name;
        let len = name1.len().min(name2.len());

        match name1[..len].cmp(&name2[..len]) {
            Ordering::Equal => {
                let c1 = name1.get(len).copied().or(if self.mode.is_directory() {
                    Some(b'/')
                } else {
                    None
                });
                let c2 = name2.get(len).copied().or(if other.mode.is_directory() {
                    Some(b'/')
                } else {
                    None
                });

                c1.cmp(&c2)
            }
            ord => ord,
        }
    }
}

impl PartialOrd for TreeEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl TreeEntryMode {
    fn is_directory(self) -> bool {
        self == Self::Directory
    }
}

impl TryFrom<&DirEntry> for TreeEntryMode {
    type Error = anyhow::Error;
    fn try_from(value: &DirEntry) -> Result<Self, Self::Error> {
        let file_type = value.file_type()?;
        let mode = value.metadata()?.permissions().mode();

        Ok(if file_type.is_dir() {
            Self::Directory
        } else if file_type.is_symlink() {
            Self::Symlink
        } else if mode & 0o111 != 0 {
            Self::ExecutableFile
        } else {
            Self::NormalFile
        })
    }
}

impl TryFrom<&str> for TreeEntryMode {
    type Error = anyhow::Error;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "100644" => Ok(Self::NormalFile),
            "100755" => Ok(Self::ExecutableFile),
            "120000" => Ok(Self::Symlink),
            "40000" => Ok(Self::Directory),
            _ => bail!("Unknown mode: {}", value),
        }
    }
}

impl fmt::Octal for TreeEntryMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Octal::fmt(&(*self as u32), f)
    }
}
