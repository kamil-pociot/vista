# 24 · Repositories inside the project

A project often holds other repositories: a `repos/` folder in `.gitignore` with checkouts of related projects, or an untracked `vendor/lib` clone. git itself only sees the folder (ignored, or `? vendor/lib/`). vista finds those repositories and treats each one as its own, like IntelliJ's VCS roots:

```
 1: Project                       2: Commit
 ▾ my-app                         ▾ [x] Changes  3 files
   ▾ repos                            ▾ [x] my-app  main  1 file
     ▸ api          (blue)                [x] main.rs
     ▾ web          (blue)              ▾ [x] repos/web  dev  2 files
       ▸ build      (olive)               [x] app.ts
         app.ts     (blue)                [x] index.html
         todo.md    (red)             ▾ [ ] Unversioned Files  1 file
     notes.txt      (olive)             ▾ [ ] repos/web  dev  1 file
   main.rs          (blue)                [ ] todo.md
```

## What it does

- **Colours from the file's own repository.** A file inside `repos/web` is modified, unversioned or ignored according to `repos/web`'s `git status` and `.gitignore`, not greyed out as ignored by the project's. Files beside the clones (`repos/notes.txt`) stay ignored. A folder that holds clones stays visible even with ignored files hidden (`Alt-I`), and takes the colour of the changes inside.
- **Changes split by repository.** When changes come from more than one repository, the Commit window shows a row per repository under Changes and under Unversioned Files: its path in the project (the project's own repository by its folder name), its branch and its file count. A repository's row ticks, unticks and folds (`←`/`→`, `Enter`) as a whole. Folders (`g`) are relative to the repository. With changes in the project's repository only, the list looks as it always did.
- **Commits go where the files are.** `Ctrl-K` makes one commit per repository with ticked files, all with the same message ("Committed 3 files in 2 repositories: abc1234, def5678 …"); `P` pushes each of them. Amend amends each repository with ticked files (with nothing ticked, the project's), and prefills the last message of the first of them (the project's, when it has ticked files). Rollback and add to VCS run in each file's repository.
- **Diffs and gutter markers** compare with HEAD of the file's repository.
- **The status bar** counts changes across all repositories; the branch in the top bar is the project's.

## How repositories are found

On every status refresh, vista looks in the folders `git status` reports whole (ignored folders, and untracked folders, which with every untracked file listed means a repository) for a `.git`, up to three folders deep (`repos/` › `team/` › `app/.git`). Each repository found gets its own `git status`, in parallel, and is searched the same way (clones inside clones, three levels). Not searched: symlinks, `node_modules` and cache folders (a `CACHEDIR.TAG`, like Cargo's `target/`), so a big ignored build folder costs nothing. Worktrees of the project's own repository (such as `.claude/worktrees/…`) are other checkouts of the same project, not repositories of their own, and stay ignored. With `GIT_DIR` set, there is just that repository.

The file watcher already covers the whole project folder, so edits and commits inside a clone refresh the colours and the Commit window like any other change.

Submodules are not included: the project's repository tracks them as one entry.

## Code and tests

- `vista-git/src/status.rs` (`StatusSnapshot::load` finds and loads nested repositories, `nest` merges them, `repo` says which one a path is in, `is_ignored` asks that one), `Git::for_path` in `vista-git/src/cmd.rs`.
- `vista-tui/src/commit.rs` (`Row::Repo`, folding, per-repository folders), `ui/commit.rs`, `App::by_repo` / `do_commit` / rollback and add in `app.rs`; `viewer.rs` and `diffview.rs` use `Git::for_path`.
- Unit: merging snapshots, ignore rules and folder roll-up across repositories, flattening clones inside clones (`status`); repository rows, ticking and folding them, folders per repository (`commit`).
- Integration: `vista-git/tests/nested_repos.rs` (real nested clones: status, worktrees and caches left out, `for_path`, a commit stays in its repository).
- E2e: `e2e_r1_nested_repos.rs` (tree colours, the Commit window's repository rows and a commit in two repositories, a nested file's diff against its own HEAD).

See also: [01 · Project tree](01-project-tree.md), [06 · Commit window](06-commit-window.md), [07 · Diff viewer](07-diff-viewer.md).
