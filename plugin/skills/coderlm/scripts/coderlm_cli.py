#!/usr/bin/env python3
"""CLI wrapper for the coderlm-server API.

Manages session state and provides clean commands for codebase exploration.
State is cached relative to cwd (default: .claude/coderlm_state/session.json).
Override with CODERLM_STATE_DIR env var.

Quick start:
  python3 coderlm_cli.py init
  python3 coderlm_cli.py search "handler"
  python3 coderlm_cli.py impl handler --file src/server/routes.rs
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path

STATE_DIR = Path(os.environ.get("CODERLM_STATE_DIR", ".claude/coderlm_state"))
STATE_FILE = STATE_DIR / "session.json"


def _load_state() -> dict:
    if not STATE_FILE.exists():
        return {}
    with STATE_FILE.open() as f:
        return json.load(f)


def _save_state(state: dict) -> None:
    STATE_DIR.mkdir(parents=True, exist_ok=True)
    with STATE_FILE.open("w") as f:
        json.dump(state, f, indent=2)


def _clear_state() -> None:
    if STATE_FILE.exists():
        STATE_FILE.unlink()


def _base_url(state: dict) -> str:
    host = state.get("host", "127.0.0.1")
    port = state.get("port", 3000)
    return f"http://{host}:{port}/api/v1"


def _session_id(state: dict) -> str:
    sid = state.get("session_id")
    if not sid:
        _die("No active session. Run: coderlm_cli.py init")
    return sid


def _die(msg: str) -> None:
    print(f"ERROR: {msg}", file=sys.stderr)
    sys.exit(1)


def _resolve_file(state: dict, symbol_name: str) -> str:
    """Auto-resolve a symbol to its file by searching the index.

    If exactly one match is found, returns the file path.
    If multiple matches, prints them and exits with guidance.
    If no matches, exits with an error.
    """
    results = _get(state, "/symbols/search", {"q": symbol_name, "limit": 50})
    symbols = results.get("symbols", [])

    if not symbols:
        _die(
            f"Symbol '{symbol_name}' not found in index.\n"
            f"  Check the name with: coderlm_cli.py search \"{symbol_name}\"\n"
            f"  Or list all symbols: coderlm_cli.py symbols --limit 200"
        )

    # Exact matches first
    exact = [s for s in symbols if s["name"] == symbol_name]
    if len(exact) == 1:
        return exact[0]["file"]
    if len(exact) > 1:
        lines = "\n".join(f"  {s['file']}  (line {s['line_range'][0]})" for s in exact)
        _die(
            f"Symbol '{symbol_name}' found in multiple files. Specify --file:\n{lines}"
        )

    # No exact match — show closest results
    lines = "\n".join(
        f"  {s['name']}  in  {s['file']}  (line {s['line_range'][0]})"
        for s in symbols[:10]
    )
    _die(
        f"No exact match for '{symbol_name}'. Closest results:\n{lines}\n"
        f"  Use the correct name and --file flag."
    )


def _request(
    method: str,
    url: str,
    data: dict | None = None,
    headers: dict | None = None,
    timeout: int = 30,
) -> dict:
    hdrs = headers or {}
    body = None
    if data is not None:
        body = json.dumps(data).encode("utf-8")
        hdrs["Content-Type"] = "application/json"

    req = urllib.request.Request(url, data=body, headers=hdrs, method=method)
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            raw = resp.read().decode("utf-8")
            return json.loads(raw) if raw.strip() else {}
    except urllib.error.HTTPError as e:
        body_text = e.read().decode("utf-8", errors="replace")
        try:
            err = json.loads(body_text)
        except json.JSONDecodeError:
            err = {"error": body_text, "status": e.code}

        if e.code == 410:
            print(
                "ERROR: Project was evicted from server. Run: coderlm_cli.py init",
                file=sys.stderr,
            )
            _clear_state()
            sys.exit(1)

        # Pretty-print the error with the status code
        detail = err.get("error", err.get("message", json.dumps(err, indent=2)))
        print(f"ERROR [{e.code}]: {detail}", file=sys.stderr)
        sys.exit(1)
    except urllib.error.URLError as e:
        _die(
            f"Cannot connect to coderlm-server ({e.reason}).\n"
            f"  Start the server: coderlm-server serve\n"
            f"  Then run: coderlm_cli.py init"
        )


def _get(state: dict, path: str, params: dict | None = None) -> dict:
    base = _base_url(state)
    url = f"{base}{path}"
    if params:
        clean = {k: v for k, v in params.items() if v is not None}
        if clean:
            url += "?" + urllib.parse.urlencode(clean)
    return _request("GET", url, headers={"X-Session-Id": _session_id(state)})


def _post(state: dict, path: str, data: dict) -> dict:
    base = _base_url(state)
    url = f"{base}{path}"
    return _request("POST", url, data=data, headers={"X-Session-Id": _session_id(state)})


def _output(result: dict) -> None:
    print(json.dumps(result, indent=2))


# ── Commands ──────────────────────────────────────────────────────────


def cmd_init(args: argparse.Namespace) -> None:
    cwd = os.path.abspath(args.cwd or os.getcwd())
    host = args.host or "127.0.0.1"
    port = args.port or 3000
    base = f"http://{host}:{port}/api/v1"

    try:
        health = _request("GET", f"{base}/health")
    except SystemExit:
        return

    result = _request("POST", f"{base}/sessions", data={"cwd": cwd})
    state = {
        "session_id": result["session_id"],
        "host": host,
        "port": port,
        "project": cwd,
        "created_at": result.get("created_at", ""),
    }
    _save_state(state)

    print(f"Session created: {result['session_id']}")
    print(f"Project: {cwd}")
    print(f"Server: {health.get('status', 'ok')} "
          f"({health.get('projects', 0)} projects, "
          f"{health.get('active_sessions', 0)} sessions)")


def cmd_status(args: argparse.Namespace) -> None:
    state = _load_state()
    if not state:
        host = args.host or "127.0.0.1"
        port = args.port or 3000
        base = f"http://{host}:{port}/api/v1"
        result = _request("GET", f"{base}/health")
        _output(result)
        return

    base = _base_url(state)
    health = _request("GET", f"{base}/health")
    info = {"server": health, "session": state}

    sid = state.get("session_id")
    if sid:
        try:
            session_info = _request("GET", f"{base}/sessions/{sid}")
            info["session_details"] = session_info
        except SystemExit:
            info["session_details"] = "session may have expired"

    _output(info)


def cmd_structure(args: argparse.Namespace) -> None:
    state = _load_state()
    params = {}
    if args.depth is not None:
        params["depth"] = args.depth
    _output(_get(state, "/structure", params))


def cmd_symbols(args: argparse.Namespace) -> None:
    state = _load_state()
    params = {}
    if args.kind:
        params["kind"] = args.kind
    if args.file:
        params["file"] = args.file
    if args.limit is not None:
        params["limit"] = args.limit
    if args.cursor:
        params["cursor"] = args.cursor
    _output(_get(state, "/symbols", params))


def cmd_search(args: argparse.Namespace) -> None:
    state = _load_state()
    params = {"q": args.query}
    if args.limit is not None:
        params["limit"] = args.limit
    if args.cursor:
        params["cursor"] = args.cursor
    _output(_get(state, "/symbols/search", params))


def cmd_impl(args: argparse.Namespace) -> None:
    state = _load_state()
    file = args.file
    if not file:
        file = _resolve_file(state, args.symbol)
    params = {"symbol": args.symbol, "file": file}
    _output(_get(state, "/symbols/implementation", params))


def cmd_callers(args: argparse.Namespace) -> None:
    state = _load_state()
    file = args.file
    if not file:
        file = _resolve_file(state, args.symbol)
    params = {"symbol": args.symbol, "file": file}
    if args.limit is not None:
        params["limit"] = args.limit
    _output(_get(state, "/symbols/callers", params))


def cmd_tests(args: argparse.Namespace) -> None:
    state = _load_state()
    file = args.file
    if not file:
        file = _resolve_file(state, args.symbol)
    params = {"symbol": args.symbol, "file": file}
    if args.limit is not None:
        params["limit"] = args.limit
    _output(_get(state, "/symbols/tests", params))


def cmd_variables(args: argparse.Namespace) -> None:
    state = _load_state()
    file = args.file
    if not file:
        file = _resolve_file(state, args.function)
    params = {"function": args.function, "file": file}
    _output(_get(state, "/symbols/variables", params))


def cmd_peek(args: argparse.Namespace) -> None:
    state = _load_state()
    params: dict = {"file": args.file}

    # --line N reads a single line (1-indexed, human-friendly)
    if args.line is not None:
        params["start"] = args.line - 1
        params["end"] = args.line
    else:
        # --start/--end are 0-indexed (API-native)
        if args.start is not None:
            params["start"] = args.start
        if args.end is not None:
            params["end"] = args.end

    _output(_get(state, "/peek", params))


def cmd_grep(args: argparse.Namespace) -> None:
    state = _load_state()
    params = {"pattern": args.pattern}
    if args.max_matches is not None:
        params["max_matches"] = args.max_matches
    if args.context_lines is not None:
        params["context_lines"] = args.context_lines
    if args.scope is not None:
        params["scope"] = args.scope
    _output(_get(state, "/grep", params))


def cmd_chunks(args: argparse.Namespace) -> None:
    state = _load_state()
    params = {"file": args.file}
    if args.size is not None:
        params["size"] = args.size
    if args.overlap is not None:
        params["overlap"] = args.overlap
    _output(_get(state, "/chunk_indices", params))


def cmd_define_file(args: argparse.Namespace) -> None:
    state = _load_state()
    _output(_post(state, "/structure/define", {
        "file": args.file,
        "definition": args.definition,
    }))


def cmd_redefine_file(args: argparse.Namespace) -> None:
    state = _load_state()
    _output(_post(state, "/structure/redefine", {
        "file": args.file,
        "definition": args.definition,
    }))


def cmd_define_symbol(args: argparse.Namespace) -> None:
    state = _load_state()
    _output(_post(state, "/symbols/define", {
        "symbol": args.symbol,
        "file": args.file,
        "definition": args.definition,
    }))


def cmd_redefine_symbol(args: argparse.Namespace) -> None:
    state = _load_state()
    _output(_post(state, "/symbols/redefine", {
        "symbol": args.symbol,
        "file": args.file,
        "definition": args.definition,
    }))


def cmd_mark(args: argparse.Namespace) -> None:
    state = _load_state()
    _output(_post(state, "/structure/mark", {
        "file": args.file,
        "mark": args.type,
    }))


def cmd_history(args: argparse.Namespace) -> None:
    state = _load_state()
    params = {}
    if args.limit is not None:
        params["limit"] = args.limit
    _output(_get(state, "/history", params))


def cmd_save_annotations(args: argparse.Namespace) -> None:
    state = _load_state()
    _output(_post(state, "/annotations/save", {}))


def cmd_load_annotations(args: argparse.Namespace) -> None:
    state = _load_state()
    _output(_post(state, "/annotations/load", {}))


def cmd_cleanup(args: argparse.Namespace) -> None:
    state = _load_state()
    if not state.get("session_id"):
        print("No active session.")
        return

    base = _base_url(state)
    sid = state["session_id"]
    _request("DELETE", f"{base}/sessions/{sid}")
    _clear_state()
    print(f"Session {sid} deleted.")


def cmd_stats(args: argparse.Namespace) -> None:
    """Get server and project stats without requiring a session."""
    host = args.host or "127.0.0.1"
    port = args.port or 3000
    base = f"http://{host}:{port}/api/v1"
    _output(_request("GET", f"{base}/stats"))


# ── Parser ────────────────────────────────────────────────────────────


def build_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(
        prog="coderlm_cli",
        description="CLI wrapper for coderlm-server API",
    )

    sub = p.add_subparsers(dest="cmd", required=True)

    # init
    p_init = sub.add_parser(
        "init",
        help="Create a session for the current project",
        description="Index the project and create a session. Run once per project.",
    )
    p_init.add_argument("--cwd", help="Project directory (default: $PWD)")
    p_init.add_argument("--host", default=None, help="Server host (default: 127.0.0.1)")
    p_init.add_argument("--port", type=int, default=None, help="Server port (default: 3000)")
    p_init.set_defaults(func=cmd_init)

    # status
    p_status = sub.add_parser("status", help="Show server and session status")
    p_status.add_argument("--host", default=None)
    p_status.add_argument("--port", type=int, default=None)
    p_status.set_defaults(func=cmd_status)

    # structure
    p_struct = sub.add_parser(
        "structure",
        help="Get project file tree",
        description="Show the project directory tree with file counts and language breakdown.",
    )
    p_struct.add_argument("--depth", type=int, default=None, help="Tree depth (0=unlimited)")
    p_struct.set_defaults(func=cmd_structure)

    # symbols
    p_sym = sub.add_parser(
        "symbols",
        help="List symbols",
        description="List symbols with optional kind/file filters. Supports pagination.",
    )
    p_sym.add_argument("--kind", help="Filter: function, method, class, struct, enum, trait, interface, constant, type, module")
    p_sym.add_argument("--file", help="Filter by file path (relative to project root)")
    p_sym.add_argument("--limit", type=int, default=None)
    p_sym.add_argument("--cursor", help="Pagination cursor from previous response next_cursor field")
    p_sym.set_defaults(func=cmd_symbols)

    # search
    p_search = sub.add_parser(
        "search",
        help="Search symbols by name (fuzzy)",
        description="Fuzzy-search symbol names. Returns ranked results. Use the 'file' field from results with impl/callers/tests.",
    )
    p_search.add_argument("query", help="Search term")
    p_search.add_argument("--limit", type=int, default=None)
    p_search.add_argument("--cursor", help="Pagination cursor from previous response next_cursor field")
    p_search.set_defaults(func=cmd_search)

    # impl
    p_impl = sub.add_parser(
        "impl",
        help="Get full source of a symbol (byte-exact)",
        description=(
            "Return the exact source code of a symbol.\n"
            "If --file is omitted, auto-resolves via search (exact match required).\n"
            "Example: impl handle_request --file src/server/routes.rs\n"
            "         impl handle_request  (auto-resolves if unique)"
        ),
    )
    p_impl.add_argument("symbol", help="Symbol name")
    p_impl.add_argument("--file", default=None, help="File containing the symbol (auto-resolved if omitted)")
    p_impl.set_defaults(func=cmd_impl)

    # callers
    p_callers = sub.add_parser(
        "callers",
        help="Find call sites for a symbol",
        description=(
            "Find every place that calls this symbol.\n"
            "If --file is omitted, auto-resolves via search.\n"
            "Example: callers run_server --file src/main.rs"
        ),
    )
    p_callers.add_argument("symbol", help="Symbol name")
    p_callers.add_argument("--file", default=None, help="File containing the symbol (auto-resolved if omitted)")
    p_callers.add_argument("--limit", type=int, default=None)
    p_callers.set_defaults(func=cmd_callers)

    # tests
    p_tests = sub.add_parser(
        "tests",
        help="Find tests referencing a symbol",
        description=(
            "Find test functions that reference this symbol.\n"
            "If --file is omitted, auto-resolves via search.\n"
            "Example: tests scan_directory --file src/index/walker.rs"
        ),
    )
    p_tests.add_argument("symbol", help="Symbol name")
    p_tests.add_argument("--file", default=None, help="File containing the symbol (auto-resolved if omitted)")
    p_tests.add_argument("--limit", type=int, default=None)
    p_tests.set_defaults(func=cmd_tests)

    # variables
    p_vars = sub.add_parser(
        "variables",
        help="List local variables in a function",
        description=(
            "List all local variable names in a function.\n"
            "If --file is omitted, auto-resolves via search.\n"
            "Example: variables scan_directory --file src/index/walker.rs"
        ),
    )
    p_vars.add_argument("function", help="Function name")
    p_vars.add_argument("--file", default=None, help="File containing the function (auto-resolved if omitted)")
    p_vars.set_defaults(func=cmd_variables)

    # peek
    p_peek = sub.add_parser(
        "peek",
        help="Read a line range from a file",
        description=(
            "Read specific lines from a file. Two modes:\n"
            "  1-indexed (human-friendly): --line 42        reads line 42 only\n"
            "  0-indexed (API-native):     --start 0 --end 50  reads lines 0-49\n"
            "Examples:\n"
            "  peek src/main.rs --line 42\n"
            "  peek src/main.rs --start 10 --end 30"
        ),
    )
    p_peek.add_argument("file", help="File path (relative to project root)")
    p_peek.add_argument("--line", type=int, default=None, help="Read a single line (1-indexed)")
    p_peek.add_argument("--start", type=int, default=None, help="Start line (0-indexed)")
    p_peek.add_argument("--end", type=int, default=None, help="End line (exclusive, 0-indexed)")
    p_peek.set_defaults(func=cmd_peek)

    # grep
    p_grep = sub.add_parser(
        "grep",
        help="Regex search across all indexed files",
        description="Search for a pattern across all project files. Scope-aware: --scope code skips comments and strings.",
    )
    p_grep.add_argument("pattern", help="Regex pattern")
    p_grep.add_argument("--max-matches", type=int, default=None)
    p_grep.add_argument("--context-lines", type=int, default=None)
    p_grep.add_argument("--scope", choices=["all", "code"], default=None,
                         help="Scope filter: 'all' (default) or 'code' (skip comments/strings)")
    p_grep.set_defaults(func=cmd_grep)

    # chunks
    p_chunks = sub.add_parser("chunks", help="Compute chunk boundaries for a file")
    p_chunks.add_argument("file", help="File path")
    p_chunks.add_argument("--size", type=int, default=None, help="Chunk size in bytes")
    p_chunks.add_argument("--overlap", type=int, default=None, help="Overlap between chunks")
    p_chunks.set_defaults(func=cmd_chunks)

    # define-file
    p_dfile = sub.add_parser("define-file", help="Set a description for a file")
    p_dfile.add_argument("file", help="File path")
    p_dfile.add_argument("definition", help="Human-readable description")
    p_dfile.set_defaults(func=cmd_define_file)

    # redefine-file
    p_rdfile = sub.add_parser("redefine-file", help="Update a file description")
    p_rdfile.add_argument("file", help="File path")
    p_rdfile.add_argument("definition", help="Updated description")
    p_rdfile.set_defaults(func=cmd_redefine_file)

    # define-symbol
    p_dsym = sub.add_parser("define-symbol", help="Set a description for a symbol")
    p_dsym.add_argument("symbol", help="Symbol name")
    p_dsym.add_argument("--file", required=True, help="File containing the symbol")
    p_dsym.add_argument("definition", help="Human-readable description")
    p_dsym.set_defaults(func=cmd_define_symbol)

    # redefine-symbol
    p_rdsym = sub.add_parser("redefine-symbol", help="Update a symbol description")
    p_rdsym.add_argument("symbol", help="Symbol name")
    p_rdsym.add_argument("--file", required=True, help="File containing the symbol")
    p_rdsym.add_argument("definition", help="Updated description")
    p_rdsym.set_defaults(func=cmd_redefine_symbol)

    # mark
    p_mark = sub.add_parser("mark", help="Tag a file with a category")
    p_mark.add_argument("file", help="File path")
    p_mark.add_argument("type", choices=["documentation", "ignore", "test", "config", "generated", "custom"],
                         help="Mark type")
    p_mark.set_defaults(func=cmd_mark)

    # history
    p_hist = sub.add_parser("history", help="Session command history")
    p_hist.add_argument("--limit", type=int, default=None)
    p_hist.set_defaults(func=cmd_history)

    # save-annotations
    p_save = sub.add_parser("save-annotations", help="Save annotations to disk (.coderlm/annotations.json)")
    p_save.set_defaults(func=cmd_save_annotations)

    # load-annotations
    p_load = sub.add_parser("load-annotations", help="Load annotations from disk")
    p_load.set_defaults(func=cmd_load_annotations)

    # cleanup
    p_clean = sub.add_parser("cleanup", help="Delete the current session")
    p_clean.set_defaults(func=cmd_cleanup)

    # stats
    p_stats = sub.add_parser("stats", help="Show server stats (projects, cache hit rates, uptime)")
    p_stats.add_argument("--host", default=None)
    p_stats.add_argument("--port", type=int, default=None)
    p_stats.set_defaults(func=cmd_stats)

    return p


def main() -> None:
    parser = build_parser()
    args = parser.parse_args()
    args.func(args)


if __name__ == "__main__":
    main()
