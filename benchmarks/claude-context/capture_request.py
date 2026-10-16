#!/usr/bin/env python3
"""Captures the raw API request body that Claude Code sends, without forwarding it.

Usage: capture_request.py <port> <out-dir>
Run Claude Code with ANTHROPIC_BASE_URL=http://127.0.0.1:<port>. Each POST body is written to
<out-dir>/request-<n>.json. Headers (they contain credentials) are never written. The server
answers with an error, so nothing reaches the real API.
"""
import json
import os
import sys
from http.server import BaseHTTPRequestHandler, HTTPServer

PORT, OUT = int(sys.argv[1]), sys.argv[2]
os.makedirs(OUT, exist_ok=True)
COUNT = [0]


class Handler(BaseHTTPRequestHandler):
    def do_POST(self):
        body = self.rfile.read(int(self.headers.get("content-length", 0)))
        if "/messages" in self.path and b'"tools"' in body:
            COUNT[0] += 1
            with open(os.path.join(OUT, f"request-{COUNT[0]}.json"), "wb") as handle:
                handle.write(body)
        self.send_response(400)
        self.send_header("content-type", "application/json")
        self.end_headers()
        self.wfile.write(json.dumps({"type": "error", "error": {"type": "invalid_request_error", "message": "captured by capture_request.py"}}).encode())

    def log_message(self, *args):
        pass


HTTPServer(("127.0.0.1", PORT), Handler).serve_forever()
