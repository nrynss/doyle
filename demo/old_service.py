#!/usr/bin/env python3
"""Old patient records service (Fernwhistle Veterinary Group).

Serves the v1, v2 OR 2q behaviour of testdata/contracts/patients-v*.yaml on a
fixed local port. Version is selected by --api-version or OLD_SERVICE_VERSION
(1, 2 or 2q).

Generates 4,812 fixed patients deterministically at startup. Nothing is stored
on disk; restarting restores the starting state.
"""

import argparse
import json
import os
import random
import threading
from http.server import ThreadingHTTPServer, BaseHTTPRequestHandler
from urllib.parse import urlparse, parse_qs, unquote

TOTAL_PATIENTS = 4812
SEED = 20240517
CONFIRMATION_TOKEN = "CONFIRM-DELETE-ALL-PATIENTS"
DEFAULT_PORT = 8091
MAX_LIMIT = 100
DEFAULT_LIMIT = 100


def generate_patients():
    rng = random.Random(SEED)
    species = ["dog", "cat", "rabbit", "horse", "ferret"]
    vaccines = ["rabies", "DHPP", "bordetella", "leptospirosis", "FVRCP"]
    substances = ["penicillin", "sulfonamide", "bee venom", "latex", "grain"]
    reactions = ["hives", "anaphylaxis", "vomiting", "swelling"]
    patients = []
    for i in range(1, TOTAL_PATIENTS + 1):
        n_vax = rng.randint(1, 3)
        vaccinations = []
        for _ in range(n_vax):
            vaccinations.append({
                "vaccine": rng.choice(vaccines),
                "given_on": "2025-{:02d}-{:02d}".format(rng.randint(1, 12), rng.randint(1, 28)),
                "due_on": "2026-{:02d}-{:02d}".format(rng.randint(1, 12), rng.randint(1, 28)),
            })
        n_all = rng.randint(0, 2)
        allergies = []
        for _ in range(n_all):
            allergies.append({
                "substance": rng.choice(substances),
                "reaction": rng.choice(reactions),
            })
        # de-duplicate allergies by substance, keeping it simple and stable
        seen = set()
        uniq = []
        for a in allergies:
            if a["substance"] in seen:
                continue
            seen.add(a["substance"])
            uniq.append(a)
        allergies = uniq
        patients.append({
            "id": "p-{:05d}".format(i),
            "name": "Patient {:05d}".format(i),
            "species": rng.choice(species),
            "breed": rng.choice(["mixed", "labrador", "siamese", "lop", "terrier"]),
            "owner_id": "owner-{:05d}".format(rng.randint(1, 2000)),
            "vaccinations": vaccinations,
            "allergies": allergies,
        })
    return patients


class State:
    def __init__(self, version):
        self.version = version
        self.patients = generate_patients()
        self.lock = threading.Lock()

    def count(self):
        with self.lock:
            return len(self.patients)

    def all_patients(self):
        with self.lock:
            return [dict(p) for p in self.patients]

    def get(self, pid):
        with self.lock:
            for p in self.patients:
                if p["id"] == pid:
                    return dict(p)
        return None

    def _slice(self, cursor, limit):
        # Caller holds self.lock. Returns (items, next_cursor, effective_limit).
        total = len(self.patients)
        offset = 0
        if cursor not in (None, ""):
            try:
                offset = int(cursor)
            except ValueError:
                raise ValueError("invalid cursor")
        if offset < 0:
            offset = 0
        if offset > total:
            offset = total
        if limit is None:
            limit = DEFAULT_LIMIT
        try:
            limit = int(limit)
        except (TypeError, ValueError):
            raise ValueError("invalid limit")
        if limit < 1:
            limit = 1
        if limit > MAX_LIMIT:
            limit = MAX_LIMIT
        end = min(offset + limit, total)
        items = [dict(p) for p in self.patients[offset:end]]
        next_cursor = str(end) if end < total else None
        return items, next_cursor, limit

    def page(self, cursor, limit):
        with self.lock:
            items, next_cursor, _ = self._slice(cursor, limit)
            return {"items": items, "next_cursor": next_cursor}

    def decommission(self, token):
        with self.lock:
            if token != CONFIRMATION_TOKEN:
                return False
            self.patients = []
            return True


STATE = None


class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"

    def _send(self, status, obj, content_type="application/json"):
        data = json.dumps(obj).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def _send_raw(self, status, data, content_type="application/json", extra_headers=None):
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(data)))
        for name, value in (extra_headers or []):
            self.send_header(name, value)
        self.end_headers()
        self.wfile.write(data)

    def do_GET(self):
        parsed = urlparse(self.path)
        path = parsed.path.rstrip("/") or "/"
        qs = parse_qs(parsed.query)

        if path == "/health":
            self._send(200, {"status": "ok", "version": STATE.version, "patient_count": STATE.count()})
            return

        if path == "/patients":
            if STATE.version == "1":
                data = json.dumps(STATE.all_patients()).encode("utf-8")
                self._send_raw(200, data)
                return
            cursor = qs.get("cursor", [None])[0]
            limit = qs.get("limit", [None])[0]
            try:
                with STATE.lock:
                    items, next_cursor, eff_limit = STATE._slice(cursor, limit)
            except ValueError as e:
                self._send(400, {"error": str(e)})
                return
            if STATE.version == "2":
                self._send(200, {"items": items, "next_cursor": next_cursor})
                return
            # 2q: a bare JSON array of at most 100 patients, the way GitHub
            # and GitLab page, with the next page in a Link response header.
            data = json.dumps(items).encode("utf-8")
            extra = []
            if next_cursor is not None:
                host = self.headers.get("Host", "127.0.0.1:8091")
                extra.append(("Link", "<http://{}/patients?cursor={}&limit={}>; rel=\"next\"".format(host, next_cursor, eff_limit)))
            self._send_raw(200, data, extra_headers=extra)
            return

        if path.startswith("/patients/"):
            pid = unquote(path[len("/patients/"):])
            if "/" in pid or pid == "":
                self._send(404, {"error": "No patient with this id"})
                return
            p = STATE.get(pid)
            if p is None:
                self._send(404, {"error": "No patient with this id"})
                return
            self._send(200, p)
            return

        if path == "/stats":
            body = {"patient_count": STATE.count()}
            if STATE.version == "2":
                import datetime
                body["generated_at"] = datetime.datetime.now(datetime.timezone.utc).isoformat()
            self._send(200, body)
            return

        self._send(404, {"error": "not found"})

    def do_POST(self):
        parsed = urlparse(self.path)
        path = parsed.path.rstrip("/") or "/"
        length = int(self.headers.get("Content-Length", "0"))
        raw = self.rfile.read(length) if length > 0 else b""
        if path == "/admin/decommission":
            try:
                body = json.loads(raw.decode("utf-8")) if raw else {}
            except json.JSONDecodeError:
                self._send(400, {"error": "invalid json"})
                return
            token = body.get("confirmation_token") if isinstance(body, dict) else None
            if STATE.decommission(token):
                self._send(200, {"deleted": True, "patient_count": STATE.count()})
            else:
                self._send(400, {"error": "Confirmation token does not match"})
            return
        self._send(404, {"error": "not found"})

    def log_message(self, *args):
        pass


def main():
    global STATE
    parser = argparse.ArgumentParser()
    parser.add_argument("--api-version", "--version", dest="api_version", default=os.environ.get("OLD_SERVICE_VERSION", "1"))
    parser.add_argument("--port", type=int, default=DEFAULT_PORT)
    args = parser.parse_args()
    version = str(args.api_version)
    if version not in ("1", "2", "2q"):
        raise SystemExit("version must be 1, 2, or 2q")
    STATE = State(version)
    httpd = ThreadingHTTPServer(("127.0.0.1", args.port), Handler)
    print("old service v{} listening on http://127.0.0.1:{}/ ({} patients)".format(version, args.port, STATE.count()), flush=True)
    httpd.serve_forever()


if __name__ == "__main__":
    main()
