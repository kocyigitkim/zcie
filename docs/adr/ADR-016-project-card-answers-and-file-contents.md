# ADR-016 — Project card, assistant answers, file contents

- Status: Accepted
- Date: 2026-10-17

## Context
In a new Claude Code session in `dxcellence`, the question "bu proje hakkında bana bilgi ver" got no useful capsule. Claude read the files itself. The checks found these causes:
1. A question about the whole project has no specific words, so the word search found unrelated old messages.
2. The answers of the assistant were not stored. The Stop hook sent only `session.end`, and `capture.assistant_messages` was false.
3. The blob sweep deleted the stored content of indexed files after 10 minutes, because only event payloads had a blob reference. A search rebuild then indexed those files without their text.
4. The file count of a project included deleted files: a deleted file keeps its path lookup.
5. The daemon binary has an ad-hoc signature whose identifier changes at each build. After an update, macOS asks again for access to protected folders (Desktop). The first file read of the new daemon waits for the answer.

## Decision
- **Project card.** An overview question gets an answer from the index, with the note "project overview". The question matches phrases such as "bu proje hakkında", "projeyi anlat", "what is this project" and "project overview". The answer has:
  - each root folder with its number of current files;
  - the top-level files and folders, and the file types;
  - the projects whose roots are inside the folder, and whether they are indexed;
  - the first 1,500 characters of the README (stored content only; a query never reads project folders);
  - the newest decisions.
- **Assistant answers.** The new Stop hook `assistant-message.sh` sends `assistant.message` with `last_assistant_message`: the final text of the turn, at most 30,000 characters, without tool calls.
  - The setting `answers` of `zcie setup` (default true) writes `capture.assistant_messages`.
  - `zcie import claude` also imports the text parts of assistant records. The daemon refuses them (403) when `answers` is false, and the import skips them.
  - An answer is indexed in the conversations index, and its decision sentences become memory records.
- **File contents.** The blob sweep keeps the content blobs of all current file records.
  - A file whose record is current but whose blob is missing is indexed again at the next scan. This brings back the blob and the search text.
  - An unchanged file with its blob present is skipped before the blob write and the parse.
- **Counts.** `zcie project status` and the card count only current (not deleted) files (`zcie_store::files::current_paths`).
- **macOS access.** The watchers start in a thread. A start slower than 20 s writes a warning about the privacy prompt.
  - After an update, the user must allow the access of `~/.zcie/bin/zcied` to the folder once, in the prompt or in System Settings > Privacy & Security.
  - A stable permission needs a stable code signature (a Developer ID, or a local signing certificate). That is later work.

## Consequences
- dxcellence: 20 current files (before: 77 with deleted files); the card lists `d-xcellence` (5,923 files) and `xavier` (878 files) as indexed inner projects. The card answers in 0.12 s.
- New sessions get the answers of earlier sessions, not only what the user wrote.
