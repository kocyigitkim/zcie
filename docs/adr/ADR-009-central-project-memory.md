# ADR-009 — Central project memory with a project ID file

- Status: Accepted
- Date: 2026-10-16

## Context
The owner works on many projects on one computer. Each Claude Code session must find the memory of its project. The owner also wants to ask about any project from any folder. Before this ADR, each client set the workspace name in the environment variable `ZCIE_WORKSPACE`. That value was per settings file. It did not move with the folder, and a new folder had no memory until a person ran a setup command.

Requirements from the owner:

1. All indexes and data are global: one store per computer (`~/.zcie`).
2. Each project folder has the file `.claude/zcie_id`. The file contains the project ID. All memory is kept by the project ID.
3. A question about any project gets an answer from any folder.
4. The ID file is committed to Git. A clone of the repository on another computer has the same ID.
5. If a folder has no ID file, the first Claude Code session in the folder makes it.
6. Maximum performance and verification. Minimum tokens.

## Decision

### 1. A project is a workspace record
A project is a workspace record in the existing registry. The project ID is the workspace ID. New projects get the prefix `proj_` (`proj_<ULID>`). Old IDs with the prefix `ws_` stay valid. ZCIE does not move or copy data. Every existing key, event and index entry stays the same.

### 2. The ID file
- Path: `<project root>/.claude/zcie_id`.
- Content: one line with the project ID, then a newline.
- Project root: the Git root of the folder. If the folder is not in a Git repository, the folder itself.
- Discovery: a client starts at its current folder and goes up through the parent folders. The first `.claude/zcie_id` with a valid ID wins.
- `ZCIE_WORKSPACE` stays as an explicit override (name or ID). Benchmarks and tests use it.
- ZCIE never makes a project for the user home folder or for `/`.

### 3. Who makes a project
| Client | Action |
|--------|--------|
| SessionStart hook (`session-start.sh`) | The only hook that makes a project. If no ID file is found, it sends `POST /v1/projects` with the folder name and the root, then writes the ID file. |
| `zcie claude setup` | Makes or connects the project and writes the ID file. |
| `zcie project init` | Same, from the command line. |
| `zcie project migrate` | Writes the ID file into each registered root that has no ID file. |
| Other hooks, MCP, CLI queries | Read the ID file only. If there is no project, the hook does nothing. |

`POST /v1/projects` is idempotent. A known ID returns the existing project. An unknown valid ID (for example from a clone) makes a project with that ID. A name collision gets a unique name (`name-2`).

### 4. Queries over more than one project
`QueryRequest` gets two fields:

- `scope`: `project` (default) or `global`. `global` reads all projects that the API key permits.
- `cross_project`: if true, the query also reads each permitted project whose name or project ID is a whole word in the question. Clients set it to true. If `cross_project` is true and the question asks about all projects (phrases such as "which of my projects", "all my projects", "tüm projeler", "hangi proje"), the query reads all permitted projects, as with `global`.

The engine runs the normal single-project query in each target project. Then it merges the results:

1. Each result gets the field `project` (the project name).
2. Projects that the question names come first. In global scope (and for "all projects" questions), the ranking uses term coverage (`crates/zcie-api/src/relevance.rs`): each result gets the weighted share of the question terms that it contains. Question words ("which", "projects", "uses") are not terms. A term in the results of few projects has a high weight: ln((projects + 1) / (projects with the term + 0.5)). Results with less than half of the best coverage are dropped, projects with no result are dropped, and the project with the best result comes first.
3. The merge takes results rank by rank (rank 1 of each project, then rank 2). Scores of different projects are not comparable, so a score sort can fill the token budget with one project only.
4. The normal token budget (`build_capsule`) applies to the merged list.
5. A merged response has no cursor. A caller asks again with a larger budget.

A query that reads one project returns the same response as before this ADR. It has no `project` labels.

### 5. Context injection
The UserPromptSubmit hook (`inject-context.sh`) sends `cross_project: true`. If the results have project labels (the prompt names another project), the hook injects the capsule. The capsule lines show `[project]` before each result. Generic prompts still get no capsule.

### 6. Project management
| Operation | HTTP | CLI | Permission |
|-----------|------|-----|------------|
| Make or connect | `POST /v1/projects` | `zcie project init` | Write |
| List | `GET /v1/projects` | `zcie project list` | Read |
| Show | `GET /v1/projects/{id or name}` | `zcie project show` | Read |
| Rename (the ID does not change) | `POST /v1/projects/{id}/rename` | `zcie project rename` | Write |
| Delete (forget all events and memory, remove from the registry) | `DELETE /v1/projects/{id}` | `zcie project delete` | Admin |

### 7. Idempotency keys
An idempotency key is unique per project. The store keeps `<project ID>/<client key>`. The same client key in two projects (for example the CLI default `cli:<session>:<turn>`), or in a deleted project and a new project, makes two events. Jobs of a removed worktree (a deleted project) end with no work.

## Consequences
- One daemon and one store hold the memory of all projects. A question from any folder can read any permitted project.
- The ID moves with the folder and with Git clones. Renaming or moving a folder does not lose memory.
- No data migration. Old workspaces keep their `ws_` IDs. `zcie project migrate` writes their ID files.
- A global query runs one query per project. The cost grows with the number of projects. Each single-project query uses its cache, so repeated global queries stay fast.
- Name matching for `cross_project` needs names of 3 characters or more. A short or common project name can match by accident. The user can rename the project.
- The ID file is public in a Git repository. It contains no secret: the API key is still necessary to read memory.
