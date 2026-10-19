# ADR-013 — Memory accuracy: Turkish search, corrections, follow-up context, multilingual vectors

- Status: Accepted
- Date: 2026-10-17

## Context
The goal is that a NEW session (no `--resume`, no shared context) answers each question about earlier sessions correctly. The user writes Turkish and mixes Turkish and English words ("toolların hepsi", "araç tanımları"). Tests with the 30-stage scenario (`benchmarks/memory-30`) and the user-ideas questions (`benchmarks/user-ideas`) showed these faults:

1. Turkish inflected words did not match ("onay" and "onayı", "API'sine").
2. A long pasted specification got a low score, because one score covers the full text.
3. A plain correction ("Düzeltme: PostgreSQL 16 değil, 17") did not replace the old statement.
4. "List all decisions" returned only the best few items.
5. An English question did not find a Turkish fact, and a Turkish question did not find English code.
6. A short follow-up message ("bunu otomatik yapsın ama maliyet artmasın") has no topic words.
7. A search rebuild lost documents, the daemon used too much memory, and interactive jobs waited behind background jobs.

## Decision

### 1. Turkish search
- The search index has a `stems` field: letter folding (ç→c, ğ→g, ı/İ→i, ö→o, ş→s, ü→u, accents removed), apostrophe suffixes removed ("API'sine" → "API"), and words cut to 5 characters (F5 stemming). A stem query has the boost 0.5; the exact query keeps the full score.
- In the conversations index, only words with 7 or more characters get a stem query ("attachment" must not match "attack").
- Term coverage (`zcie-query/src/relevance.rs`) uses the same stems. Two stems match when they are equal or one is a prefix of the other with 4 or more characters. Turkish meta words ("kullanıcının", "istedi", "isteği", "cevap ver") are not terms.
- Bilingual equivalents (`zcie-query/src/synonyms.rs`): a question term also matches its equivalent in the other language (araç/tool, maliyet/cost, bağlam/context, karar/decision, gecikme/late, …). Words that are prefixes of unrelated words are not in the list.
- English history phrases ("did we", "have we", "we rejected", "we chose") and Turkish ones ("reddettik", "vazgeçtik") select the `timeline` class.

### 2. Long texts
- A text longer than 1,500 characters is indexed as passages of 1,500 characters with 200 characters of overlap. Each passage is a document with the same entity ref; the best passage gives the score.
- The response shows the window of 400 characters with the most question terms, not the first characters.
- A long or pasted document (more than 2,000 characters, or `<pasted_content>`) makes no tasks from refs: "#89" in a specification is an example, not a task.

### 3. Corrections
- A user message that starts with a correction marker ("Düzeltme", "Correction", "Güncelleme", "Aslında", "Yanlış söyledim", "I was wrong") or has "X değil, Y" or "Y, not X" corrects an earlier statement.
- The corrected record is the active earlier statement of the same project that shares the most words with the correction (3 or more words, or 2 words and the old value). It becomes `superseded`, and a `Supersedes` edge links the two records.
- A superseded record can still hold valid facts ("PostgreSQL 16; MySQL rejected" after "16 değil, 17"). The capsule therefore keeps it with the note "a later correction changes part of it; where they differ, the correction is true". The capsule first takes the best 6 results, then puts superseded records after the current records (the opposite order dropped the only record with the MySQL fact). The ranking weight of a superseded record is 0.1.

### 4. List of all decisions
- A question that asks for all decisions or rules returns the titles of all active statements of the project (at most 40 items, budget 2,000 tokens). Stored questions are not in the list. The capsule shows all items.

### 5. Multilingual vector search
- The default model is `potion-multilingual-128M` (model2vec, MIT, 500,353 Unigram pieces, 256 dimensions; revision `73908c3438cf03b6a01bcb9611d62b23d0726f08`). `potion-base-8M` stays available (`zcie vector install --model potion-base-8M`).
- The Unigram tokenizer is implemented in Rust (`zcie-vector/src/unigram.rs`): the normalizer of the model (punctuation split, white space, NFKC compatibility characters) and Viterbi over each word. Its token IDs are the same as the IDs of the Hugging Face `tokenizers` library on 12,752 of 12,752 test lines.
- The weights are read as a stream and kept as int8 with one scale per row: about 130 MB instead of 512 MB. The load takes about 0.3 s.
- The lowest useful similarity depends on the model: 0.20 for the multilingual model, 0.30 for `potion-base-8M` (`[vector] min_similarity` changes it).
- A stated requirement ("we use PostgreSQL 16; we rejected MySQL") is embedded. An observation and a stored question are not embedded.
- If the configured default model is not installed, the daemon uses the first installed known model.

### 6. Follow-up context (contextual retrieval)
- A user message of 400 characters or fewer gets `context`: the start (300 characters) of the user message before it in the same session. The order is the event sequence, not the event ID (IDs made in the same millisecond are not ordered).
- The context is in the search document and in the vector text, and the response item has it. The capsule shows it as "previous message of the user".
- In term coverage, a term that only the context has counts half. The earlier message, which has the term itself, therefore ranks first.
- A one-time migration (`follow_up_context_v2`) fills the context of records stored before this change and starts a search rebuild.

### 7. Follow-up messages and capsule budget
- The first 3 request records of a response that are chat messages (400 characters or fewer, not a pasted document) get `follow_up`: the next 2 user messages of the session, each cut to 150 characters, if they have 400 characters or fewer. A user often refines an idea in the next messages ("peki şunu istiyorum: bunu otomatik yapsın ama maliyet artmasın"). The capsule shows them as "next messages of the user".
- A sentence record (decision, preference) of a long message shows at most 390 characters around the sentence, not the full message. Before this change, six rules of one pasted document matched an unrelated question by the words of other parts of the document.
- The default capsule budget is 1,200 tokens (before: 800). With 800 tokens, the follow-up text left room for only 3 results. Everyday prompts get no capsule (0 of 15), so the larger budget applies to project questions only.
- General format and technology words written in capitals (JSON, YAML, README, API, …) are not project names, and a name match counts only in statements (memory, task, commit), not in code symbols.

### 8. Text that is not the words of the user
- A message that starts with "!" (a shell command in the bash mode of Claude Code) makes no memory record and is not used as context. A one-time migration (`shell_input_v1`) retracts such records.
- `<task-notification>`, `<local-command…>`, `<command-…>`, `<bash-…>`, "Caveat:" and "[Request interrupted" prompts are not captured and get no capsule (`zcie_is_user_text` in `common.sh`).

### 9. Import of earlier conversations
- `zcie import claude [--dir] [--include-automated] [--dry-run] [--existing-only]` reads the Claude Code transcripts. It imports user messages and the answers to `AskUserQuestion` (as "Decision: <answer> (question: <question>)"). It skips `sdk-cli` sessions (automated runs) unless `--include-automated` is given.
- A transcript folder maps to the Git root of its working folder (worktrees map to the main repository). Folders that only hold other projects are skipped.

### 10. Search index, memory and queue
- The search rebuild deletes the documents of the tenant with a term delete, not with `delete_all_documents`. `delete_all_documents` resets the operation stamp, so delete operations of concurrent jobs removed documents that the rebuild added after it (`crates/zcie-search/tests/rebuild.rs` fails with the old code). A sequence guard stops a second rebuild of the same state.
- A file larger than 1 MB is hashed as a stream and never read whole. RocksDB memtables are 64 MB in total (16 MB per column family), with a shared 32 MB block cache. Scans and the recovery read pages, not the full store.
- Background jobs use at most half of the queue; worker 0 takes interactive jobs only; the refill scan goes on from the last key of the previous refill. A captured message is searchable about 0.1 s after the capture while thousands of file jobs wait.

## Consequences
- A new session gets the facts of earlier sessions, in Turkish or English, with the current value after a correction.
- The daemon uses about 200 MB more memory with the multilingual model. A user who wants less memory installs `potion-base-8M` and sets `[vector] model`.
- The synonym list is a fixed list. A new mixed-language term needs a new entry.
- Results: `benchmarks/results/memory-accuracy-2026-10-17.md`.
