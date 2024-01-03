use bstr::{BStr, BString, ByteSlice};
use std::borrow::Cow;
use std::path::PathBuf;

use crate::entry::Status;
use crate::walk::function::{can_recurse, emit_entry};
use crate::walk::EmissionMode::CollapseDirectory;
use crate::walk::{classify, Action, Context, Delegate, Error, Options, Outcome};
use crate::{entry, walk, Entry};

/// ### Deviation
///
/// Git mostly silently ignores IO errors and stops iterating seemingly quietly, while we error loudly.
#[allow(clippy::too_many_arguments)]
pub fn recursive(
    is_worktree_dir: bool,
    current: &mut PathBuf,
    current_bstr: &mut BString,
    current_status: entry::Status,
    current_kind: entry::Kind,
    ctx: &mut Context<'_>,
    opts: Options,
    delegate: &mut dyn Delegate,
    out: &mut Outcome,
    state: &mut State,
) -> Result<Action, Error> {
    out.read_dir_calls += 1;
    let entries = gix_fs::read_dir(current, opts.precompose_unicode).map_err(|err| Error::ReadDir {
        path: current.to_owned(),
        source: err,
    })?;

    let mut num_entries = 0;
    let mark = state.mark(is_worktree_dir);
    for entry in entries {
        let entry = entry.map_err(|err| Error::DirEntry {
            parent_directory: current.to_owned(),
            source: err,
        })?;
        // Important to count right away, otherwise the directory could be seen as empty even though it's not.
        // That is, this should be independent of the kind.
        num_entries += 1;

        let prev_len = current_bstr.len();
        if prev_len != 0 {
            current_bstr.push(b'/');
        }
        current_bstr.extend_from_slice(
            gix_path::try_os_str_into_bstr(entry.file_name())
                .expect("no illformed UTF-8")
                .as_ref(),
        );
        current.push(entry.file_name());

        let (status, kind) = classify::path(
            current,
            current_bstr,
            if prev_len == 0 { 0 } else { prev_len + 1 },
            None,
            || entry.file_type().ok().map(Into::into),
            opts,
            ctx,
        )?;

        if can_recurse(current_bstr.as_bstr(), status, kind, delegate) {
            let action = recursive(
                false,
                current,
                current_bstr,
                status,
                kind.expect("it's clear by onw"),
                ctx,
                opts,
                delegate,
                out,
                state,
            )?;
            if action != Action::Continue {
                break;
            }
        } else if !state.held_for_directory_collapse(current_bstr.as_bstr(), status, kind, &opts) {
            let action = emit_entry(
                Cow::Borrowed(current_bstr.as_bstr()),
                status,
                None,
                kind,
                opts,
                out,
                delegate,
            );
            if action != Action::Continue {
                return Ok(action);
            }
        }
        current_bstr.truncate(prev_len);
        current.pop();
    }

    Ok(mark.reduce_held_entries(
        num_entries,
        state,
        current_bstr.as_bstr(),
        current_status,
        current_kind,
        opts,
        out,
        delegate,
    ))
}

#[derive(Default)]
pub(super) struct State {
    /// The entries to hold back until it's clear what to do with them.
    pub on_hold: Vec<Entry>,
}

impl State {
    /// Hold the entry with the given `status` if it's a candidate for collapsing the containing directory.
    fn held_for_directory_collapse(
        &mut self,
        rela_path: &BStr,
        status: entry::Status,
        kind: Option<entry::Kind>,
        opts: &Options,
    ) -> bool {
        let kind = match kind {
            Some(kind) => kind,
            None => {
                // NOTE: this can be a `.git` directory or file, and we don't get the file type for it.
                return false;
            }
        };

        if opts.should_hold(status) {
            self.on_hold.push(Entry {
                rela_path: rela_path.to_owned(),
                status,
                kind,
            });
            true
        } else {
            false
        }
    }

    /// Keep track of state we need to later resolve the state.
    /// Worktree directories are special, as they don't fold.
    fn mark(&self, is_worktree_dir: bool) -> Mark {
        Mark {
            start_index: self.on_hold.len(),
            is_worktree_dir,
        }
    }
}

struct Mark {
    start_index: usize,
    is_worktree_dir: bool,
}

impl Mark {
    #[allow(clippy::too_many_arguments)]
    fn reduce_held_entries(
        mut self,
        num_entries: usize,
        state: &mut State,
        dir_rela_path: &BStr,
        dir_status: entry::Status,
        dir_kind: entry::Kind,
        opts: Options,
        out: &mut walk::Outcome,
        delegate: &mut dyn walk::Delegate,
    ) -> walk::Action {
        if num_entries == 0 {
            emit_entry(
                Cow::Borrowed(dir_rela_path),
                dir_status,
                None,
                Some(if num_entries == 0 {
                    assert_ne!(
                        dir_kind,
                        entry::Kind::Repository,
                        "BUG: it shouldn't be possible to classify an empty dir as repository"
                    );
                    entry::Kind::EmptyDirectory
                } else {
                    dir_kind
                }),
                opts,
                out,
                delegate,
            )
        } else if let Some(action) = self.try_collapse(dir_rela_path, dir_kind, state, out, opts, delegate) {
            action
        } else {
            self.emit_all_held(state, opts, out, delegate)
        }
    }

    fn emit_all_held(
        &mut self,
        state: &mut State,
        opts: Options,
        out: &mut walk::Outcome,
        delegate: &mut dyn walk::Delegate,
    ) -> Action {
        for entry in state.on_hold.drain(self.start_index..) {
            let action = emit_entry(
                Cow::Owned(entry.rela_path),
                entry.status,
                None,
                Some(entry.kind),
                opts,
                out,
                delegate,
            );
            if action != Action::Continue {
                return action;
            }
        }
        Action::Continue
    }

    #[allow(clippy::too_many_arguments)]
    fn try_collapse(
        &self,
        dir_rela_path: &BStr,
        dir_kind: entry::Kind,
        state: &mut State,
        out: &mut walk::Outcome,
        opts: Options,
        delegate: &mut dyn walk::Delegate,
    ) -> Option<Action> {
        if self.is_worktree_dir {
            return None;
        }
        let (mut expendable, mut precious, mut untracked, mut entries) = (0, 0, 0, 0);
        for status in state.on_hold[self.start_index..].iter().map(|e| e.status) {
            entries += 1;
            match status {
                Status::DotGit | Status::Pruned | Status::TrackedExcluded => {
                    unreachable!("pruned aren't held")
                }
                Status::Tracked => { /* causes the folder not to be collapsed */ }
                Status::Ignored(gix_ignore::Kind::Expendable) => expendable += 1,
                Status::Ignored(gix_ignore::Kind::Precious) => precious += 1,
                Status::Untracked => untracked += 1,
            }
        }

        let dir_status = if opts.emit_untracked == CollapseDirectory
            && untracked != 0
            && untracked + expendable + precious == entries
            && (!opts.collapse_is_for_deletion || precious == 0)
        {
            entry::Status::Untracked
        } else if opts.emit_ignored == Some(CollapseDirectory) {
            if expendable != 0 && expendable == entries {
                entry::Status::Ignored(gix_ignore::Kind::Expendable)
            } else if precious != 0 && precious == entries {
                entry::Status::Ignored(gix_ignore::Kind::Precious)
            } else {
                return None;
            }
        } else {
            return None;
        };

        if !matches!(dir_status, entry::Status::Untracked | entry::Status::Ignored(_)) {
            return None;
        }

        let mut removed_without_counting = 0;
        let mut action = Action::Continue;
        for entry in state.on_hold.drain(self.start_index..) {
            if entry.status != dir_status && action == Action::Continue {
                action = emit_entry(
                    Cow::Owned(entry.rela_path),
                    entry.status,
                    Some(dir_status),
                    entry.kind.into(),
                    opts,
                    out,
                    delegate,
                );
            } else {
                removed_without_counting += 1;
            };
        }
        out.seen_entries += removed_without_counting as u32;

        state.on_hold.push(Entry {
            rela_path: dir_rela_path.to_owned(),
            status: dir_status,
            kind: dir_kind,
        });
        Some(action)
    }
}

impl Options {
    fn should_hold(&self, status: entry::Status) -> bool {
        if status.is_pruned() {
            return false;
        }
        self.emit_ignored == Some(CollapseDirectory) || self.emit_untracked == CollapseDirectory
    }
}
