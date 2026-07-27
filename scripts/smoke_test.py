#!/usr/bin/env python3
"""
Smoke test end-to-end de rust-mcp-server.

Hace el handshake JSON-RPC completo:
  1. initialize
  2. notifications/initialized
  3. tools/list
  4. tools/call list_loaded_projects
  5. shutdown

Uso:
  ./scripts/smoke_test.py [<ruta al binario>]
  ./scripts/smoke_test.py    # usa target/release/rust-mcp-server
"""

import json
import os
import subprocess
import sys
from pathlib import Path


def main() -> int:
    if len(sys.argv) > 1:
        binary = Path(sys.argv[1])
    else:
        binary = (
            Path(__file__).resolve().parent.parent / "target/release/rust-mcp-server"
        )

    if not binary.exists():
        print(f"ERROR: binary not found at {binary}", file=sys.stderr)
        print("Build it first: cargo build --release", file=sys.stderr)
        return 1

    proc = subprocess.Popen(
        [str(binary)],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        bufsize=0,
        env={**os.environ, "RUST_MCP_LOG_LEVEL": "warn"},
    )

    def send(msg: dict) -> None:
        line = json.dumps(msg)
        print(f">>> {line}", file=sys.stderr)
        assert proc.stdin is not None
        proc.stdin.write(line + "\n")
        proc.stdin.flush()

    def recv() -> dict:
        assert proc.stdout is not None
        line = proc.stdout.readline()
        if not line:
            stderr = proc.stderr.read() if proc.stderr else ""
            raise RuntimeError(f"no response, stderr:\n{stderr}")
        print(f"<<< {line.rstrip()}", file=sys.stderr)
        return json.loads(line)

    # 1) initialize
    send(
        {
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2025-11-25",
                "capabilities": {},
                "clientInfo": {"name": "smoke-test", "version": "0.1.0"},
            },
        }
    )
    init = recv()
    assert init.get("result", {}).get("serverInfo", {}).get("name") == "rust-mcp-server", (
        f"unexpected server name: {init}"
    )
    print("✓ initialize OK")

    # 2) initialized notification (no response)
    send({"jsonrpc": "2.0", "method": "notifications/initialized", "params": {}})

    # 3) tools/list
    send({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}})
    tools = recv()
    tool_names = [t["name"] for t in tools.get("result", {}).get("tools", [])]
    expected_tools = [
        "list_loaded_projects",
        "load_rust_project",
        "unload_rust_project",
        "project_metadata",
        "inspect_build_config",
        "list_crates",
        "list_dependencies",
        "list_cargo_features",
        "program_summary",
        "get_file_content",
        "search_source",
        "list_functions",
        "list_structs",
        "list_enums",
        "list_traits",
        "list_impls",
        "list_modules",
        "list_macros",
        "inspect_function",
        "inspect_struct",
        "inspect_enum",
        "inspect_trait",
        "validate_code_reference",
        "find_references",
        "function_callers",
        "analysis_summary",
        "resolve_type",
        "find_implementations",
        "get_type_hierarchy",
        "function_call_graph",
        "struct_field_xref",
        "find_unsafe_blocks",
        "find_unwrap_patterns",
        "find_unchecked_patterns",
        "find_panic_sites",
        "find_dead_code",
        "find_unused_dependencies",
    ]
    for t in expected_tools:
        assert t in tool_names, f"expected tool missing: {t}"
    print(f"✓ tools/list OK ({len(tool_names)} tool(s), {len(expected_tools)} expected present)")

    # 4) Load the fixture crate
    fixture = os.path.join(
        os.path.dirname(os.path.abspath(__file__)),
        "..",
        "tests",
        "fixtures",
        "sample_crate",
    )
    send(
        {
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "load_rust_project",
                "arguments": {"source": os.path.abspath(fixture), "alias": "sample"},
            },
        }
    )
    r = recv()
    assert "Project Loaded" in r.get("result", {}).get("content", [{}])[0].get("text", ""), r
    print("✓ load_rust_project OK")

    # 5) program_summary
    send(
        {
            "jsonrpc": "2.0",
            "id": 4,
            "method": "tools/call",
            "params": {"name": "program_summary", "arguments": {"name": "sample"}},
        }
    )
    r = recv()
    text = r["result"]["content"][0]["text"]
    assert "sample_crate" in text, f"unexpected summary: {text[:200]}"
    assert "Functions" in text, f"unexpected summary: {text[:200]}"
    print("✓ program_summary OK")

    # 6) list_functions
    send(
        {
            "jsonrpc": "2.0",
            "id": 5,
            "method": "tools/call",
            "params": {"name": "list_functions", "arguments": {"name": "sample"}},
        }
    )
    r = recv()
    text = r["result"]["content"][0]["text"]
    assert "add" in text and "multiply" in text, f"unexpected functions: {text[:300]}"
    print("✓ list_functions OK")

    # 7) inspect_function
    send(
        {
            "jsonrpc": "2.0",
            "id": 6,
            "method": "tools/call",
            "params": {
                "name": "inspect_function",
                "arguments": {"name": "sample", "function_name": "add"},
            },
        }
    )
    r = recv()
    text = r["result"]["content"][0]["text"]
    assert "fn add" in text, f"unexpected function body: {text[:300]}"
    print("✓ inspect_function OK")

    # 8) validate_code_reference (positive)
    send(
        {
            "jsonrpc": "2.0",
            "id": 7,
            "method": "tools/call",
            "params": {
                "name": "validate_code_reference",
                "arguments": {"name": "sample", "symbol": "add"},
            },
        }
    )
    r = recv()
    text = r["result"]["content"][0]["text"]
    assert "EXISTS" in text, f"unexpected: {text[:200]}"
    print("✓ validate_code_reference (EXISTS) OK")

    # 9) find_references
    send(
        {
            "jsonrpc": "2.0",
            "id": 8,
            "method": "tools/call",
            "params": {
                "name": "find_references",
                "arguments": {"name": "sample", "symbol": "add"},
            },
        }
    )
    r = recv()
    text = r["result"]["content"][0]["text"]
    print(f"✓ find_references OK ({len(text)} bytes)")

    # 10) unload
    send(
        {
            "jsonrpc": "2.0",
            "id": 9,
            "method": "tools/call",
            "params": {"name": "unload_rust_project", "arguments": {"name": "sample"}},
        }
    )
    r = recv()
    text = r["result"]["content"][0]["text"]
    assert "Project Unloaded" in text, f"unexpected: {text[:200]}"
    print("✓ unload_rust_project OK")

    # 11) Reload + analysis_summary (ra_ap_* integration smoke test)
    send(
        {
            "jsonrpc": "2.0",
            "id": 10,
            "method": "tools/call",
            "params": {
                "name": "load_rust_project",
                "arguments": {"source": os.path.abspath(fixture), "alias": "sample"},
            },
        }
    )
    recv()
    send(
        {
            "jsonrpc": "2.0",
            "id": 11,
            "method": "tools/call",
            "params": {"name": "analysis_summary", "arguments": {"name": "sample"}},
        }
    )
    r = recv()
    text = r["result"]["content"][0]["text"]
    assert "rust-analyzer" in text, f"unexpected: {text[:200]}"
    assert "lib.rs" in text, f"unexpected: {text[:200]}"
    assert "ASG Summary" in text, f"unexpected: {text[:200]}"
    print("✓ analysis_summary (ra_ap_*) OK")

    # 12) resolve_type (iter 3.5 — semantic query)
    send(
        {
            "jsonrpc": "2.0",
            "id": 13,
            "method": "tools/call",
            "params": {
                "name": "resolve_type",
                "arguments": {"name": "sample", "symbol": "add"},
            },
        }
    )
    r = recv()
    text = r["result"]["content"][0]["text"]
    assert "Definition of `add`" in text, f"unexpected: {text[:300]}"
    print(f"✓ resolve_type (ra_ap_* semantic) OK")

    # 13) find_implementations (iter 3.6)
    send(
        {
            "jsonrpc": "2.0",
            "id": 14,
            "method": "tools/call",
            "params": {
                "name": "find_implementations",
                "arguments": {"name": "sample", "symbol": "add"},
            },
        }
    )
    r = recv()
    text = r["result"]["content"][0]["text"]
    assert "Implementations of `add`" in text, f"unexpected: {text[:300]}"
    print(f"✓ find_implementations (ra_ap_* semantic) OK")

    # 14) get_type_hierarchy (iter 3.6)
    # `add` es una función, no un trait/type — el tool debe responder
    # con un "no type hierarchy info" honesto, no colgar.
    send(
        {
            "jsonrpc": "2.0",
            "id": 15,
            "method": "tools/call",
            "params": {
                "name": "get_type_hierarchy",
                "arguments": {"name": "sample", "symbol": "add"},
            },
        }
    )
    r = recv()
    text = r["result"]["content"][0]["text"]
    assert "Type hierarchy" in text or "Error" in text, f"unexpected: {text[:300]}"
    print(f"✓ get_type_hierarchy (ra_ap_* semantic) OK")

    # 15) function_call_graph (iter 3.6 — semantic callers)
    send(
        {
            "jsonrpc": "2.0",
            "id": 16,
            "method": "tools/call",
            "params": {
                "name": "function_call_graph",
                "arguments": {"name": "sample", "symbol": "add"},
            },
        }
    )
    r = recv()
    text = r["result"]["content"][0]["text"]
    assert "Callers of `add`" in text, f"unexpected: {text[:300]}"
    print(f"✓ function_call_graph (ra_ap_* semantic) OK")

    # 16) struct_field_xref (iter 3.6)
    send(
        {
            "jsonrpc": "2.0",
            "id": 17,
            "method": "tools/call",
            "params": {
                "name": "struct_field_xref",
                "arguments": {"name": "sample", "field": "add"},
            },
        }
    )
    r = recv()
    text = r["result"]["content"][0]["text"]
    assert "References to field `add`" in text, f"unexpected: {text[:300]}"
    print(f"✓ struct_field_xref (ra_ap_* semantic) OK")

    # 17) Bug-hunt: find_unsafe_blocks (iter 4)
    send(
        {
            "jsonrpc": "2.0",
            "id": 18,
            "method": "tools/call",
            "params": {
                "name": "find_unsafe_blocks",
                "arguments": {"name": "sample"},
            },
        }
    )
    r = recv()
    text = r["result"]["content"][0]["text"]
    assert "Unsafe blocks" in text, f"unexpected: {text[:300]}"
    print(f"✓ find_unsafe_blocks (iter 4 bug-hunt) OK")

    # 18) Bug-hunt: find_panic_sites (iter 4)
    send(
        {
            "jsonrpc": "2.0",
            "id": 19,
            "method": "tools/call",
            "params": {
                "name": "find_panic_sites",
                "arguments": {"name": "sample"},
            },
        }
    )
    r = recv()
    text = r["result"]["content"][0]["text"]
    assert "Panic sites" in text, f"unexpected: {text[:300]}"
    print(f"✓ find_panic_sites (iter 4 bug-hunt) OK")

    # 19) Bug-hunt: find_dead_code (iter 4)
    send(
        {
            "jsonrpc": "2.0",
            "id": 20,
            "method": "tools/call",
            "params": {
                "name": "find_dead_code",
                "arguments": {"name": "sample"},
            },
        }
    )
    r = recv()
    text = r["result"]["content"][0]["text"]
    assert "Dead code" in text, f"unexpected: {text[:300]}"
    print(f"✓ find_dead_code (iter 4 bug-hunt) OK")

    send(
        {
            "jsonrpc": "2.0",
            "id": 12,
            "method": "tools/call",
            "params": {"name": "unload_rust_project", "arguments": {"name": "sample"}},
        }
    )
    recv()

    # 5) shutdown
    assert proc.stdin is not None
    proc.stdin.close()
    rc = proc.wait(timeout=5)
    assert rc == 0, f"non-zero exit: {rc}"
    print(f"✓ clean shutdown (rc={rc})")

    print("\n✅ All smoke tests passed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
