# ZCIE Bar

A macOS menu bar app that shows the token savings of ZCIE for each project (ADR-022).

## Build and start

```sh
apps/zcie-bar/build.sh            # build ~/Applications/ZCIE Bar.app and start it
apps/zcie-bar/build.sh --login    # also start it at login (LaunchAgent com.zcie.bar)
apps/zcie-bar/test.sh             # TS-22-06: decode the report of `zcie usage --json`
```

Requirements: macOS 14 or later, the Xcode command line tools (`swiftc`), and `zcie` in the PATH of the shell.

`build.sh` does these steps:

1. It copies `zcie` to `~/.zcie/bin/zcie`. An app has no shell PATH, and macOS asks before an app runs a program in Desktop, Documents or Downloads.
2. It runs `zcie usage` once from the shell. This records the project root of each folder in `~/.zcie/run/usage-roots.json`.
3. It builds the app.

Run `build.sh` again after you update ZCIE.

## What it shows

Open the app (Finder, Spotlight or `open -a "ZCIE Bar"`) to see the detailed report. The login item starts it with `--background`: then only the menu bar item shows.


- **Menu bar:** the saving % of the project with the last activity, or "ZCIE" when there is no value.
- **Menu:** the projects that used ZCIE in the last 90 days, with the saving % and the saved tokens.
- **Detailed report** (window), for each project:
  - the sessions with ZCIE and with no ZCIE: turns, median tokens per turn, Read and search calls per turn, and the token parts;
  - the context capsules, the read guard and the verification gate;
  - the tokens of each day.

## Data

- The app runs `zcie usage --json --no-disk` at start, every 5 minutes and on "Refresh". It reads no transcript and no project folder itself.
- `zcie usage` reads the Claude Code transcripts (`~/.claude/projects`) and the hook statistics (`~/.zcie/stats/hooks.jsonl`).
- The cache `~/.zcie/run/usage-cache.json` keeps counters only, never message text.

## Method

The saving is an observation. It compares the median tokens per turn of the sessions with ZCIE against the sessions with no ZCIE of the same project. It is shown only when each group has 3 sessions or more. The tasks of the two groups are not the same. The controlled measurement is the isolated benchmark (`benchmarks/isolated`, ADR-019).
