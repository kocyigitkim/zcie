# ADR-014 — npm package, setup wizard, settings and end-to-end verification

- Status: Accepted
- Date: 2026-10-17

## Context
The user wants to install ZCIE on a computer with one npm command, choose the settings during the installation, change them later, and know that the installation really works. Before this change, the installation needed a Rust build, 4 to 6 commands (`daemon start`, `daemon install`, `claude setup`, `vector install`, `import claude`) and the check `zcie doctor`, which does not prove that a new session remembers an earlier one.

## Decision

### 1. npm package `zcie`
- Folder `npm/zcie`: `bin/zcie.js` (launcher), `package.json`, `README.md`. `scripts/packaging/build-npm.sh` builds the release programs `zcie`, `zcied` and `zcie-mcp` and puts them in `native/<platform>-<arch>/` of a copy of the folder. It sets `version` from the Rust workspace and `os`/`cpu` from the platforms in the package, and writes `dist/zcie-<version>.tgz`.
- The launcher runs `native/<platform>-<arch>/zcie` with all arguments and the terminal (`stdio: inherit`) and returns its exit code. It sets the executable bit again if a copy dropped it. For a platform without a build it stops with a clear message. Before `zcie setup`, other commands print the hint "Run: zcie setup".
- The package has no `postinstall` script: npm 7 and later hide the output of install scripts and give them no terminal, so a wizard cannot run there. The user runs `zcie setup` after `npm install -g`.
- This package has the build of macOS on Apple Silicon. Other platforms need a build on that platform (the script uses the platform of the computer that runs it).

### 2. Settings (`<home>/config/setup.toml`)
| Key | Values | Default | Effect |
|---|---|---|---|
| `language` | `tr`, `en` | `tr` if `LANG` starts with `tr` | language of the wizard and the reports |
| `port` | 1024–65535 | 47831 | `server.http_addr`; the CLI, the hooks and the plugin use it |
| `vector` | `multilingual`, `english`, `off` | `multilingual` | `vector.enabled`, `vector.model`; the model is installed |
| `workers` | 1, 2, 4 | 2 | `ingest.worker_count` (CPU use while a project is indexed; before: 4) |
| `autostart` | true, false | true (macOS) | launchd agent |
| `claude` | `user`, `project`, `none` | `user` | Claude Code hooks for all projects, for one folder, or none |
| `claude_dir` | folder | — | folder of `claude = project` |
| `profile` | `auto`, `balanced`, `lean`, `off` | `auto` | Claude Code tool profile (`off`: the tool list is not changed) |
| `budget` | 200–4000 | 1200 | `ZCIE_INJECT_BUDGET` of the hooks; `budget` of `client.json` |
| `opencode` | true, false | false | global OpenCode plugin and MCP server |

- `zcie setup` asks each setting that no option gives, with the current value as the default (a new setup: the defaults). An empty answer or the end of the input takes the default, so piped answers work. `--yes` asks nothing. Each setting has an option (`--port`, `--vector`, …). The wizard shows a summary and applies it only after a confirmation. At the end it offers the import of earlier Claude Code conversations (only if `~/.claude/projects` exists) and the live test (only if `claude` is installed).
- `zcie config show` prints the settings with their allowed values. `zcie config set <key> <value> …` checks all values first (a wrong value changes nothing), applies them and runs `zcie verify`. Running `zcie setup` again is the other way to change the settings.
- When the CLI has no `--url` and no `ZCIE_URL`, it uses the port of `setup.toml`.

### 3. Apply (`apps/zcie/src/commands/setup/apply.rs`)
1. Write the four `engine.toml` keys (other keys stay) and `client.json`.
2. Install the vector model if it is missing. A verified file of `~/.zcie/models` is copied instead of a new download.
3. Install the hook scripts.
4. Daemon:
   - With autostart, a copy of `zcied` in `<home>/bin` runs from a launchd agent. The copy keeps working when the npm or nvm folder changes, and it is not in a folder that macOS protects.
   - The agent label is `dev.zcie.zcied` for `~/.zcie`, and `dev.zcie.zcied.<hash>` for another home, so that a test installation does not replace the real agent.
   - A change of `port`, `vector` or `workers`, or a new program, restarts the daemon (`launchctl kickstart -k`, or stop and start without autostart).
5. Claude Code:
   - The setup of the old scope is removed when the scope changes.
   - With `claude = user`, the folder setups of the registered projects are removed. Each hook would otherwise run twice, which gives two captures of each prompt and two capsules. Only the entries that ZCIE added are removed (`zcie-setup.json` lists them).
6. OpenCode: install or remove the global plugin.
7. Save `setup.toml`.

`zcie uninstall` removes the Claude Code and OpenCode entries and the agent, and stops the daemon. `--purge` also deletes the home folder. `zcie daemon restart` restarts the daemon (through launchd when the agent is loaded). `zcie daemon stop` waits 90 s (before: 30 s), because a long file job ends first.

### 4. Verification (`zcie verify [--live]`)
`zcie setup` and `zcie config set` run the checks at the end; each line is ✓ or ✗ with a fix. The command fails if a check fails.
- **Programs:** `zcie`, `zcied` and `zcie-mcp` are present.
- **Tools:** `curl` and `jq` are present (the hooks do nothing without them).
- **Daemon:** the daemon answers with the same version as `zcie`.
- **Autostart:** the launchd agent is loaded (with autostart).
- **Settings files:** `engine.toml` matches `setup.toml`, and the Claude Code settings file has the hooks.
- **Memory, with the installed hook scripts and the input that Claude Code gives them:**
  1. The SessionStart hook in a new temporary folder makes the project ID file.
  2. The UserPromptSubmit hook of session 1 captures a Turkish fact with a unique code.
  3. The context hook of a NEW session 2 must print the code.
  4. With the multilingual model, an English question must also find the Turkish fact.
  5. The temporary project is deleted.
- **MCP:** `zcie-mcp` answers `initialize` and `tools/list` with `zcie_query` and `zcie_expand`.
- **OpenCode:** the plugin file is present (when enabled).
- **Live (`--live`):**
  - Two real `claude -p` sessions run in a temporary folder: session 1 tells a code, and session 2 (new, no `--resume`) must answer with it.
  - Variables of a surrounding ZCIE setup (`ZCIE_WORKSPACE`, `ZCIE_URL`, key variables) are removed for the test sessions. Before this fix, a setup that ran inside a Claude Code session wrote the facts into the project of that session.
  - A missing `claude` program (Claude Code updates itself) is tried again for 30 s.
  - It costs about $0.05 on the account of the user.

## Consequences
- Installation: `npm install -g ./dist/zcie-0.1.0.tgz` (or `npm install -g zcie` after a publication) and `zcie setup`.
- Measured on macOS 26.5 (Apple Silicon) with an isolated npm prefix, home and port:
  - Setup with the live test: 12/12 checks. The new Claude Code session answered with the code of the earlier session.
  - The same two sessions without ZCIE answered "Bunu bilmiyorum".
  - Wizard change (language, profile, budget, model, workers, autostart off, port): applied. `config set` back: 11/11 checks.
  - `kill -9` of the daemon: launchd started it again in 3 s.
  - `uninstall --purge`: the agent, the daemon, the Claude Code entries and the data were removed.
  - Folder setup replaced by the setup for all projects: no hook runs twice.
- The package is about 40 MB unpacked (three programs). It has one platform.
- `npm uninstall -g zcie` does not run `zcie uninstall` (npm 7 and later run no uninstall scripts). The README says to run `zcie uninstall` first.
