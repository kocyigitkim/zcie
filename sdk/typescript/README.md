# sdk/typescript — `@zcie/client`

Client of the local ZCIE daemon for Node 18+, Bun, Deno and TypeScript. No dependencies, no build step:
`src/index.js` is ESM JavaScript, `src/index.d.ts` has the types.

```js
import { Client } from "@zcie/client";

// ZCIE_URL, ZCIE_API_KEY or ZCIE_API_KEY_FILE, and the project from ZCIE_WORKSPACE or .claude/zcie_id.
const zcie = Client.fromEnv();
const answer = await zcie.query("where is parse_config defined", { tokenBudget: 250 });
const all = await zcie.query("which of my projects use Redis", { scope: "global" });
await zcie.capture({ type: "user.message", sessionId: "s-1", turnId: "1", payload: "Decision: we use Postgres 16." });
const source = await zcie.expand([answer.results[0].ref], "source");
```

Errors are `ZcieError` with `status` (0 = the daemon does not answer) and `code`.

Tests run against a daemon: `ZCIE_URL=… ZCIE_API_KEY=… ZCIE_TEST_REPO=… npm test`.
`cargo test -p zcie-tests --test sdks` starts a daemon and runs them (TS-15).
