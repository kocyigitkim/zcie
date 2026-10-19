# ADR-015 — Only added folders are indexed

- Status: Accepted
- Date: 2026-10-17

## Context
`zcie import claude` and the SessionStart hook register the folder of each conversation as a root of its project. Each registration also started a full scan and a file watcher. On the computer of the user this gave about 50 scanned and watched folders, and 159,732 pending jobs for 52,508 files. The jobs grew instead of shrinking, and the daemon used about 300 % CPU for hours. A folder that the user added with `zcie project add` waited behind this backlog. Three causes made the backlog:
1. All registered folders were indexed, including folders that only had conversations.
2. Each daemon start queued a scan of all registered folders again, so the same files were queued again (up to 32 times).
3. The file watcher did not apply `.gitignore`, so build outputs (`build/*.elf`, `.build/`) made new jobs at each build.

The user decided: only the changes of folders that were added with `zcie project add` are taken.

## Decision
- `WorktreeRecord.indexed` (default false). `true` means that the files of the folder are scanned, indexed and watched.
- `EnsureProjectRequest.index` (default false). `RegisterRepositoryRequest.index` (default true).
  - `zcie project add` sends `index = true`.
  - `zcie repo add` and `POST /v1/repositories` keep their behavior (index).
  - Conversation import, the SessionStart hook, `zcie claude setup` and `zcie project init` register the folder for the memory only.
- A registration never sets `indexed` back to false: a hook that registers an added folder again keeps it indexed.
- A folder that is not indexed gets no scan, no watcher and no reconciliation scan at startup. File and scan jobs of such a folder end as no-ops.
- At startup, the pending file and scan jobs of folders that are not indexed are deleted. On the computer of the user, the first start deleted 147,567 jobs.
- The file watcher applies the `.gitignore` and `.git/info/exclude` rules of the root, like the scan.
- `zcie project status` shows "index off: memory only" for a project with no indexed folder. `ProjectStatus.indexed` has the same value.
- Records of folders from before this rule have `indexed = false`. Run `zcie project add <folder>` again for a folder that must be indexed. The files that were indexed before stay in the index; they are not updated.

- A Git repository inside a project folder is its own project. The search for `.claude/zcie_id` stops at a Git repository root, in the hooks, the CLI, the SDKs and the OpenCode plugin. Before this rule, a session in `dxcellence/d-xcellence` (no ID file) got the project of `dxcellence`.
- `zcie project add` lists the folders that it does not index: Git repositories inside the folder (with the command to add each), and top folders that `.gitignore` excludes. `--nested` adds each inner repository as its own project.
- An unchanged file (same content hash and parser version) is skipped before the blob write and the parse. A scan queues each tracked file, so a rescan must be cheap. Measured: 2,681 jobs of `d-xcellence` in less than 90 s, against about 200 jobs per minute before.
- The watchers start in a thread after the HTTP gateway. The first access of a launchd daemon to `~/Desktop` waited 3 min 15 s once, probably for the macOS privacy check, and the daemon did not answer during this time. The next start took 5 s, and the watchers started in 8 s in the background.

## Consequences
- After the change, the queue had 0 jobs and the daemon used 2.5 % CPU. `zcie project add` of dxcellence completed in seconds (77 files).
- The memory (captured messages, decisions, the project of each folder) works the same for all folders.
- dxcellence: the root has 77 indexed files (the other 16,151 files are in `.gitignore` folders: two inner repositories, `data/`, `backups/`). `d-xcellence` (5,923 files, 24,173 symbols, 200 commits) and `xavier` (878 files, 3,848 symbols, 200 commits) are their own projects.
