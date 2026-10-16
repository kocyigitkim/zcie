# Git adapter — install

The `post-commit` hook sends a `git.commit` event after each commit. It uses `jq` and `curl`. It never blocks a commit.

## 1. Install the hook

```sh
cp /path/to/ZCIEngine/adapters/git/post-commit /path/to/your/repo/.git/hooks/post-commit
chmod +x /path/to/your/repo/.git/hooks/post-commit
```

If the repository already has a `post-commit` hook, call this script from it.

## 2. Configure (optional)

```sh
git config zcie.workspace atheron              # default: ZCIE_WORKSPACE, then the folder name
git config zcie.repositoryId repo_01JAB3...     # the ID from `zcie repo add`
export ZCIE_API_KEY="zcie_..."                  # a key with write permission
```

## 3. Link a commit to a task

Add a trailer line to the commit message:

```
Fix weapon socket alignment

ZCIE-Task: #89
```

The daemon makes a verified `IMPLEMENTS` link from task `#89` to the commit (evidence `commit_trailer`).
The idempotency key is `git:<hash of the repository path>:<sha>`, so a replay does not make a second event.
