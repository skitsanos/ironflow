#!/usr/bin/env python3
"""Small stateful MCP 2025-11-25 server used by the documented example and tests."""

import argparse
import json
import os
import subprocess
import sys
import time


TOOLS = [
    {
        "name": "search",
        "description": "Search in the indexed documentation set",
        "inputSchema": {
            "type": "object",
            "properties": {"query": {"type": "string"}},
            "required": ["query"],
        },
    },
    {
        "name": "echo",
        "description": "Return the provided query",
        "inputSchema": {
            "type": "object",
            "properties": {"query": {"type": "string"}},
            "required": ["query"],
        },
    },
]


def emit(message):
    print(json.dumps(message, separators=(",", ":")), flush=True)


def trace(path, **event):
    if not path:
        return
    with open(path, "a", encoding="utf-8") as output:
        output.write(json.dumps({"pid": os.getpid(), **event}) + "\n")


def rpc_error(request_id, code, message):
    return {
        "jsonrpc": "2.0",
        "id": request_id,
        "error": {"code": code, "message": message},
    }


def initialize_response(request, mode):
    request_id = request.get("id")
    if mode == "wrong-id":
        request_id = "not-the-request-id"
    protocol_version = (
        "2099-01-01" if mode == "unsupported-version" else "2025-11-25"
    )
    return {
        "jsonrpc": "1.0" if mode == "wrong-jsonrpc" else "2.0",
        "id": request_id,
        "result": {
            "protocolVersion": protocol_version,
            "capabilities": {"tools": {"listChanged": False}},
            "serverInfo": {
                "name": "ironflow-mcp-mock",
                "version": "1.0.0",
            },
        },
    }


def call_tool_response(request):
    params = request.get("params", {})
    name = params.get("name")
    query = params.get("arguments", {}).get("query", "")
    if name == "search":
        text = f"Search result for '{query}' is available in the IronFlow docs."
    elif name == "echo":
        text = f"Echo: {query}"
    else:
        return rpc_error(request.get("id"), -32601, f"Unknown tool '{name}'")
    return {
        "jsonrpc": "2.0",
        "id": request.get("id"),
        "result": {
            "content": [{"type": "text", "text": text}],
            "isError": False,
        },
    }


def parse_args():
    parser = argparse.ArgumentParser()
    parser.add_argument("--trace")
    parser.add_argument(
        "--mode",
        choices=[
            "normal",
            "wrong-id",
            "wrong-list-id",
            "wrong-jsonrpc",
            "unsupported-version",
            "result-and-error",
            "interleaved",
            "slow-call",
        ],
        default="normal",
    )
    parser.add_argument("--delay", type=float, default=1.0)
    parser.add_argument("--parent-pid-file")
    parser.add_argument("--child-pid-file")
    return parser.parse_args()


def main():
    args = parse_args()
    initialized = False
    ready = False
    child = None
    if args.parent_pid_file:
        with open(args.parent_pid_file, "w", encoding="utf-8") as output:
            output.write(str(os.getpid()))
    if args.child_pid_file:
        child = subprocess.Popen(
            ["sh", "-c", "trap '' TERM; while :; do sleep 1; done"],
            stdin=subprocess.DEVNULL,
        )
        with open(args.child_pid_file, "w", encoding="utf-8") as output:
            output.write(str(child.pid))

    for line in sys.stdin:
        request = json.loads(line)
        method = request.get("method")
        trace(args.trace, method=method, id=request.get("id"))

        if method == "initialize":
            if initialized:
                emit(rpc_error(request.get("id"), -32600, "Already initialized"))
                continue
            initialized = True
            response = initialize_response(request, args.mode)
            if args.mode == "result-and-error":
                response["error"] = {"code": -32000, "message": "ambiguous"}
            if args.mode == "wrong-jsonrpc":
                sys.stdout.write(
                    json.dumps(response) + "\n"
                )
                sys.stdout.flush()
            else:
                emit(response)
        elif method == "notifications/initialized":
            ready = initialized
        elif method == "tools/list":
            if not ready:
                emit(rpc_error(request.get("id"), -32002, "Session not initialized"))
                continue
            if args.mode == "interleaved":
                emit(
                    {
                        "jsonrpc": "2.0",
                        "method": "notifications/message",
                        "params": {"level": "info", "data": "listing tools"},
                    }
                )
                emit({"jsonrpc": "2.0", "id": "server-ping", "method": "ping"})
            emit(
                {
                    "jsonrpc": "2.0",
                    "id": (
                        "not-the-list-request-id"
                        if args.mode == "wrong-list-id"
                        else request.get("id")
                    ),
                    "result": {"tools": TOOLS},
                }
            )
        elif method == "tools/call":
            if not ready:
                emit(rpc_error(request.get("id"), -32002, "Session not initialized"))
                continue
            if args.mode == "slow-call":
                time.sleep(args.delay)
            emit(call_tool_response(request))
        elif method == "notifications/cancelled":
            continue
        elif method is None and request.get("id") == "server-ping":
            continue
        else:
            emit(rpc_error(request.get("id"), -32601, f"Unknown method '{method}'"))

    trace(args.trace, event="eof")
    if child is not None:
        child.wait()


if __name__ == "__main__":
    main()
