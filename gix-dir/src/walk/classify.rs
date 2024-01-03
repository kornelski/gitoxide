use crate::entry;
use crate::walk::{Context, Error, Options};
use bstr::{BStr, BString, ByteSlice};
use std::ffi::OsStr;
use std::path::{Component, Path, PathBuf};

/// Classify the `worktree_relative_root` path and return the first `PathKind` that indicates that
/// it isn't a directory, leaving `buf` with the path matching the returned `PathKind`,
/// which is at most equal to `worktree_relative_root`.
pub fn root(
    worktree_root: &Path,
    buf: &mut BString,
    worktree_relative_root: &Path,
    options: Options,
    ctx: &mut Context<'_>,
) -> Result<(entry::Status, Option<entry::Kind>), Error> {
    buf.clear();
    let mut last_length = None;
    let mut path_buf = worktree_root.to_owned();
    // These initial values kick in if worktree_relative_root.is_empty();
    let mut file_kind = None;
    let mut file_type = None;
    for component in worktree_relative_root
        .components()
        .chain(if worktree_relative_root.as_os_str().is_empty() {
            Some(Component::Normal(OsStr::new("")))
        } else {
            None
        })
    {
        if last_length.is_some() {
            buf.push(b'/');
        }
        path_buf.push(component);
        buf.extend_from_slice(gix_path::os_str_into_bstr(component.as_os_str()).expect("no illformed UTF8"));
        file_type = path_buf.symlink_metadata().map(|m| m.file_type().into()).ok();

        let res = path(
            &mut path_buf,
            buf,
            last_length.map(|l| l + 1 /* slash */).unwrap_or_default(),
            file_type,
            || None,
            options,
            ctx,
        )?;
        file_kind = Some(res.0);
        file_type = res.1;
        if !res.0.can_recurse(file_type) {
            break;
        }
        last_length = Some(buf.len());
    }
    Ok((file_kind.expect("One iteration of the loop at least"), file_type))
}

/// Figure out what to do with `rela_path`, provided as worktree-relative path, with `disk_file_type` if it is known already
/// as it helps to match pathspecs correctly, which can be different for directories.
/// `path` is a disk-accessible variant of `rela_path` which is within the `worktree_root`, and will be modified temporarily but remain unchanged.
///
/// Note that `rela_path` is used as buffer for convenience, but will be left as is when this function returns.
/// `filename_start_idx` is the index at which the filename begins, i.e. `a/b` has `2` as index.
/// It may resemble a directory on the way to a leaf (like a file)
///
/// Returns `(status, file_kind)` to identify the `status` on disk, along with a classification `file_kind`.
pub fn path(
    path: &mut PathBuf,
    rela_path: &mut BString,
    filename_start_idx: usize,
    disk_file_type: Option<entry::Kind>,
    on_demand_file_type: impl FnOnce() -> Option<entry::Kind>,
    Options {
        ignore_case,
        recurse_repositories,
        ..
    }: Options,
    ctx: &mut Context<'_>,
) -> Result<(entry::Status, Option<entry::Kind>), Error> {
    if is_eq(rela_path[filename_start_idx..].as_bstr(), ".git", ignore_case) {
        return Ok((entry::Status::DotGit, disk_file_type));
    }
    let pathspec_could_match = rela_path.is_empty()
        || ctx
            .pathspec
            .can_match_relative_path(rela_path.as_bstr(), disk_file_type.map(|ft| ft.is_dir()));
    if !pathspec_could_match {
        return Ok((entry::Status::Pruned, disk_file_type));
    }

    let (index_file_type, is_tracked) = resolve_file_type_with_index(rela_path, ctx.index, ignore_case);
    let mut file_type = index_file_type.or(disk_file_type).or_else(on_demand_file_type);

    if let Some(tracked_status) = is_tracked {
        return Ok((tracked_status, file_type));
    }

    if let Some(excluded) = ctx
        .excludes
        .as_mut()
        .map_or(Ok(None), |stack| {
            stack
                .at_entry(rela_path.as_bstr(), file_type.map(|ft| ft.is_dir()), ctx.objects)
                .map(|platform| platform.excluded_kind())
        })
        .map_err(Error::ExcludesAccess)?
    {
        return Ok((entry::Status::Ignored(excluded), file_type));
    }

    debug_assert!(is_tracked.is_none());
    let mut status = entry::Status::Untracked;

    if file_type.map_or(false, |ft| ft.is_dir()) {
        if !recurse_repositories {
            path.push(gix_discover::DOT_GIT_DIR);
            let mut is_nested_nonbare_repo = gix_discover::is_git(path).ok().map_or(false, |kind| {
                matches!(kind, gix_discover::repository::Kind::WorkTree { .. })
            });
            if is_nested_nonbare_repo {
                let git_dir_is_our_own =
                    gix_path::realpath_opts(path, ctx.current_dir, gix_path::realpath::MAX_SYMLINKS)
                        .ok()
                        .map_or(false, |realpath_candidate| realpath_candidate == ctx.git_dir_realpath);
                is_nested_nonbare_repo = !git_dir_is_our_own;
            }
            path.pop();

            if is_nested_nonbare_repo {
                file_type = Some(entry::Kind::Repository);
            }
        }
    } else {
        let pathspec_matches = ctx
            .pathspec
            .pattern_matching_relative_path(
                rela_path.as_bstr(),
                disk_file_type.map(|ft| ft.is_dir()),
                ctx.pathspec_attributes,
            )
            .map_or(false, |m| !m.is_excluded());
        if !pathspec_matches {
            status = entry::Status::Pruned;
        }
    }
    Ok((status, file_type))
}

/// Note that `rela_path` is used as buffer for convenience, but will be left as is when this function returns.
/// Also note `maybe_file_type` will be `None` for entries that aren't up-to-date and files, for directories all entries must be uptodate.
/// Returns `(maybe_file_type, Option(tracked or tracked_excluded)`, while `tracked` indicates either a direct match or an indirect one as
/// sub-entries are matched which are uptodate. `tracked_exclued` indicates it's a sparse directory.
fn resolve_file_type_with_index(
    rela_path: &mut BString,
    index: &gix_index::State,
    ignore_case: bool,
) -> (Option<entry::Kind>, Option<entry::Status>) {
    let mut tracked;
    let file_type = match index.entry_by_path_and_stage_icase(rela_path.as_bstr(), 0, ignore_case) {
        None => {
            rela_path.push(b'/');
            let res = index.prefixed_entries_range_icase(rela_path.as_bstr(), ignore_case);
            rela_path.pop();

            tracked = res.is_some().then_some(entry::Status::Tracked);
            let mut one_index_signalling_with_cone = None;
            let mut all_excluded_from_worktree_non_cone = false;
            let kind = res
                .filter(|range| {
                    if range.len() == 1 {
                        one_index_signalling_with_cone = range.start.into();
                    }
                    let entries = &index.entries()[range.clone()];
                    let all_up_to_date = entries
                        .iter()
                        .all(|e| e.flags.contains(gix_index::entry::Flags::UPTODATE));
                    if !all_up_to_date && one_index_signalling_with_cone.is_none() {
                        all_excluded_from_worktree_non_cone = entries
                            .iter()
                            .all(|e| e.flags.contains(gix_index::entry::Flags::SKIP_WORKTREE));
                    }
                    all_up_to_date
                })
                .map(|_| entry::Kind::Directory);

            if all_excluded_from_worktree_non_cone
                || one_index_signalling_with_cone
                    .filter(|_| kind.is_none())
                    .map_or(false, |idx| index.entries()[idx].mode.is_sparse())
            {
                tracked = Some(entry::Status::TrackedExcluded);
            }
            kind
        }
        Some(entry) => {
            tracked = Some(entry::Status::Tracked);
            if !entry.flags.contains(gix_index::entry::Flags::UPTODATE) {
                None
            } else if entry.mode.is_submodule() {
                entry::Kind::Repository.into()
            } else if entry.mode.contains(gix_index::entry::Mode::FILE) {
                entry::Kind::File.into()
            } else if entry.mode.contains(gix_index::entry::Mode::SYMLINK) {
                entry::Kind::Symlink.into()
            } else {
                None
            }
        }
    };

    (file_type, tracked)
}

fn is_eq(lhs: &BStr, rhs: impl AsRef<BStr>, ignore_case: bool) -> bool {
    if ignore_case {
        lhs.eq_ignore_ascii_case(rhs.as_ref().as_ref())
    } else {
        lhs == rhs.as_ref()
    }
}
