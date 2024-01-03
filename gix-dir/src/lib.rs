//! A crate for handling a git-style directory walk.
#![deny(missing_docs, rust_2018_idioms)]
#![forbid(unsafe_code)]

use bstr::{BStr, BString};
use std::borrow::Cow;

/// A directory entry, typically obtained using [`walk()`].
#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct EntryRef<'a> {
    /// The repository-relative path at which the file or directory could be found, with unix-style component separators.
    ///
    /// To obtain the respective file, join it with the `worktree_root` passed to [`walk()`].
    /// The rationale here is that this is a compressed and normalized version compared to the paths we would otherwise get,
    /// which is preferable especially when converted to [`Entry`] due to lower memory requirements.
    ///
    /// This also means that the original path to be presented to the user needs to be computed separately, as it's also relative
    /// to their prefix, i.e. their current working directory within the repository.
    ///
    /// Note that this value can be empty if information about the `worktree_root` is provided, which is fine as
    /// [joining](std::path::Path::join) with an empty string is a no-op.
    ///
    /// Note that depending on the way entries are emitted, even refs might already contain an owned `rela_path`, for use with
    /// [into_owned()](EntryRef::into_owned())
    ///
    pub rela_path: Cow<'a, BStr>,
    /// The status of entry, most closely related to what we know from `git status`, but not the same.
    pub status: entry::Status,
    /// Further specify the what the entry is, similar to a file mode.
    pub kind: entry::Kind,
}

/// Just like [`EntryRef`], but with all fields owned (and thus without a lifetime to consider).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct Entry {
    /// See [EntryRef::rela_path] for details.
    pub rela_path: BString,
    /// The status of entry, most closely related to what we know from `git status`, but not the same.
    pub status: entry::Status,
    /// Further specify the what the entry is, similar to a file mode.
    pub kind: entry::Kind,
}

///
pub mod entry;

///
pub mod walk;
pub use walk::function::walk;
