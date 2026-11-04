#!/usr/bin/env python3
"""Plant the story's L/S memories in Lambo for the with-Doyle run: the
fixture's own lambo_derive call (testdata/legacy/story/memories/lambo.json),
sent over HTTP MCP to the named session only. Prints the ack, never a key.
Usage: lambo-plant.py REPO SESSION_URL"""
import json, sys, urllib.request

repo, url = sys.argv[1], sys.argv[2]
if "lambo-dev" in url or url.rstrip("/").endswith("/mcp"):
    sys.exit("lambo-plant: refusing the lambo-dev session")
call = json.load(open(f"{repo}/testdata/legacy/story/memories/lambo.json"))["request"]
# The fixture types the console procedures "procedure", which this Lambo
# rejects (entity, logic, constraint, resource, observation): sent as logic.
for c in call["params"]["arguments"]["concepts"]:
    if c["concept_type"] == "procedure":
        c["concept_type"] = "logic"
hdr = {"Content-Type": "application/json", "Accept": "application/json, text/event-stream"}


def post(body, sid=None):
    h = dict(hdr)
    if sid:
        h["Mcp-Session-Id"] = sid
    req = urllib.request.Request(url, data=json.dumps(body).encode(), headers=h, method="POST")
    with urllib.request.urlopen(req, timeout=30) as r:
        text = r.read().decode()
        sid = r.headers.get("Mcp-Session-Id") or sid
    for line in text.splitlines():
        if line.startswith("data:"):
            text = line[5:]
    return (json.loads(text) if text.strip() else None), sid


init, sid = post({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
    "protocolVersion": "2025-03-26", "capabilities": {}, "clientInfo": {"name": "t910-plant", "version": "0"}}})
post({"jsonrpc": "2.0", "method": "notifications/initialized"}, sid)
ack, _ = post(call, sid)
print(json.dumps(ack, indent=1)[:2000])
