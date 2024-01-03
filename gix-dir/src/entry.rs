use crate::{Entry, EntryRef};
use std::borrow::Cow;

/// The kind of the entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum Kind {
    /// The entry is a blob, executable or not.
    File,
    /// The entry is a symlink.
    Symlink,
    /// A directory that contains no file or directory.
    EmptyDirectory,
    /// The entry is an ordinary directory.
    Directory,
    /// The entry is a directory which *contains* a `.git` folder.
    Repository,
}

/// The kind of entry as obtained from a directory.
///
/// The order of variants roughly relates from cheap-to-compute to most expensive, as each level needs more tests to assert.
/// Thus, `DotGit` is the cheapest, while `Untracked` is among the most expensive and one of the major outcomes of any
/// [`walk`](crate::walk()) run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum Status {
    /// The filename of an entry was `.git`, which is generally pruned.
    DotGit,
    /// The provided pathspec prevented further processing as the path didn't match, or it is a `.git` directory.
    /// If this happens, no further checks are done so we wouldn't know if the path is also ignored for example (by mention in `.gitignore`).
    Pruned,
    /// Always in conjunction with a directory on disk that is also known as cone-mode sparse-checkout exclude marker - i.e. a directory
    /// that is excluded, so its whole content is excluded and not checked out nor is part of the index.
    TrackedExcluded,
    /// The entry is tracked in Git.
    Tracked,
    /// The entry is ignored as per `.gitignore` files and their rules.
    ///
    /// If this is a directory, then its entire contents is ignored. Otherwise, possibly due to configuration, individual ignored files are listed.
    Ignored(gix_ignore::Kind),
    /// The entry is not tracked by git yet, it was not found in the [index](gix_index::State).
    ///
    /// If it's a directory, the entire directory contents is untracked.
    Untracked,
}

impl EntryRef<'_> {
    /// Strip the lifetime to obtain a fully owned copy.
    pub fn to_owned(&self) -> Entry {
        Entry {
            rela_path: self.rela_path.clone().into_owned(),
            status: self.status,
            kind: self.kind,
        }
    }

    /// Turn this instance into a fully owned copy.
    pub fn into_owned(self) -> Entry {
        Entry {
            rela_path: self.rela_path.into_owned(),
            status: self.status,
            kind: self.kind,
        }
    }
}

impl Entry {
    /// Obtain an [`EntryRef`] from this instance.
    pub fn to_ref(&self) -> EntryRef<'_> {
        EntryRef {
            rela_path: Cow::Borrowed(self.rela_path.as_ref()),
            status: self.status,
            kind: self.kind,
        }
    }
}

impl From<std::fs::FileType> for Kind {
    fn from(value: std::fs::FileType) -> Self {
        if value.is_dir() {
            Kind::Directory
        } else if value.is_symlink() {
            Kind::Symlink
        } else {
            Kind::File
        }
    }
}

impl Status {
    /// Return true if this status is considered pruned. A pruned entry is typically hidden from view due to a pathspec.
    pub fn is_pruned(&self) -> bool {
        match self {
            Status::DotGit | Status::TrackedExcluded | Status::Pruned => true,
            Status::Ignored(_) | Status::Untracked | Status::Tracked => false,
        }
    }
    /// Return `true` if this directory isn't ignored, and is not a repository.
    /// This implements the default rules of `git status`, which is good for a minimal traversal through
    /// tracked and non-ignored portions of a worktree.
    pub fn can_recurse(&self, file_type: Option<Kind>) -> bool {
        if file_type.map_or(true, |ft| !ft.is_recursable_dir()) {
            return false;
        }
        match self {
            Status::DotGit | Status::TrackedExcluded | Status::Pruned | Status::Ignored(_) => false,
            Status::Untracked | Status::Tracked => true,
        }
    }
}

impl Kind {
    fn is_recursable_dir(&self) -> bool {
        matches!(self, Kind::Directory)
    }

    /// Return `true` if this is a directory on disk. Note that this is true for repositories as well.
    pub fn is_dir(&self) -> bool {
        matches!(self, Kind::Directory | Kind::Repository)
    }
}
