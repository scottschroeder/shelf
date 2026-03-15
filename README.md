# shelf

CLI to jump between projects

## Configuration

Shelf reads a YAML config named `shelf.yml` from your XDG config directory:

- `$XDG_CONFIG_HOME/shelf/shelf.yml` if `XDG_CONFIG_HOME` is set
- otherwise `~/.config/shelf/shelf.yml`

### Example

```yaml
projects:
  - title: work
    root: /Users/alex/src/work
    extract: src/work/(.*)
    recurse: true
  - title: oss
    root: /Users/alex/src/oss
    extract: src/oss/(.*)
    exclude:
      - src/oss/vendor
    recurse: true
  - title: tools
    root: /Users/alex/dev/tools
    extract: dev/tools/(.*)
directories:
  - path: /Users/alex/src/work/scratch
    label: Scratch
  - path: /Users/alex/src/work/notes
worktrees:
  root: /Users/alex/src/worktrees
```

Per Project Fields:

- `title`: label shown in the picker
- `root`: directory to scan for git repositories
- `extract`: regex used to derive the project name from the path
- `exclude`: list of regexes to skip paths (optional)
- `recurse`: continue scanning inside discovered repos (optional)

Worktree Fields:
- `root`: root folder used by `shelf worktree create` (required for `worktree create`)

Worktree create examples:
- `shelf worktree create handle-foo`: create worktree and branch `handle-foo`
- `shelf worktree create handle-foo --branch alex/feature-x`: create worktree `handle-foo` with branch `alex/feature-x`
- `shelf worktree create handle-foo --detach`: create detached worktree without creating a branch
- `shelf worktree create handle-foo origin/main`: create branch `handle-foo` from `origin/main`

Worktree cleanup:
- `shelf worktree cleanup`: select one or more linked worktrees with skim, then remove them
- Cleanup entries show branch/upstream information when available, and flags like `dirty`, `detached`, `locked`, or `prunable`
- Cleanup uses force removal so dirty worktrees are still removable after explicit selection


## Shell Aliases
Open a fuzzy finder, pick one of your projects, and `cd` into that directory.
```
alias dev='cd $(shelf project preset --tmux-rename default-only)'
```
