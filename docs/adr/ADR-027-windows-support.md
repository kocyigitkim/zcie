# ADR-027 — Windows 11 support

- Status: Accepted
- Date: 2026-10-22
- Tasks: D-089 of `docs/12-development-plan.md`

## Context
ZCIE ran on macOS only. The code used Unix file modes, Unix signals, `kill` and launchd. The client hooks are POSIX shell scripts with `jq` and `curl`. The user needs ZCIE on Windows 11 with Claude Code, Cursor and Codex CLI.

## Decision
ZCIE SHALL run on Windows 11 x64. The hook scripts SHALL run in Git Bash.

1. Shell. `claude::hooks::shell()` finds Git Bash: `ZCIE_BASH`, `CLAUDE_CODE_GIT_BASH_PATH`, `C:\Program Files\Git\bin\bash.exe`, then the `bin` folder next to `git.exe`. On Windows each hook command is `"<bash>" "<script with />" <args>`. On macOS the command stays the script path.
2. Paths. `zcie_read_input` changes `C:\x` to `C:/x` in `cwd` and `tool_input.file_path`. Git Bash and the daemon both accept that form. Folder searches stop at a drive root.
3. User folder. `user_home()` reads `HOME`, then `USERPROFILE`.
4. Files. Unix file modes are set on Unix only. The user profile folder of Windows is private by default.
5. Daemon control. The CLI checks a process with `tasklist`. `zcie daemon stop` writes `run/zcied.stop`; the daemon stops when it sees the file. The daemon reloads `clients.toml` when the file changes, instead of on SIGHUP.
6. Autostart. A hidden `.vbs` script in the Startup folder of the user starts `zcied` at login. It needs no administrator rights.
7. Programs. Program names get `.exe`. Clients that npm installs (`claude.cmd`, `codex.cmd`) start through `cmd /C`.
8. Package. `build-npm.sh` runs in Git Bash and packs `native/win32-x64/*.exe`. CI builds and tests on `windows-latest`.

## Consequences
- Windows needs Git for Windows and `jq` (`winget install Git.Git jqlang.jq`). Windows 11 has `curl`.
- The Windows autostart does not restart the daemon after a crash.
- The Windows build is checked only by CI. No manual test on a Windows 11 computer was done when this ADR was written.
