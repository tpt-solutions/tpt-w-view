# Post-release bug triage

## Intake

- Bugs come in as GitHub issues in the `tpt-solutions` organisation. Each report should state the
  tpt-view version, the operating system and GPU adapter (shown in the status line or the
  diagnostics report), and the steps to reproduce.
- If the user has opted in to crash logging, ask for the `crash.log` from the app log directory.
  Read it before asking for anything else.

## Triage

| Label | Meaning | Target |
|-------|---------|--------|
| `security` | Possible path escape, data loss, unsafe overwrite | Fix in a patch release, same week |
| `data-loss` | Original or output overwritten or truncated | Patch release |
| `crash` | Panic or abort | Next patch release |
| `regression` | Worked in the previous release | Next patch release |
| `decode` | An image decodes wrong or fails | Next minor release |
| `ui` | Visual or interaction problem | Next minor release |

## Process

1. Reproduce on the labelled platform. If you cannot, ask for the minimal file and steps.
2. Add a failing test in the crate that owns the behaviour before fixing it.
3. Add a changelog entry under **Fixed** (or **Security**).
4. Release a patch version when a `security`, `data-loss`, or `crash` fix lands.
