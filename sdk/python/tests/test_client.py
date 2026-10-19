"""Runs against a running daemon: ZCIE_URL, ZCIE_API_KEY (admin) and ZCIE_TEST_REPO (a fixture repository)."""

import os
import tempfile
import time
import unittest
from pathlib import Path

from zcie import Client, ZcieError, find_project_id

ENV = os.environ


def wait_for(check):
    for _ in range(300):
        if check():
            return
        time.sleep(0.1)
    raise AssertionError("condition not reached")


class ClientTest(unittest.TestCase):
    client = Client(ENV.get("ZCIE_URL", ""), ENV.get("ZCIE_API_KEY", ""), "sdk-py")

    def test_project_repository_query_expand(self):
        self.assertEqual(self.client.health()["status"], "ok")
        self.assertTrue(self.client.ensure_project(name="sdk-py")["project_id"].startswith("proj_"))
        self.client.register_repository(ENV["ZCIE_TEST_REPO"])
        found = {}
        wait_for(lambda: found.update(self.client.query("where is attach_to_bone defined", 250)) or found.get("answer"))
        self.assertEqual(found["class"], "symbol_definition")
        ref = next(r["ref"] for r in found["results"] if r["kind"] == "symbol")
        item = self.client.expand([ref], "source", 1000)["items"][0]
        self.assertEqual(item["status"], "ok")
        self.assertIn("attach_to_bone", item["content"])

    def test_capture_is_idempotent_and_memory_is_found(self):
        args = dict(event_type="user.message", session_id="s-py", turn_id="1", idempotency_key="py:1",
                    payload="Decision: we use slerp for camera blends.")
        first = self.client.capture(**args)
        again = self.client.capture(**args)
        self.assertEqual(again["event_id"], first["event_id"])
        self.assertTrue(again["duplicate"])
        wait_for(lambda: self.client.query("slerp camera blends", mode="memory")["results"])

    def test_errors_carry_status_and_code(self):
        with self.assertRaises(ZcieError) as wrong:
            Client(ENV.get("ZCIE_URL", ""), "zcie_wrong", "sdk-py").list_projects()
        self.assertEqual(wrong.exception.status, 401)
        with self.assertRaises(ZcieError) as down:
            Client("http://127.0.0.1:9", "x", timeout=2).health()
        self.assertEqual(down.exception.status, 0)

    def test_find_project_id_and_from_env(self):
        root = Path(tempfile.mkdtemp(prefix="zcie-py-"))
        (root / ".claude").mkdir()
        (root / "src" / "deep").mkdir(parents=True)
        (root / ".claude" / "zcie_id").write_text("proj_01M52Y8GKM6NJ53WCSMFY4W55A\n")
        self.assertEqual(find_project_id(str(root / "src" / "deep")), "proj_01M52Y8GKM6NJ53WCSMFY4W55A")
        self.assertEqual(Client.from_env({"ZCIE_API_KEY": "k"}, str(root / "src")).project, "proj_01M52Y8GKM6NJ53WCSMFY4W55A")
        self.assertEqual(Client.from_env({"ZCIE_API_KEY": "k", "ZCIE_WORKSPACE": "other"}, str(root)).project, "other")


if __name__ == "__main__":
    unittest.main()
