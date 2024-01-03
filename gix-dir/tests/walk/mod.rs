use gix_dir::walk;
use gix_testtools::scripted_fixture_read_only;

use crate::walk_utils::{
    collect, collect_filtered, entry, entry_dirstat, fixture, fixture_in, options, options_emit_all, try_collect,
    try_collect_filtered_opts,
};
use gix_dir::entry::Kind::*;
use gix_dir::entry::Status::*;
use gix_dir::walk::EmissionMode::*;
use gix_ignore::Kind::*;

mod baseline {
    use std::path::Path;

    /// Parse multiple walks out of a single `fixture`.
    pub fn extract_walks(_fixture: &Path) -> crate::Result {
        Ok(())
    }
}

#[test]
#[ignore = "needs assertions and a way to match options"]
fn baseline() -> crate::Result {
    baseline::extract_walks(&scripted_fixture_read_only("walk_baseline.sh")?)?;
    Ok(())
}

#[test]
#[cfg_attr(windows, ignore = "symlinks the way they are organized don't yet work on windows")]
fn root_may_not_lead_through_symlinks() -> crate::Result {
    for (name, intermediate, expected) in [
        ("immediate-breakout-symlink", "", 0),
        ("breakout-symlink", "hide", 1),
        ("breakout-symlink", "hide/../hide", 1),
    ] {
        let root = fixture_in("many-symlinks", name);
        let err = try_collect(&root, |keep, ctx| {
            walk(&root.join(intermediate).join("breakout"), &root, ctx, options(), keep)
        })
        .unwrap_err();
        assert!(
            matches!(err, walk::Error::SymlinkInRoot { component_index, .. } if component_index == expected),
            "{name} should have component {expected}"
        );
    }
    Ok(())
}

#[test]
fn empty_root() -> crate::Result {
    let root = fixture("empty");
    let (out, entries) = collect(&root, |keep, ctx| walk(&root, &root, ctx, options(), keep));
    assert_eq!(
        out,
        walk::Outcome {
            read_dir_calls: 1,
            returned_entries: entries.len(),
            seen_entries: 1,
        }
    );
    assert_eq!(
        entries.len(),
        0,
        "by default, nothing is shown as the directory is empty"
    );

    let (out, entries) = collect(&root, |keep, ctx| {
        walk(
            &root,
            &root,
            ctx,
            walk::Options {
                emit_empty_directories: true,
                ..options()
            },
            keep,
        )
    });
    assert_eq!(
        out,
        walk::Outcome {
            read_dir_calls: 1,
            returned_entries: entries.len(),
            seen_entries: 1,
        }
    );
    assert_eq!(entries.len(), 1);
    assert_eq!(
        &entries[0],
        &entry("", Untracked, EmptyDirectory),
        "this is how we can indicate the worktree is entirely untracked"
    );
    Ok(())
}

#[test]
fn complex_empty() -> crate::Result {
    let root = fixture("complex-empty");
    let (out, entries) = collect(&root, |keep, ctx| walk(&root, &root, ctx, options_emit_all(), keep));
    assert_eq!(
        out,
        walk::Outcome {
            read_dir_calls: 9,
            returned_entries: entries.len(),
            seen_entries: 5,
        }
    );
    assert_eq!(
        entries,
        &[
            entry("dirs-and-files/dir/file", Untracked, File),
            entry("dirs-and-files/sub", Untracked, EmptyDirectory),
            entry("empty-toplevel", Untracked, EmptyDirectory),
            entry("only-dirs/other", Untracked, EmptyDirectory),
            entry("only-dirs/sub/subsub", Untracked, EmptyDirectory),
        ],
        "we see each and every directory, and get it classified as empty as it's set to be emitted"
    );

    let (out, entries) = collect(&root, |keep, ctx| {
        walk(
            &root,
            &root,
            ctx,
            walk::Options {
                emit_empty_directories: false,
                ..options()
            },
            keep,
        )
    });
    assert_eq!(
        out,
        walk::Outcome {
            read_dir_calls: 9,
            returned_entries: entries.len(),
            seen_entries: 5,
        }
    );
    assert_eq!(
        entries,
        &[entry("dirs-and-files/dir/file", Untracked, File),],
        "by default, no empty directory shows up"
    );
    Ok(())
}

#[test]
fn only_untracked() -> crate::Result {
    let root = fixture("only-untracked");
    let (out, entries) = collect(&root, |keep, ctx| walk(&root, &root, ctx, options(), keep));
    assert_eq!(
        out,
        walk::Outcome {
            read_dir_calls: 3,
            returned_entries: entries.len(),
            seen_entries: 7,
        }
    );
    assert_eq!(
        &entries,
        &[
            entry("a", Untracked, File),
            entry("b", Untracked, File),
            entry("c", Untracked, File),
            entry("d/a", Untracked, File),
            entry("d/b", Untracked, File),
            entry("d/d/a", Untracked, File),
        ]
    );

    let (out, entries) = collect_filtered(&root, |keep, ctx| walk(&root, &root, ctx, options(), keep), Some("d/*"));
    assert_eq!(
        out,
        walk::Outcome {
            read_dir_calls: 3,
            returned_entries: entries.len(),
            seen_entries: 7,
        }
    );
    assert_eq!(
        &entries,
        &[
            entry("d/a", Untracked, File),
            entry("d/b", Untracked, File),
            entry("d/d/a", Untracked, File),
        ]
    );

    let (out, entries) = collect(&root, |keep, ctx| {
        walk(
            &root,
            &root,
            ctx,
            walk::Options {
                emit_untracked: CollapseDirectory,
                ..options()
            },
            keep,
        )
    });
    assert_eq!(
        out,
        walk::Outcome {
            read_dir_calls: 3,
            returned_entries: entries.len(),
            seen_entries: 7 + 2,
        },
        "There are 2 extra directories that we fold into, but ultimately discard"
    );
    assert_eq!(
        &entries,
        &[
            entry("a", Untracked, File),
            entry("b", Untracked, File),
            entry("c", Untracked, File),
            entry("d", Untracked, Directory),
        ]
    );
    Ok(())
}

#[test]
#[ignore = "TBD"]
fn only_untracked_explicit_pathspec_selection() -> crate::Result {
    let root = fixture("only-untracked");
    let (out, entries) = collect_filtered(
        &root,
        |keep, ctx| {
            walk(
                &root,
                &root,
                ctx,
                walk::Options {
                    emit_untracked: Matching,
                    ..options()
                },
                keep,
            )
        },
        ["d/a", "d/d/a"],
    );
    assert_eq!(
        out,
        walk::Outcome {
            read_dir_calls: 3,
            returned_entries: entries.len(),
            seen_entries: 7,
        },
    );
    assert_eq!(
        &entries,
        &[entry("d/a", Untracked, File), entry("d/d/a", Untracked, File)],
        "this works just like expected, as nothing is collapsed anyway"
    );

    let (out, entries) = collect_filtered(
        &root,
        |keep, ctx| {
            walk(
                &root,
                &root,
                ctx,
                walk::Options {
                    emit_untracked: CollapseDirectory,
                    ..options()
                },
                keep,
            )
        },
        ["d/a", "d/d/a"],
    );
    assert_eq!(
        out,
        walk::Outcome {
            read_dir_calls: 3,
            returned_entries: entries.len(),
            seen_entries: 7 + 2,
        },
        "There are 2 extra directories that we fold into, but ultimately discard"
    );
    assert_eq!(
        &entries,
        &[entry("d/a", Untracked, File), entry("d/d/a", Untracked, File)],
        "we actually want to mention the entries that matched the pathspec precisely, so two of them would be needed here\
        while preventing the directory collapse from happening"
    );
    Ok(())
}

#[test]
fn expendable_and_precious() {
    let root = fixture("expendable-and-precious");
    let (out, entries) = collect(&root, |keep, ctx| walk(&root, &root, ctx, options_emit_all(), keep));
    assert_eq!(
        out,
        walk::Outcome {
            read_dir_calls: 6,
            returned_entries: entries.len(),
            seen_entries: 18,
        }
    );
    assert_eq!(
        &entries,
        &[
            entry(".gitignore", Tracked, File),
            entry("a.o", Ignored(Expendable), File),
            entry("all-expendable", Ignored(Expendable), Directory),
            entry("all-expendable-by-filematch/e.o", Ignored(Expendable), File),
            entry("all-expendable-by-filematch/f.o", Ignored(Expendable), File),
            entry("all-precious", Ignored(Precious), Directory),
            entry("all-precious-by-filematch/a.precious", Ignored(Precious), File),
            entry("all-precious-by-filematch/b.precious", Ignored(Precious), File),
            entry("mixed/b.o", Ignored(Expendable), File),
            entry("mixed/precious", Ignored(Precious), File),
            entry("precious", Ignored(Precious), File),
            entry("some-expendable/file", Tracked, File),
            entry("some-expendable/file.o", Ignored(Expendable), File),
            entry("some-expendable/new", Untracked, File),
            entry("some-precious/file", Tracked, File),
            entry("some-precious/file.precious", Ignored(Precious), File),
            entry("some-precious/new", Untracked, File),
        ],
        "listing everything is a 'matching' preset, which is among the most efficient."
    );

    let (out, entries) = collect(&root, |keep, ctx| {
        walk(
            &root,
            &root,
            ctx,
            walk::Options {
                emit_ignored: Some(CollapseDirectory),
                emit_tracked: true,
                ..options()
            },
            keep,
        )
    });
    assert_eq!(
        out,
        walk::Outcome {
            read_dir_calls: 6,
            returned_entries: entries.len(),
            seen_entries: 18 + 2,
        }
    );

    assert_eq!(
        &entries,
        &[
            entry(".gitignore", Tracked, File),
            entry("a.o", Ignored(Expendable), File),
            entry("all-expendable", Ignored(Expendable), Directory),
            entry("all-expendable-by-filematch", Ignored(Expendable), Directory),
            entry("all-precious", Ignored(Precious), Directory),
            entry("all-precious-by-filematch", Ignored(Precious), Directory),
            entry("mixed/b.o", Ignored(Expendable), File),
            entry("mixed/precious", Ignored(Precious), File),
            entry("precious", Ignored(Precious), File),
            entry("some-expendable/file", Tracked, File),
            entry("some-expendable/file.o", Ignored(Expendable), File),
            entry("some-expendable/new", Untracked, File),
            entry("some-precious/file", Tracked, File),
            entry("some-precious/file.precious", Ignored(Precious), File),
            entry("some-precious/new", Untracked, File),
        ],
        "those that have tracked and ignored won't be collapsed, nor will be folders that have mixed precious and ignored files,\
        those with all files of one type will be collapsed though"
    );

    let (out, entries) = collect(&root, |keep, ctx| {
        walk(
            &root,
            &root,
            ctx,
            walk::Options {
                emit_ignored: None,
                emit_untracked: CollapseDirectory,
                emit_tracked: false,
                ..options()
            },
            keep,
        )
    });
    assert_eq!(
        out,
        walk::Outcome {
            read_dir_calls: 6,
            returned_entries: entries.len(),
            seen_entries: 16 + 2,
        }
    );

    assert_eq!(
        &entries,
        &[
            entry("some-expendable/new", Untracked, File),
            entry("some-precious/new", Untracked, File),
        ],
        "even with collapsing, once there is a tracked file in the directory, we show the untracked file directly"
    );
}

#[test]
fn subdir_untracked() -> crate::Result {
    let root = fixture("subdir-untracked");
    let (out, entries) = collect(&root, |keep, ctx| walk(&root, &root, ctx, options(), keep));
    assert_eq!(
        out,
        walk::Outcome {
            read_dir_calls: 3,
            returned_entries: entries.len(),
            seen_entries: 7,
        }
    );
    assert_eq!(&entries, &[entry("d/d/a", Untracked, File)]);

    let (out, entries) = collect_filtered(
        &root,
        |keep, ctx| walk(&root, &root, ctx, options(), keep),
        Some("d/d/*"),
    );
    assert_eq!(
        out,
        walk::Outcome {
            read_dir_calls: 3,
            returned_entries: entries.len(),
            seen_entries: 7,
        },
        "pruning has no actual effect here as there is no extra directories that could be avoided"
    );
    assert_eq!(&entries, &[entry("d/d/a", Untracked, File)]);

    let (out, entries) = collect(&root, |keep, ctx| {
        walk(
            &root,
            &root,
            ctx,
            walk::Options {
                emit_untracked: CollapseDirectory,
                ..options()
            },
            keep,
        )
    });
    assert_eq!(
        out,
        walk::Outcome {
            read_dir_calls: 3,
            returned_entries: entries.len(),
            seen_entries: 7 + 1,
        },
        "there is a folded directory we added"
    );
    assert_eq!(&entries, &[entry("d/d", Untracked, Directory)]);
    Ok(())
}

#[test]
fn only_untracked_from_subdir() -> crate::Result {
    let root = fixture("only-untracked");
    let (out, entries) = collect(&root, |keep, ctx| {
        walk(&root.join("d").join("d"), &root, ctx, options(), keep)
    });
    assert_eq!(
        out,
        walk::Outcome {
            read_dir_calls: 1,
            returned_entries: entries.len(),
            seen_entries: 1,
        }
    );
    assert_eq!(
        &entries,
        &[entry("d/d/a", Untracked, File)],
        "even from subdirs, paths are worktree relative"
    );
    Ok(())
}

#[test]
fn untracked_and_ignored() -> crate::Result {
    let root = fixture("subdir-untracked-and-ignored");
    let (out, entries) = collect(&root, |keep, ctx| {
        walk(
            &root,
            &root,
            ctx,
            walk::Options {
                emit_ignored: Some(Matching),
                ..options()
            },
            keep,
        )
    });
    assert_eq!(
        out,
        walk::Outcome {
            read_dir_calls: 5,
            returned_entries: entries.len(),
            seen_entries: 21,
        },
        "some untracked ones are hidden by default"
    );
    assert_eq!(
        &entries,
        &[
            entry(".gitignore", Untracked, File),
            entry("a.o", Ignored(Expendable), File),
            entry("b.o", Ignored(Expendable), File),
            entry("c.o", Ignored(Expendable), File),
            entry("d/a.o", Ignored(Expendable), File),
            entry("d/b.o", Ignored(Expendable), File),
            entry("d/d/a", Untracked, File),
            entry("d/d/a.o", Ignored(Expendable), File),
            entry("d/d/b.o", Ignored(Expendable), File),
            entry("d/d/generated", Ignored(Expendable), Directory),
            entry("d/generated", Ignored(Expendable), Directory),
            entry("generated", Ignored(Expendable), Directory),
            entry("objs/a.o", Ignored(Expendable), File),
            entry("objs/b.o", Ignored(Expendable), File),
            entry("objs/sub/other.o", Ignored(Expendable), File),
        ]
    );

    let (out, entries) = collect_filtered(
        &root,
        |keep, ctx| {
            walk(
                &root,
                &root,
                ctx,
                walk::Options {
                    emit_pruned: true,
                    ..options()
                },
                keep,
            )
        },
        Some("**/a*"),
    );
    assert_eq!(
        out,
        walk::Outcome {
            read_dir_calls: 5,
            returned_entries: entries.len(),
            seen_entries: 21,
        },
        "basically the same result…"
    );

    assert_eq!(
        &entries,
        &[entry(".gitignore", Pruned, File), entry("d/d/a", Untracked, File),],
        "…but with different classification as the ignore file is pruned so it's not untracked anymore"
    );

    let (out, entries) = collect(&root, |keep, ctx| {
        walk(
            &root,
            &root,
            ctx,
            walk::Options {
                emit_ignored: None,
                emit_untracked: CollapseDirectory,
                ..options()
            },
            keep,
        )
    });
    assert_eq!(
        out,
        walk::Outcome {
            read_dir_calls: 5,
            returned_entries: entries.len(),
            seen_entries: 21 + 1,
        },
        "we still encounter the same amount of entries, and 1 folded directory"
    );
    assert_eq!(
        &entries,
        &[entry(".gitignore", Untracked, File), entry("d/d", Untracked, Directory)],
        "aggregation kicks in here"
    );

    let (out, entries) = collect(&root, |keep, ctx| {
        walk(
            &root,
            &root,
            ctx,
            walk::Options {
                emit_ignored: Some(CollapseDirectory),
                ..options()
            },
            keep,
        )
    });
    assert_eq!(
        out,
        walk::Outcome {
            read_dir_calls: 5,
            returned_entries: entries.len(),
            seen_entries: 21 + 2,
        },
        "some untracked ones are hidden by default, folded directories"
    );
    assert_eq!(
        &entries,
        &[
            entry(".gitignore", Untracked, File),
            entry("a.o", Ignored(Expendable), File),
            entry("b.o", Ignored(Expendable), File),
            entry("c.o", Ignored(Expendable), File),
            entry("d/a.o", Ignored(Expendable), File),
            entry("d/b.o", Ignored(Expendable), File),
            entry("d/d/a", Untracked, File),
            entry("d/d/a.o", Ignored(Expendable), File),
            entry("d/d/b.o", Ignored(Expendable), File),
            entry("d/d/generated", Ignored(Expendable), Directory),
            entry("d/generated", Ignored(Expendable), Directory),
            entry("generated", Ignored(Expendable), Directory),
            entry("objs", Ignored(Expendable), Directory),
        ],
        "objects are aggregated"
    );

    let (out, entries) = collect(&root, |keep, ctx| {
        walk(
            &root,
            &root,
            ctx,
            walk::Options {
                emit_ignored: Some(CollapseDirectory),
                emit_untracked: CollapseDirectory,
                ..options()
            },
            keep,
        )
    });
    assert_eq!(
        out,
        walk::Outcome {
            read_dir_calls: 5,
            returned_entries: entries.len(),
            seen_entries: 21 + 3,
        },
        "some untracked ones are hidden by default, and folded directories"
    );
    assert_eq!(
        &entries,
        &[
            entry(".gitignore", Untracked, File),
            entry("a.o", Ignored(Expendable), File),
            entry("b.o", Ignored(Expendable), File),
            entry("c.o", Ignored(Expendable), File),
            entry("d/a.o", Ignored(Expendable), File),
            entry("d/b.o", Ignored(Expendable), File),
            entry("d/d", Untracked, Directory),
            entry_dirstat("d/d/a.o", Ignored(Expendable), File, Untracked),
            entry_dirstat("d/d/b.o", Ignored(Expendable), File, Untracked),
            entry_dirstat("d/d/generated", Ignored(Expendable), Directory, Untracked),
            entry("d/generated", Ignored(Expendable), Directory),
            entry("generated", Ignored(Expendable), Directory),
            entry("objs", Ignored(Expendable), Directory),
        ],
        "ignored ones are aggregated, and we get the same effect as with `git status --ignored` - collapsing of untracked happens\
        and we still list the ignored files that were inside.\
        Also note the entries that would be dropped in case of `git clean` are marked with `entry_dirstat`, which would display what's\
        done differently."
    );
    Ok(())
}

#[test]
fn precious_are_not_expendable() {
    let root = fixture("untracked-and-precious");
    let (out, entries) = collect(&root, |keep, ctx| {
        walk(
            &root,
            &root,
            ctx,
            walk::Options {
                emit_ignored: Some(CollapseDirectory),
                emit_untracked: CollapseDirectory,
                ..options()
            },
            keep,
        )
    });
    assert_eq!(
        out,
        walk::Outcome {
            read_dir_calls: 3,
            returned_entries: entries.len(),
            seen_entries: 10,
        },
    );

    assert_eq!(
        &entries,
        &[
            entry("a.o", Ignored(Expendable), File),
            entry("d/a.o", Ignored(Expendable), File),
            entry("d/b.o", Ignored(Expendable), File),
            entry("d/d", Untracked, Directory),
            entry_dirstat("d/d/a.precious", Ignored(Precious), File, Untracked),
        ],
        "by default precious files are treated no differently than expendable files, which is fine\
            unless you want to delete `d/d`. Then we shouldn't ever see `d/d` and have to deal with \
            a collapsed precious file."
    );

    for equivalent_pathspec in ["d/*", "d/", "d"] {
        let (out, entries) = collect_filtered(
            &root,
            |keep, ctx| {
                walk(
                    &root,
                    &root,
                    ctx,
                    walk::Options {
                        emit_ignored: Some(CollapseDirectory),
                        emit_untracked: CollapseDirectory,
                        ..options()
                    },
                    keep,
                )
            },
            Some(equivalent_pathspec),
        );
        assert_eq!(
            out,
            walk::Outcome {
                read_dir_calls: 3,
                returned_entries: entries.len(),
                seen_entries: 10,
            },
            "{equivalent_pathspec}: should yield same result"
        );

        assert_eq!(
            &entries,
            &[
                entry("d/a.o", Ignored(Expendable), File),
                entry("d/b.o", Ignored(Expendable), File),
                entry("d/d", Untracked, Directory),
                entry_dirstat("d/d/a.precious", Ignored(Precious), File, Untracked),
            ],
            "'{equivalent_pathspec}' should yield the same entries"
        );
    }

    let (out, entries) = collect(&root, |keep, ctx| {
        walk(
            &root,
            &root,
            ctx,
            walk::Options {
                emit_ignored: Some(CollapseDirectory),
                emit_untracked: CollapseDirectory,
                collapse_is_for_deletion: true,
                ..options()
            },
            keep,
        )
    });
    assert_eq!(
        out,
        walk::Outcome {
            read_dir_calls: 3,
            returned_entries: entries.len(),
            seen_entries: 9,
        },
    );

    assert_eq!(
        &entries,
        &[
            entry("a.o", Ignored(Expendable), File),
            entry("d/a.o", Ignored(Expendable), File),
            entry("d/b.o", Ignored(Expendable), File),
            entry("d/d/a.precious", Ignored(Precious), File),
            entry("d/d/new", Untracked, File),
        ],
        "If collapses are for deletion, we don't treat precious files like expendable/ignored anymore so they show up individually\
        and prevent collapsing into a folder in the first place"
    );
}

#[test]
#[cfg_attr(
    not(target_vendor = "apple"),
    ignore = "Needs filesystem that folds unicode composition"
)]
fn decomposed_unicode_in_directory_is_returned_precomposed() -> crate::Result {
    let root = gix_testtools::tempfile::TempDir::new()?;

    let decomposed = "a\u{308}";
    let precomposed = "ä";
    std::fs::create_dir(root.path().join(decomposed))?;
    std::fs::write(root.path().join(decomposed).join(decomposed), [])?;

    let (out, entries) = collect(root.path(), |keep, ctx| {
        walk(
            root.path(),
            root.path(),
            ctx,
            walk::Options {
                precompose_unicode: true,
                ..options()
            },
            keep,
        )
    });

    assert_eq!(
        out,
        walk::Outcome {
            read_dir_calls: 2,
            returned_entries: entries.len(),
            seen_entries: 1,
        }
    );
    assert_eq!(entries.len(), 1);
    assert_eq!(
        &entries[0],
        &entry(format!("{precomposed}/{precomposed}").as_str(), Untracked, File),
        "even root paths are returned precomposed then"
    );

    let (_out, entries) = collect(root.path(), |keep, ctx| {
        walk(
            &root.path().join(decomposed),
            root.path(),
            ctx,
            walk::Options {
                precompose_unicode: false,
                ..options()
            },
            keep,
        )
    });
    assert_eq!(entries.len(), 1);
    assert_eq!(
        &entries[0],
        &entry(format!("{decomposed}/{decomposed}").as_str(), Untracked, File),
        "if disabled, it stays decomposed as provided"
    );
    Ok(())
}

#[test]
fn root_must_be_in_worktree() -> crate::Result {
    let err = try_collect("worktree root does not matter here".as_ref(), |keep, ctx| {
        walk(
            "traversal".as_ref(),
            "unrelated-worktree".as_ref(),
            ctx,
            options(),
            keep,
        )
    })
    .unwrap_err();
    assert!(matches!(err, walk::Error::RootNotInWorktree { .. }));
    Ok(())
}

#[test]
#[cfg_attr(windows, ignore = "symlinks the way they are organized don't yet work on windows")]
fn worktree_root_can_be_symlink() -> crate::Result {
    let root = fixture_in("many-symlinks", "symlink-to-breakout-symlink");
    let (out, entries) = collect(&root, |keep, ctx| walk(&root.join("file"), &root, ctx, options(), keep));
    assert_eq!(
        out,
        walk::Outcome {
            read_dir_calls: 0,
            returned_entries: entries.len(),
            seen_entries: 1,
        }
    );
    assert_eq!(entries.len(), 1);
    assert_eq!(
        &entries[0],
        &entry("file", Untracked, File),
        "it allows symlinks for the worktree itself"
    );
    Ok(())
}

#[test]
fn root_may_not_go_through_dot_git() -> crate::Result {
    let root = fixture("with-nested-dot-git");
    for dir in ["", "subdir"] {
        let (out, entries) = collect(&root, |keep, ctx| {
            walk(
                &root.join("dir").join(".git").join(dir),
                &root,
                ctx,
                options_emit_all(),
                keep,
            )
        });
        assert_eq!(
            out,
            walk::Outcome {
                read_dir_calls: 0,
                returned_entries: entries.len(),
                seen_entries: 1,
            }
        );
        assert_eq!(entries.len(), 1, "no traversal happened as root passes though .git");
        assert_eq!(&entries[0], &entry("dir/.git", DotGit, Directory));
    }
    Ok(())
}

#[test]
fn root_enters_directory_with_dot_git_in_reconfigured_worktree_tracked() -> crate::Result {
    let root = fixture("nonstandard-worktree");
    let (out, entries) = try_collect_filtered_opts(
        &root,
        |keep, ctx| {
            walk(
                &root.join("dir-with-dot-git").join("inside"),
                &root,
                ctx,
                walk::Options {
                    emit_tracked: true,
                    ..options()
                },
                keep,
            )
        },
        None::<&str>,
        Some("dir-with-dot-git/.git"),
    )?;

    assert_eq!(
        out,
        walk::Outcome {
            read_dir_calls: 0,
            returned_entries: entries.len(),
            seen_entries: 1,
        }
    );

    assert_eq!(entries.len(), 1);
    assert_eq!(
        &entries[0],
        &entry("dir-with-dot-git/inside", Tracked, File),
        "everything is tracked, so it won't try to detect git repositories anyway"
    );

    let (out, entries) = try_collect_filtered_opts(
        &root,
        |keep, ctx| {
            walk(
                &root.join("dir-with-dot-git").join("inside"),
                &root,
                ctx,
                walk::Options {
                    emit_tracked: false,
                    ..options()
                },
                keep,
            )
        },
        None::<&str>,
        Some("dir-with-dot-git/.git"),
    )?;

    assert_eq!(
        out,
        walk::Outcome {
            read_dir_calls: 0,
            returned_entries: 0,
            seen_entries: 1,
        }
    );

    assert!(entries.is_empty());
    Ok(())
}

#[test]
fn root_enters_directory_with_dot_git_in_reconfigured_worktree_untracked() -> crate::Result {
    let root = fixture("nonstandard-worktree-untracked");
    let (_out, entries) = try_collect_filtered_opts(
        &root,
        |keep, ctx| {
            walk(
                &root.join("dir-with-dot-git").join("inside"),
                &root,
                ctx,
                options(),
                keep,
            )
        },
        None::<&str>,
        Some("dir-with-dot-git/.git"),
    )?;
    assert_eq!(entries.len(), 1);
    assert_eq!(
        &entries[0],
        &entry("dir-with-dot-git/inside", Untracked, File),
        "it can enter a dir and treat it as normal even if own .git is inside,\
         which otherwise would be a repository"
    );
    Ok(())
}

#[test]
fn root_may_not_go_through_nested_repository_unless_enabled() -> crate::Result {
    let root = fixture("nested-repository");
    let walk_root = root.join("nested").join("file");
    let (_out, entries) = collect(&root, |keep, ctx| {
        walk(
            &walk_root,
            &root,
            ctx,
            walk::Options {
                recurse_repositories: true,
                ..options()
            },
            keep,
        )
    });
    assert_eq!(entries.len(), 1);
    assert_eq!(
        &entries[0],
        &entry("nested/file", Untracked, File),
        "it happily enters the repository and lists the file"
    );

    let (out, entries) = collect(&root, |keep, ctx| walk(&walk_root, &root, ctx, options(), keep));
    assert_eq!(
        out,
        walk::Outcome {
            read_dir_calls: 0,
            returned_entries: entries.len(),
            seen_entries: 1,
        }
    );
    assert_eq!(entries.len(), 1);
    assert_eq!(
        &entries[0],
        &entry("nested", Untracked, Repository),
        "thus it ends in the directory that is a repository"
    );
    Ok(())
}

#[test]
fn root_may_not_go_through_submodule() -> crate::Result {
    let root = fixture("with-submodule");
    let (out, entries) = collect(&root, |keep, ctx| {
        walk(
            &root.join("submodule").join("dir").join("file"),
            &root,
            ctx,
            options_emit_all(),
            keep,
        )
    });
    assert_eq!(
        out,
        walk::Outcome {
            read_dir_calls: 0,
            returned_entries: entries.len(),
            seen_entries: 1,
        }
    );
    assert_eq!(entries.len(), 1, "it refuses to start traversal in a submodule");
    assert_eq!(
        &entries[0],
        &entry("submodule", Tracked, Repository),
        "thus it ends in the directory that is the submodule"
    );
    Ok(())
}

#[test]
fn walk_with_submodule() -> crate::Result {
    let root = fixture("with-submodule");
    let (out, entries) = collect(&root, |keep, ctx| walk(&root, &root, ctx, options_emit_all(), keep));
    assert_eq!(
        out,
        walk::Outcome {
            read_dir_calls: 2,
            returned_entries: entries.len(),
            seen_entries: 4,
        }
    );
    assert_eq!(
        entries,
        [
            entry(".gitmodules", Tracked, File),
            entry("dir/file", Tracked, File),
            entry("submodule", Tracked, Repository)
        ],
        "thus it ends in the directory that is the submodule"
    );
    Ok(())
}

#[test]
fn root_that_is_tracked_file_is_returned() -> crate::Result {
    let root = fixture("dir-with-tracked-file");
    let (out, entries) = collect(&root, |keep, ctx| {
        walk(&root.join("dir").join("file"), &root, ctx, options_emit_all(), keep)
    });
    assert_eq!(
        out,
        walk::Outcome {
            read_dir_calls: 0,
            returned_entries: entries.len(),
            seen_entries: 1,
        }
    );

    assert_eq!(entries.len(), 1);
    assert_eq!(
        &entries[0],
        &entry("dir/file", Tracked, File),
        "a tracked file as root just returns that file (even though no iteration is possible)"
    );
    Ok(())
}

#[test]
fn root_that_is_untracked_file_is_returned() -> crate::Result {
    let root = fixture("dir-with-file");
    let (out, entries) = collect(&root, |keep, ctx| {
        walk(&root.join("dir").join("file"), &root, ctx, options(), keep)
    });
    assert_eq!(
        out,
        walk::Outcome {
            read_dir_calls: 0,
            returned_entries: entries.len(),
            seen_entries: 1,
        }
    );

    assert_eq!(entries.len(), 1);
    assert_eq!(
        &entries[0],
        &entry("dir/file", Untracked, File),
        "an untracked file as root just returns that file (even though no iteration is possible)"
    );
    Ok(())
}

#[test]
fn top_level_root_that_is_a_file() {
    let root = fixture("just-a-file");
    let err = try_collect(&root, |keep, ctx| walk(&root, &root, ctx, options(), keep)).unwrap_err();
    assert!(matches!(err, walk::Error::WorktreeRootIsFile { .. }));
}

#[test]
fn root_can_be_pruned_early_with_pathspec() -> crate::Result {
    let root = fixture("dir-with-file");
    let (out, entries) = collect_filtered(
        &root,
        |keep, ctx| walk(&root.join("dir"), &root, ctx, options_emit_all(), keep),
        Some("no-match/"),
    );
    assert_eq!(
        out,
        walk::Outcome {
            read_dir_calls: 0,
            returned_entries: entries.len(),
            seen_entries: 1,
        }
    );
    assert_eq!(entries.len(), 1);

    assert_eq!(
        &entries[0],
        &entry("dir", Pruned, Directory),
        "the pathspec didn't match the root, early abort"
    );
    Ok(())
}

#[test]
fn file_root_is_shown_if_pathspec_matches_exactly() -> crate::Result {
    let root = fixture("dir-with-file");
    let (out, entries) = collect_filtered(
        &root,
        |keep, ctx| walk(&root.join("dir").join("file"), &root, ctx, options(), keep),
        Some("*dir/*"),
    );
    assert_eq!(
        out,
        walk::Outcome {
            read_dir_calls: 0,
            returned_entries: entries.len(),
            seen_entries: 1,
        }
    );
    assert_eq!(entries.len(), 1);

    assert_eq!(
        &entries[0],
        &entry("dir/file", Untracked, File),
        "the pathspec matched the root precisely"
    );
    Ok(())
}

#[test]
fn root_that_is_tracked_and_ignored_is_considered_tracked() -> crate::Result {
    let root = fixture("tracked-is-ignored");
    let walk_root = "dir/file";
    let (out, entries) = collect(&root, |keep, ctx| {
        walk(&root.join(walk_root), &root, ctx, options_emit_all(), keep)
    });
    assert_eq!(
        out,
        walk::Outcome {
            read_dir_calls: 0,
            returned_entries: entries.len(),
            seen_entries: 1,
        }
    );
    assert_eq!(entries.len(), 1);

    assert_eq!(
        &entries[0],
        &entry(walk_root, Tracked, File),
        "tracking is checked first, so we can safe exclude checks for most entries"
    );
    Ok(())
}

#[test]
fn root_with_dir_that_is_tracked_and_ignored() -> crate::Result {
    let root = fixture("tracked-is-ignored");
    for emission in [Matching, CollapseDirectory] {
        let (out, entries) = collect(&root, |keep, ctx| {
            walk(
                &root,
                &root,
                ctx,
                walk::Options {
                    emit_ignored: Some(emission),
                    emit_tracked: true,
                    emit_untracked: emission,
                    ..options_emit_all()
                },
                keep,
            )
        });
        assert_eq!(
            out,
            walk::Outcome {
                read_dir_calls: 2,
                returned_entries: entries.len(),
                seen_entries: 3,
            }
        );
        assert_eq!(entries.len(), 2);

        assert_eq!(
            entries,
            [
                entry(".gitignore", Tracked, File),
                entry("dir/file", Tracked, File)
            ],
            "'tracked' is the overriding property here, so we even enter ignored directories if they have tracked contents,\
            otherwise we might permanently miss new untracked files in there. Emission mode has no effect"
        );
    }

    Ok(())
}

#[test]
fn root_that_is_ignored_is_listed_for_files_and_directories() -> crate::Result {
    let root = fixture("ignored-dir");
    for walk_root in ["dir", "dir/file"] {
        for emission in [Matching, CollapseDirectory] {
            let (out, entries) = collect(&root, |keep, ctx| {
                walk(
                    &root.join(walk_root),
                    &root,
                    ctx,
                    walk::Options {
                        emit_ignored: Some(emission),
                        ..options()
                    },
                    keep,
                )
            });
            assert_eq!(
                out,
                walk::Outcome {
                    read_dir_calls: 0,
                    returned_entries: entries.len(),
                    seen_entries: 1,
                }
            );
            assert_eq!(entries.len(), 1);

            assert_eq!(
                &entries[0],
                &entry("dir", Ignored(Expendable), Directory),
                "excluded directories or files that walkdir are listed without further recursion"
            );
        }
    }
    Ok(())
}

#[test]
#[cfg_attr(
    not(target_vendor = "apple"),
    ignore = "Needs filesystem that folds unicode composition"
)]
fn decomposed_unicode_in_root_is_returned_precomposed() -> crate::Result {
    let root = gix_testtools::tempfile::TempDir::new()?;

    let decomposed = "a\u{308}";
    let precomposed = "ä";
    std::fs::write(root.path().join(decomposed), [])?;

    let (out, entries) = collect(root.path(), |keep, ctx| {
        walk(
            &root.path().join(decomposed),
            root.path(),
            ctx,
            walk::Options {
                precompose_unicode: true,
                ..options()
            },
            keep,
        )
    });

    assert_eq!(
        out,
        walk::Outcome {
            read_dir_calls: 0,
            returned_entries: entries.len(),
            seen_entries: 1,
        }
    );
    assert_eq!(entries.len(), 1);
    assert_eq!(
        &entries[0],
        &entry(precomposed, Untracked, File),
        "even root paths are returned precomposed then"
    );

    let (_out, entries) = collect(root.path(), |keep, ctx| {
        walk(
            &root.path().join(decomposed),
            root.path(),
            ctx,
            walk::Options {
                precompose_unicode: false,
                ..options()
            },
            keep,
        )
    });
    assert_eq!(entries.len(), 1);
    assert_eq!(
        &entries[0],
        &entry(decomposed, Untracked, File),
        "if disabled, it stays decomposed as provided"
    );
    Ok(())
}

#[test]
fn root_cannot_pass_through_case_altered_capital_dot_git_if_case_insensitive() {
    let root = fixture("with-nested-capitalized-dot-git");
    for dir in ["", "subdir"] {
        let (out, entries) = collect(&root, |keep, ctx| {
            walk(
                &root.join("dir").join(".GIT").join(dir),
                &root,
                ctx,
                walk::Options {
                    ignore_case: true,
                    ..options_emit_all()
                },
                keep,
            )
        });
        assert_eq!(
            out,
            walk::Outcome {
                read_dir_calls: 0,
                returned_entries: entries.len(),
                seen_entries: 1,
            }
        );
        assert_eq!(entries.len(), 1, "no traversal happened as root passes though .git");
        assert_eq!(
            &entries[0],
            &entry("dir/.GIT", DotGit, Directory),
            "it compares in a case-insensitive fashion"
        );
    }

    let (_out, entries) = collect(&root, |keep, ctx| {
        walk(
            &root.join("dir").join(".GIT").join("config"),
            &root,
            ctx,
            walk::Options {
                ignore_case: false,
                ..options()
            },
            keep,
        )
    });
    assert_eq!(entries.len(), 1,);
    assert_eq!(
        &entries[0],
        &entry("dir/.GIT/config", Untracked, File),
        "it passes right through what now seems like any other directory"
    );
}

#[test]
fn partial_checkout_cone_and_non_one() -> crate::Result {
    for fixture_name in ["partial-checkout-cone-mode", "partial-checkout-non-cone"] {
        let root = fixture(fixture_name);
        let not_in_cone_but_created_locally_by_hand = "d/file-created-manually";
        let (out, entries) = collect(&root, |keep, ctx| {
            walk(
                &root.join(not_in_cone_but_created_locally_by_hand),
                &root,
                ctx,
                options_emit_all(),
                keep,
            )
        });
        assert_eq!(
            out,
            walk::Outcome {
                read_dir_calls: 0,
                returned_entries: entries.len(),
                seen_entries: 1,
            }
        );
        assert_eq!(entries.len(), 1);

        assert_eq!(
            &entries[0],
            &entry("d", TrackedExcluded, Directory),
            "{fixture_name}: we avoid entering excluded sparse-checkout directories even if they are present on disk,\
            no matter with cone or without."
        );
    }
    Ok(())
}
