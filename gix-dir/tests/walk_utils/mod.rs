use bstr::BStr;
use gix_dir::walk::Action;
use gix_dir::{entry, walk, Entry, EntryRef};
use gix_testtools::scripted_fixture_read_only;
use std::path::{Path, PathBuf};

pub fn fixture_in(filename: &str, name: &str) -> PathBuf {
    let root = scripted_fixture_read_only(format!("{filename}.sh")).expect("script works");
    root.join(name)
}

pub fn fixture(name: &str) -> PathBuf {
    fixture_in("many", name)
}

/// Default options
pub fn options() -> walk::Options {
    walk::Options::default()
}

/// Default options
pub fn options_emit_all() -> walk::Options {
    walk::Options {
        precompose_unicode: false,
        ignore_case: false,
        recurse_repositories: false,
        collapse_is_for_deletion: false,
        emit_pruned: true,
        emit_ignored: Some(walk::EmissionMode::Matching),
        emit_tracked: true,
        emit_untracked: walk::EmissionMode::Matching,
        emit_empty_directories: true,
    }
}

pub fn entry(
    rela_path: impl AsRef<BStr>,
    kind: gix_dir::entry::Status,
    mode: gix_dir::entry::Kind,
) -> (Entry, Option<entry::Status>) {
    (
        Entry {
            rela_path: rela_path.as_ref().to_owned(),
            status: kind,
            kind: mode,
        },
        None,
    )
}

pub fn entry_dirstat(
    rela_path: impl AsRef<BStr>,
    kind: gix_dir::entry::Status,
    mode: gix_dir::entry::Kind,
    dir_status: gix_dir::entry::Status,
) -> (Entry, Option<entry::Status>) {
    (
        Entry {
            rela_path: rela_path.as_ref().to_owned(),
            status: kind,
            kind: mode,
        },
        Some(dir_status),
    )
}

pub fn collect(
    worktree_root: &Path,
    cb: impl FnOnce(&mut dyn walk::Delegate, walk::Context) -> Result<walk::Outcome, walk::Error>,
) -> (walk::Outcome, Entries) {
    try_collect(worktree_root, cb).unwrap()
}

pub fn collect_filtered(
    worktree_root: &Path,
    cb: impl FnOnce(&mut dyn walk::Delegate, walk::Context) -> Result<walk::Outcome, walk::Error>,
    patterns: impl IntoIterator<Item = impl AsRef<BStr>>,
) -> (walk::Outcome, Entries) {
    try_collect_filtered(worktree_root, cb, patterns).unwrap()
}

pub fn try_collect(
    worktree_root: &Path,
    cb: impl FnOnce(&mut dyn walk::Delegate, walk::Context) -> Result<walk::Outcome, walk::Error>,
) -> Result<(walk::Outcome, Entries), walk::Error> {
    try_collect_filtered(worktree_root, cb, None::<&str>)
}

pub fn try_collect_filtered(
    worktree_root: &Path,
    cb: impl FnOnce(&mut dyn walk::Delegate, walk::Context) -> Result<walk::Outcome, walk::Error>,
    patterns: impl IntoIterator<Item = impl AsRef<BStr>>,
) -> Result<(walk::Outcome, Entries), walk::Error> {
    try_collect_filtered_opts(worktree_root, cb, patterns, None)
}

pub fn try_collect_filtered_opts(
    worktree_root: &Path,
    cb: impl FnOnce(&mut dyn walk::Delegate, walk::Context) -> Result<walk::Outcome, walk::Error>,
    patterns: impl IntoIterator<Item = impl AsRef<BStr>>,
    git_dir: Option<&str>,
) -> Result<(walk::Outcome, Entries), walk::Error> {
    let git_dir = worktree_root.join(git_dir.unwrap_or(".git"));
    let mut index = std::fs::read(git_dir.join("index")).ok().map_or_else(
        || gix_index::State::new(gix_index::hash::Kind::Sha1),
        |bytes| {
            gix_index::State::from_bytes(
                &bytes,
                std::time::UNIX_EPOCH.into(),
                gix_index::hash::Kind::Sha1,
                Default::default(),
            )
            .map(|t| t.0)
            .expect("valid index")
        },
    );
    index
        .entries_mut()
        .iter_mut()
        .filter(|e| {
            // relevant for partial checkouts, all related entries will have skip-worktree set,
            // which also means they will never be up-to-date.
            !e.flags.contains(gix_index::entry::Flags::SKIP_WORKTREE)
        })
        .for_each(|e| {
            // pretend that the index was refreshed beforehand so we know what's uptodate.
            e.flags |= gix_index::entry::Flags::UPTODATE;
        });
    let mut search = gix_pathspec::Search::from_specs(
        patterns.into_iter().map(|spec| {
            gix_pathspec::parse(spec.as_ref(), gix_pathspec::Defaults::default()).expect("tests use valid pattern")
        }),
        None,
        "we don't provide absolute pathspecs, thus need no worktree root".as_ref(),
    )
    .expect("search creation can't fail");
    let mut stack = gix_worktree::Stack::from_state_and_ignore_case(
        worktree_root,
        false, /* ignore case */
        gix_worktree::stack::State::IgnoreStack(gix_worktree::stack::state::Ignore::new(
            Default::default(),
            Default::default(),
            None,
            gix_worktree::stack::state::ignore::Source::WorktreeThenIdMappingIfNotSkipped,
        )),
        &index,
        index.path_backing(),
    );

    let cwd = gix_fs::current_dir(false).expect("valid cwd");
    let git_dir_realpath = gix_path::realpath_opts(&git_dir, &cwd, gix_path::realpath::MAX_SYMLINKS).unwrap();
    let mut dlg = Collect::default();
    let outcome = cb(
        &mut dlg,
        walk::Context {
            git_dir_realpath: &git_dir_realpath,
            current_dir: &cwd,
            index: &index,
            pathspec: &mut search,
            pathspec_attributes: &mut |_, _, _, _| panic!("we do not use pathspecs that require attributes access."),
            excludes: Some(&mut stack),
            objects: &gix_object::find::Never,
        },
    )?;

    Ok((outcome, dlg.into_entries()))
}

type Entries = Vec<(Entry, Option<entry::Status>)>;

#[derive(Default)]
struct Collect {
    entries: Entries,
}

impl Collect {
    fn into_entries(mut self) -> Entries {
        self.entries.sort_by(|a, b| a.0.rela_path.cmp(&b.0.rela_path));
        self.entries
    }
}

impl walk::Delegate for Collect {
    fn emit(&mut self, entry: EntryRef<'_>, dir_status: Option<entry::Status>) -> Action {
        self.entries.push((entry.to_owned(), dir_status));
        walk::Action::Continue
    }
}
