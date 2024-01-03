use crate::{entry, EntryRef};
use bstr::BStr;
use std::path::PathBuf;

/// A type returned by the [`Delegate::emit()`] as passed to [`walk()`](function::walk()).
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[must_use]
pub enum Action {
    /// Continue the traversal as normal.
    Continue,
    /// Do not continue the traversal, but exit it.
    Cancel,
}

/// A way for the caller to control the traversal based on provided data.
pub trait Delegate {
    /// Called for each observed `entry` *inside* a directory, or the directory itself if the traversal is configured
    /// to simplify the result (i.e. if every file in a directory is ignored, emit the containing directory instead
    /// of each file), or if the root of the traversal passes through a directory that can't be traversed.
    ///
    /// It will also be called if the `root` in [`walk()`](crate::walk()) itself is matching a particular status,
    /// even if it is a file.
    ///
    /// Note that tracked entries will only be emitted if [`Options::emit_tracked`] is `true`.
    /// Further, not all pruned entries will be observable as they might be pruned so early that the kind of
    /// item isn't yet known. Pruned entries are also only emitted if [`Options::emit_pruned`] is `true`.
    ///
    /// `collapsed_directory_status` is `Some(dir_status)` if this entry was part of a directory with the given
    /// `dir_status` that wasn't the same as the one of `entry`. Depending on the operation, these then want to be
    /// used or discarded.
    fn emit(&mut self, entry: EntryRef<'_>, collapsed_directory_status: Option<entry::Status>) -> Action;

    /// Return `true` if the given entry can be recursed into. Will only be called if the entry is a physical directory.
    /// The base implementation will act like git does by default in `git status`.
    ///
    /// Note that this method will see all directories, even though not all of them may end up being [emitted](Self::emit()).
    /// If this method returns `false`, the `entry` will always be emitted.
    fn can_recurse(&mut self, entry: EntryRef<'_>) -> bool {
        entry.status.can_recurse(Some(entry.kind))
    }
}

/// The way entries are emitted using the [Delegate].
///
/// The choice here controls if entries are emitted immediately, or have to be held back.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum EmissionMode {
    /// Emit each entry as it matches exactly, without doing any kind of simplification.
    ///
    /// Emissions in this mode are happening as they occour, without any buffering or ordering.
    #[default]
    Matching,
    /// Emit only a containing directory if all of its entries are of the same type.
    ///
    /// Note that doing so is more expensive as it requires us to keep track of all entries in the directory structure
    /// until it's clear what to finally emit.
    CollapseDirectory,
}

/// Options for use in [`walk()`](function::walk()) function.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct Options {
    /// If true, the filesystem will store paths as decomposed unicode, i.e. `ä` becomes `"a\u{308}"`, which means that
    /// we have to turn these forms back from decomposed to precomposed unicode before storing it in the index or generally
    /// using it. This also applies to input received from the command-line, so callers may have to be aware of this and
    /// perform conversions accordingly.
    /// If false, no conversions will be performed.
    pub precompose_unicode: bool,
    /// If true, the filesystem ignores the case of input, which makes `A` the same file as `a`.
    /// This is also called case-folding.
    /// Note that [pathspecs](Context::pathspec) must also be using the same defaults, which makes them match case-insensitive
    /// automatically.
    pub ignore_case: bool,
    /// If `true`, we will stop figuring out if any directory that is a candidate for recursion is also a nested repository,
    /// which saves time but leads to recurse into it. If `false`, nested repositories will not be traversed.
    pub recurse_repositories: bool,
    /// If `true`, entries that are pruned and whose [Kind](crate::entry::Kind) is known will be emitted.
    pub emit_pruned: bool,
    /// If `Some(mode)`, entries that are ignored will be emitted according to the given `mode`.
    /// If `None`, ignored entries will not be emitted at all.
    pub emit_ignored: Option<EmissionMode>,
    /// When directories are meant to be for mutation or deletion, this must be `true` to assure we don't collapse
    /// directories that have precious files in them. That way, these can't be accidentally deleted as they are contained
    /// in a now collapsed folder.
    /// If `false`, precious files are treated like expendable files, which is usually what you want when displaying them
    /// for addition to the repository.
    pub collapse_is_for_deletion: bool,
    /// If `true`, we will also emit entries for tracked items. Otherwise these will remain 'hidden', even if a pathspec directly
    /// refers to it.
    pub emit_tracked: bool,
    /// Controls the way untracked files are emitted. By default, this is happening immediately and without any simplification.
    pub emit_untracked: EmissionMode,
    /// If `true`, emit empty directories as well. Note that a directory also counts as empty if it has any amount or depth of nested
    /// subdirectories, as long as none of them includes a file.
    /// Thus, this makes leaf-level empty directories visible, as those don't have any content.
    pub emit_empty_directories: bool,
}

/// All information that is required to perform a dirwalk, and classify paths properly.
pub struct Context<'a> {
    /// The `git_dir` of the parent repository, after a call to [`gix_path::realpath()`].
    ///
    /// It's used to help us differentiate our own `.git` directory from nested unrelated repositories,
    /// which is needed if `core.worktree` is used to nest the `.git` directory deeper within.
    pub git_dir_realpath: &'a std::path::Path,
    /// The current working directory as returned by `gix_fs::current_dir()` to assure it respects `core.precomposeUnicode`.
    /// It's used to produce the realpath of the git-dir of a repository candidate to assure it's not our own repository.
    pub current_dir: &'a std::path::Path,
    /// The index to quickly understand if a file or directory is tracked or not.
    ///
    /// ### Important
    ///
    /// The index must have been validated so that each entry that is considered up-to-date will have the [gix_index::entry::Flags::UPTODATE] flag
    /// set. Otherwise the index entry is not considered and a disk-access may occour which is costly.
    pub index: &'a gix_index::State,
    /// A pathspec to use as filter - we only traverse into directories if it matches.
    /// Note that the `ignore_case` setting it uses should match our [Options::ignore_case].
    /// If no such filtering is desired, pass an empty `pathspec` which will match everything.
    pub pathspec: &'a mut gix_pathspec::Search,
    /// The `attributes` callback for use in [gix_pathspec::Search::pattern_matching_relative_path()], which happens when
    /// pathspecs use attributes for filtering.
    /// If `pathspec` isn't empty, this function may be called if pathspecs perform attribute lookups.
    pub pathspec_attributes: &'a mut dyn FnMut(
        &BStr,
        gix_pathspec::attributes::glob::pattern::Case,
        bool,
        &mut gix_pathspec::attributes::search::Outcome,
    ) -> bool,
    /// A way to query the `.gitignore` files to see if a directory or file is ignored.
    /// Set to `None` to not perform any work on checking for ignored, which turns previously ignored files into untracked ones, a useful
    /// operation when trying to add ignored files to a repository.
    pub excludes: Option<&'a mut gix_worktree::Stack>,
    /// Access to the object database for use with `excludes` - it's possible to access `.gitignore` files in the index if configured.
    pub objects: &'a dyn gix_object::Find,
}

/// Additional information collected as outcome of [`walk()`](function::walk()).
#[derive(Default, Debug, Clone, Copy, Ord, PartialOrd, Eq, PartialEq)]
pub struct Outcome {
    /// The amount of calls to read the directory contents.
    pub read_dir_calls: u32,
    /// The amount of returned entries provided to the callback. This number can be lower than `seen_entries`.
    pub returned_entries: usize,
    /// The amount of entries, prior to pathspecs filtering them out or otherwise excluding them.
    pub seen_entries: u32,
}

/// The error returned by [`walk()`](function::walk()).
#[derive(Debug, thiserror::Error)]
#[allow(missing_docs)]
pub enum Error {
    #[error("Worktree root at '{}' is not a directory", root.display())]
    WorktreeRootIsFile { root: PathBuf },
    #[error("Traversal root '{}' contains relative path components and could not be normalized", root.display())]
    NormalizeRoot { root: PathBuf },
    #[error("Traversal root '{}' must be literally contained in worktree root '{}'", root.display(), worktree_root.display())]
    RootNotInWorktree { root: PathBuf, worktree_root: PathBuf },
    #[error("A symlink was found at component {component_index} of traversal root '{}' as seen from worktree root '{}'", root.display(), worktree_root.display())]
    SymlinkInRoot {
        root: PathBuf,
        worktree_root: PathBuf,
        /// This index starts at 0, with 0 being the first component.
        component_index: usize,
    },
    #[error("Failed to update the excludes stack to see if a path is excluded")]
    ExcludesAccess(std::io::Error),
    #[error("Failed read the directory at '{}'", path.display())]
    ReadDir { path: PathBuf, source: std::io::Error },
    #[error("Could not obtain directory entry in root of '{}'", parent_directory.display())]
    DirEntry {
        parent_directory: PathBuf,
        source: std::io::Error,
    },
    #[error("Could not obtain filetype of directory entry '{}'", path.display())]
    DirEntryFileType { path: PathBuf, source: std::io::Error },
    #[error("Could not obtain symlink metadata on '{}'", path.display())]
    SymlinkMetadata { path: PathBuf, source: std::io::Error },
}

mod classify;
pub(crate) mod function;
mod readdir;
