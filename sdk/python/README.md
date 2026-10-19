# sdk/python — `zcie`

Client of the local ZCIE daemon. Python 3.9+, standard library only. Responses are dicts with the JSON fields of the API.

```python
from zcie import Client

zcie = Client.from_env()  # ZCIE_URL, ZCIE_API_KEY or ZCIE_API_KEY_FILE, ZCIE_WORKSPACE or .claude/zcie_id
answer = zcie.query("where is parse_config defined", token_budget=250)
everything = zcie.query("which of my projects use Redis", scope="global")
zcie.capture("user.message", session_id="s-1", turn_id="1", payload="Decision: we use Postgres 16.")
source = zcie.expand([answer["results"][0]["ref"]], "source")
```

Errors are `ZcieError` with `status` (0 = the daemon does not answer) and `code`.

Install: `pip install ./sdk/python`. Tests run against a daemon:
`ZCIE_URL=… ZCIE_API_KEY=… ZCIE_TEST_REPO=… python3 -m unittest discover -s tests` (in `sdk/python`).
`cargo test -p zcie-tests --test sdks` starts a daemon and runs them (TS-15).
