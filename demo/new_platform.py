#!/usr/bin/env python3
"""New platform that the migration imports into.

Accepts an import (POST /import) and reports its count (GET /stats, GET /count).
Stores patients in memory, upserting by id. Fixed port 8092.
"""

import json
import threading
from http.server import ThreadingHTTPServer, BaseHTTPRequestHandler
from urllib.parse import urlparse

DEFAULT_PORT = 8092


class State:
    def __init__(self):
        self.patients = {}  # id -> patient
        self.order = []
        self.lock = threading.Lock()

    def count(self):
        with self.lock:
            return len(self.patients)

    def all_patients(self):
        with self.lock:
            return [self.patients[i] for i in self.order]

    def import_patients(self, items):
        if not isinstance(items, list):
            raise ValueError("patients must be a list")
        imported = 0
        with self.lock:
            for p in items:
                if not isinstance(p, dict):
                    continue
                pid = p.get("id")
                if pid is None:
                    pid = "imported-{}".format(len(self.patients) + imported + 1)
                    p = dict(p)
                    p["id"] = pid
                if pid not in self.patients:
                    self.order.append(pid)
                self.patients[pid] = dict(p)
                imported += 1
            return imported

    def reset(self):
        with self.lock:
            self.patients = {}
            self.order = []


STATE = State()


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def _send(self, status, obj):
        data = json.dumps(obj).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self):
        path = urlparse(self.path).path.rstrip("/") or "/"
        if path == "/health":
            self._send(200, {"status": "ok", "patient_count": STATE.count()})
            return
        if path == "/patients":
            self._send(200, STATE.all_patients())
            return
        if path in ("/stats", "/count"):
            self._send(200, {"patient_count": STATE.count()})
            return
        self._send(404, {"error": "not found"})

    def do_POST(self):
        path = urlparse(self.path).path.rstrip("/") or "/"
        length = int(self.headers.get("Content-Length", "0"))
        raw = self.rfile.read(length) if length > 0 else b""
        if path == "/import":
            try:
                body = json.loads(raw.decode("utf-8")) if raw else {}
            except json.JSONDecodeError:
                self._send(400, {"error": "invalid json"})
                return
            if isinstance(body, list):
                items = body
            elif isinstance(body, dict):
                if "patients" in body:
                    items = body["patients"]
                elif "items" in body:
                    items = body["items"]
                else:
                    items = [body]
            else:
                self._send(400, {"error": "expected list or object"})
                return
            if not isinstance(items, list):
                self._send(400, {"error": "patients must be a list"})
                return
            try:
                imported = STATE.import_patients(items)
            except ValueError as e:
                self._send(400, {"error": str(e)})
                return
            self._send(200, {"imported": imported, "patient_count": STATE.count()})
            return
        if path == "/reset":
            STATE.reset()
            self._send(200, {"patient_count": STATE.count()})
            return
        self._send(404, {"error": "not found"})

    def log_message(self, *args):
        pass


def main():
    import argparse
    parser = argparse.ArgumentParser()
    parser.add_argument("--port", type=int, default=DEFAULT_PORT)
    args = parser.parse_args()
    httpd = ThreadingHTTPServer(("127.0.0.1", args.port), Handler)
    print("new platform listening on http://127.0.0.1:{}/ ({} patients)".format(args.port, STATE.count()), flush=True)
    httpd.serve_forever()


if __name__ == "__main__":
    main()
