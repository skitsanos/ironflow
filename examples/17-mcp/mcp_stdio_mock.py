#!/usr/bin/env python3
import json
import sys


def read_request():
    payload = (sys.stdin.readline() or "").strip()
    return json.loads(payload or "{}")


def emit_initialize():
    return {
        "jsonrpc": "2.0",
        "id": "mock-init",
        "result": {
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": {
                    "listChanged": False
                }
            },
            "serverInfo": {
                "name": "ironflow-mcp-mock",
                "version": "0.1.0"
            }
        }
    }


def emit_tools():
    return {
        "jsonrpc": "2.0",
        "id": "mock-tools",
        "result": {
            "tools": [
                {
                    "name": "search",
                    "description": "Search in the indexed documentation set",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "query": {"type": "string"}
                        },
                        "required": ["query"],
                    },
                },
                {
                    "name": "echo",
                    "description": "Return the provided query",
                    "inputSchema": {
                        "type": "object",
                        "properties": {
                            "query": {"type": "string"}
                        },
                        "required": ["query"],
                    },
                },
            ]
        }
    }


def emit_call(request):
    name = request.get("params", {}).get("name")
    query = request.get("params", {}).get("arguments", {}).get("query", "")

    if name == "search":
        text = f"Search result for '{query}' is available in the IronFlow docs."
    elif name == "echo":
        text = f"Echo: {query}"
    else:
        return {
            "jsonrpc": "2.0",
            "id": request.get("id"),
            "error": {
                "code": -32601,
                "message": f"Unknown tool '{name}'",
            },
        }

    return {
        "jsonrpc": "2.0",
        "id": request.get("id"),
        "result": {
            "content": [
                {
                    "type": "text",
                    "text": text
                }
            ],
            "isError": False
        }
    }


def main():
    request = read_request()
    method = request.get("method", "")

    if method == "initialize":
        response = emit_initialize()
    elif method == "tools/list":
        response = emit_tools()
    elif method == "tools/call":
        response = emit_call(request)
    else:
        response = {
            "jsonrpc": "2.0",
            "id": request.get("id"),
            "error": {
                "code": -32601,
                "message": f"Unknown method '{method}'",
            }
        }

    print(json.dumps(response))


if __name__ == "__main__":
    main()
