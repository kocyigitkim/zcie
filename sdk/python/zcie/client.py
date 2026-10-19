"""The HTTP client. Responses are plain dicts with the JSON fields of the API."""

import json
import os
import re
import time
import urllib.error
import urllib.parse
import urllib.request
import uuid
from pathlib import Path
from typing import Any, Dict, List, Mapping, Optional

DEFAULT_ENDPOINT = "http://127.0.0.1:47831"
EVENT_SCHEMA = "zcie.event.v1"
ID_FILE = ".claude/zcie_id"
_ID_PATTERN = re.compile(r"^(proj|ws)_[0-9A-Z]{26}$")


class ZcieError(Exception):
    """An error answer of the daemon, or a transport error (status 0)."""

    def __init__(self, status: int, code: str, message: str):
        super().__init__(f"{code} ({status}): {message}")
        self.status = status
        self.code = code
        self.message = message


def find_project_id(start: Optional[str] = None) -> Optional[str]:
    """Finds .claude/zcie_id in `start` (default: the current folder) or a parent folder.

    The search stops at a Git repository root: a repository inside another project folder is
    its own project."""
    folder = Path(start or os.getcwd()).resolve()
    for candidate in [folder, *folder.parents]:
        try:
            first = (candidate / ID_FILE).read_text(encoding="utf-8").splitlines()[0].strip()
        except (OSError, IndexError):
            first = ""
        if _ID_PATTERN.match(first):
            return first
        if (candidate / ".git").exists():
            return None
    return None


def _read_key(env: Mapping[str, str]) -> str:
    if env.get("ZCIE_API_KEY"):
        return env["ZCIE_API_KEY"]
    files = [env.get("ZCIE_API_KEY_FILE"), str(Path.home() / ".zcie" / "config" / "admin.key")]
    for file in filter(None, files):
        try:
            return Path(file).read_text(encoding="utf-8").splitlines()[0].strip()
        except (OSError, IndexError):
            continue
    return ""


def _quote(value: str) -> str:
    return urllib.parse.quote(value, safe="")


def _clean(body: Dict[str, Any]) -> Dict[str, Any]:
    return {k: v for k, v in body.items() if v is not None}


class Client:
    """A client of one daemon and one project (project ID or name)."""

    def __init__(self, endpoint: str = DEFAULT_ENDPOINT, api_key: str = "", project: str = "default", timeout: float = 30.0):
        self.endpoint = endpoint.rstrip("/")
        self.api_key = api_key
        self.project = project
        self.timeout = timeout

    @classmethod
    def from_env(cls, env: Optional[Mapping[str, str]] = None, cwd: Optional[str] = None) -> "Client":
        """ZCIE_URL, ZCIE_API_KEY or ZCIE_API_KEY_FILE (default ~/.zcie/config/admin.key),
        ZCIE_WORKSPACE or the .claude/zcie_id of the current folder."""
        env = os.environ if env is None else env
        project = env.get("ZCIE_WORKSPACE") or find_project_id(cwd) or "default"
        return cls(env.get("ZCIE_URL") or DEFAULT_ENDPOINT, _read_key(env), project)

    def request(self, method: str, path: str, body: Optional[Dict[str, Any]] = None) -> Any:
        """Sends one request. Raises ZcieError on a non-2xx answer."""
        data = json.dumps(_clean(body)).encode("utf-8") if body is not None else None
        request = urllib.request.Request(self.endpoint + path, data=data, method=method)
        request.add_header("Content-Type", "application/json")
        if self.api_key:
            request.add_header("Authorization", f"Bearer {self.api_key}")
        try:
            with urllib.request.urlopen(request, timeout=self.timeout) as response:
                text = response.read().decode("utf-8")
        except urllib.error.HTTPError as error:
            raise _api_error(error.code, error.read().decode("utf-8", "replace")) from None
        except (urllib.error.URLError, OSError) as error:
            raise ZcieError(0, "transport", str(error)) from None
        return json.loads(text) if text else None

    def health(self) -> Dict[str, Any]:
        return self.request("GET", "/v1/health")

    def capture(self, event_type: str, session_id: str, payload: Optional[str] = None, turn_id: Optional[str] = None,
                idempotency_key: Optional[str] = None, app: str = "sdk-python", repository_id: Optional[str] = None) -> Dict[str, Any]:
        """Captures one event. `idempotency_key` makes a retry safe."""
        key = idempotency_key or f"{app}:{session_id}:{turn_id or time.time_ns()}:{uuid.uuid4().hex[:8]}"
        source = _clean({"app": app, "session_id": session_id, "turn_id": turn_id})
        body = {"schema": EVENT_SCHEMA, "idempotency_key": key, "workspace": self.project, "repository_id": repository_id,
                "source": source, "type": event_type, "payload": payload}
        return self.request("POST", "/v1/events", body)

    def register_repository(self, path: str, project: Optional[str] = None) -> Dict[str, Any]:
        return self.request("POST", "/v1/repositories", {"workspace": self.project, "path": path, "project": project})

    def query(self, question: str, token_budget: Optional[int] = None, mode: Optional[str] = None, scope: Optional[str] = None,
              cross_project: bool = True, max_results: Optional[int] = None, cursor: Optional[str] = None,
              project: Optional[str] = None) -> Dict[str, Any]:
        """Runs a query. scope: "project" (default) or "global"."""
        body = {"workspace": project or self.project, "question": question, "mode": mode, "token_budget": token_budget,
                "max_results": max_results, "cursor": cursor, "scope": scope, "cross_project": cross_project}
        return self.request("POST", "/v1/query", body)

    def expand(self, refs: List[str], level: str = "evidence", token_budget: Optional[int] = None, with_related: Optional[List[str]] = None) -> Dict[str, Any]:
        """Expands refs and line ranges ("path:start-end"); with_related adds "callers" and "tests" of symbols."""
        body = {"workspace": self.project, "refs": refs, "level": level, "token_budget": token_budget}
        if with_related:
            body["with"] = with_related
        return self.request("POST", "/v1/expand", body)

    def get_entity(self, ref: str) -> Any:
        return self.request("GET", f"/v1/entities/{_quote(ref)}")

    def get_timeline(self, ref: str) -> Dict[str, Any]:
        return self.request("GET", f"/v1/timeline/{_quote(ref)}")

    def create_task(self, title: str, source_event_id: str, external_ref: Optional[str] = None) -> Dict[str, Any]:
        body = {"workspace": self.project, "title": title, "source_event_id": source_event_id, "external_ref": external_ref}
        return self.request("POST", "/v1/tasks", body)

    def link(self, from_ref: str, relation: str, to_ref: str, source_event_id: str, evidence: str = "user_confirmation",
             note: Optional[str] = None) -> Dict[str, Any]:
        body = {"workspace": self.project, "from": from_ref, "relation": relation, "to": to_ref, "evidence": evidence,
                "source_event_id": source_event_id, "note": note}
        return self.request("POST", "/v1/links", body)

    def correct_memory(self, memory_id: str, text: str, source_event_id: str) -> Dict[str, Any]:
        return self.request("POST", f"/v1/memory/{_quote(memory_id)}/correct", {"text": text, "source_event_id": source_event_id})

    def forget(self, selector: Dict[str, str], reason: str) -> Dict[str, Any]:
        return self.request("POST", "/v1/forget", {"workspace": self.project, "selector": selector, "reason": reason})

    def rebuild_search(self) -> Dict[str, Any]:
        return self.request("POST", "/v1/admin/rebuild-search")

    def ensure_project(self, name: Optional[str] = None, project_id: Optional[str] = None, root: Optional[str] = None) -> Dict[str, Any]:
        return self.request("POST", "/v1/projects", {"project_id": project_id, "name": name, "root": root})

    def list_projects(self) -> List[Dict[str, Any]]:
        return self.request("GET", "/v1/projects")["projects"]

    def get_project(self, id_or_name: Optional[str] = None) -> Dict[str, Any]:
        return self.request("GET", f"/v1/projects/{_quote(id_or_name or self.project)}")

    def rename_project(self, id_or_name: str, name: str) -> Dict[str, Any]:
        return self.request("POST", f"/v1/projects/{_quote(id_or_name)}/rename", {"name": name})

    def delete_project(self, id_or_name: str) -> Dict[str, Any]:
        return self.request("DELETE", f"/v1/projects/{_quote(id_or_name)}")


def _api_error(status: int, text: str) -> ZcieError:
    try:
        detail = json.loads(text)["error"]
        return ZcieError(status, detail.get("code", "http_error"), detail.get("message", text))
    except (ValueError, KeyError, TypeError):
        return ZcieError(status, "http_error", text)
