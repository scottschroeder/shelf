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
```

Fields:

- `title`: label shown in the picker
- `root`: directory to scan for git repositories
- `extract`: regex used to derive the project name from the path
- `exclude`: list of regexes to skip paths (optional)
- `recurse`: continue scanning inside discovered repos (optional)


## Shell Aliases
Open a fuzzy finder, pick one of your projects, and `cd` into that directory.
```
alias dev='cd $(shelf project preset --tmux-rename default-only)'
```
