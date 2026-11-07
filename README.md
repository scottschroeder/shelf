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
    color: cyan
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
- `color`: optional project tag color (`blue`, `cyan`, `green`, `yellow`, `red`, `magenta`, `white`)
- `exclude`: list of regexes to skip paths (optional)
- `recurse`: continue scanning inside discovered repos (optional)

Worktree Fields:
- `root`: root folder used by `shelf worktree create` (required for `worktree create`)

Worktree create examples:
- `shelf worktree create handle-foo`: create worktree and branch `handle-foo`
- `shelf worktree create handle-foo --branch alex/feature-x`: create worktree `handle-foo` with branch `alex/feature-x`
- `shelf worktree create handle-foo --detach`: create detached worktree without creating a branch
- `shelf worktree create handle-foo origin/main`: create branch `handle-foo` from `origin/main`


Worktree interactive:
- `shelf worktree interactive`: pick any linked worktree of the current repo and print its path
- `shelf worktree interactive foo`: print the path of linked worktree `foo`, or create it (detached by default) if it does not exist
- `shelf worktree interactive foo --branch alex/feature-x`: create `foo` with branch `alex/feature-x` instead of detaching
- Inside the picker, alt-enter creates a worktree named after the query (works even when the query fuzzy-matches existing entries, since their text is verbose); Enter on a query that matched nothing also creates
- Print-a-path contract matches `shelf project preset`, so shell integration is e.g. `worktree() { cd "$(shelf worktree interactive "$@")"; }`

Worktree cleanup:
- `shelf worktree cleanup`: select one or more linked worktrees with skim, then remove them
- Cleanup entries show branch/upstream information when available, and flags like `dirty`, `detached`, `locked`, or `prunable`
- Cleanup uses force removal so dirty worktrees are still removable after explicit selection


## Shell Aliases
Open a fuzzy finder, pick one of your projects, and `cd` into that directory.
```
alias dev='cd $(shelf project preset --tmux-rename default-only)'
```
