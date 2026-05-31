"""HTTP server for the resource Tracker update workflow."""

from __future__ import annotations

import argparse
import json
import threading
import time
from http import HTTPStatus
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import Any
from urllib.parse import parse_qs, urlparse

from .registry import TrackerRegistry, TrackerValidationError


class TrackerHTTPServer(ThreadingHTTPServer):
    daemon_threads = True

    def __init__(
        self,
        server_address: tuple[str, int],
        registry: TrackerRegistry,
        cleanup_interval_seconds: int = 10,
    ):
        super().__init__(server_address, TrackerRequestHandler)
        self.registry = registry
        self.cleanup_interval_seconds = cleanup_interval_seconds
        self._cleanup_stop = threading.Event()
        self._cleanup_thread: threading.Thread | None = None

    def start_cleanup_thread(self) -> None:
        if self._cleanup_thread and self._cleanup_thread.is_alive():
            return
        self._cleanup_thread = threading.Thread(target=self._cleanup_loop, name="tracker-cleanup", daemon=True)
        self._cleanup_thread.start()

    def stop_cleanup_thread(self) -> None:
        self._cleanup_stop.set()
        if self._cleanup_thread:
            self._cleanup_thread.join(timeout=2)

    def _cleanup_loop(self) -> None:
        while not self._cleanup_stop.wait(self.cleanup_interval_seconds):
            self.registry.cleanup_expired()


class TrackerRequestHandler(BaseHTTPRequestHandler):
    server: TrackerHTTPServer

    def do_GET(self) -> None:
        parsed = urlparse(self.path)
        path = parsed.path.rstrip("/") or "/"
        query = parse_qs(parsed.query)

        if path in ("/", "/health", "/api/v1/health"):
            self._send_json(
                HTTPStatus.OK,
                {
                    "status": "ok",
                    "message": "HTTP Tracker is listening",
                    "node_ttl_seconds": self.server.registry.node_ttl_seconds,
                },
            )
            return

        if path == "/api/v1/nodes":
            self._send_json(HTTPStatus.OK, {"nodes": self.server.registry.list_nodes()})
            return

        if path == "/api/v1/files":
            self._send_json(HTTPStatus.OK, {"files": self.server.registry.list_files()})
            return

        if path == "/api/v1/query":
            file_hash = _first(query.get("file_hash"))
            file_name = _first(query.get("file_name"))
            try:
                locations = self.server.registry.find_file_locations(file_hash=file_hash, file_name=file_name)
            except TrackerValidationError as exc:
                self._send_error(HTTPStatus.BAD_REQUEST, str(exc))
                return
            self._send_json(HTTPStatus.OK, {"locations": locations})
            return

        if path == "/api/v1/snapshot":
            self._send_json(HTTPStatus.OK, self.server.registry.snapshot())
            return
        
        # These are API v2 queries
        if path == "/api/v2/file_list":
            
            return

        self._send_error(HTTPStatus.NOT_FOUND, "endpoint not found")

    def do_POST(self) -> None:
        parsed = urlparse(self.path)
        path = parsed.path.rstrip("/") or "/"

        if path == "/api/v1/update":
            payload = self._read_json_body()
            if payload is None:
                return
            try:
                result = self.server.registry.update_node(payload, remote_host=self.client_address[0])
            except TrackerValidationError as exc:
                self._send_error(HTTPStatus.BAD_REQUEST, str(exc))
                return
            self._send_json(HTTPStatus.OK, result)
            return

        if path == "/api/v1/offline":
            payload = self._read_json_body()
            if payload is None:
                return
            try:
                node_id = payload.get("node_id") if isinstance(payload, dict) else None
                existed = self.server.registry.mark_node_offline(node_id)
            except TrackerValidationError as exc:
                self._send_error(HTTPStatus.BAD_REQUEST, str(exc))
                return
            self._send_json(HTTPStatus.OK, {"status": "ok", "removed": existed})
            return

        self._send_error(HTTPStatus.NOT_FOUND, "endpoint not found")

    def do_DELETE(self) -> None:
        parsed = urlparse(self.path)
        parts = [part for part in parsed.path.split("/") if part]
        if len(parts) == 4 and parts[:3] == ["api", "v1", "nodes"]:
            try:
                existed = self.server.registry.mark_node_offline(parts[3])
            except TrackerValidationError as exc:
                self._send_error(HTTPStatus.BAD_REQUEST, str(exc))
                return
            self._send_json(HTTPStatus.OK, {"status": "ok", "removed": existed})
            return
        self._send_error(HTTPStatus.NOT_FOUND, "endpoint not found")

    def log_message(self, fmt: str, *args: Any) -> None:
        print(f"[tracker] {self.address_string()} - {fmt % args}")

    def _read_json_body(self) -> dict[str, Any] | None:
        raw_length = self.headers.get("Content-Length")
        if raw_length is None:
            self._send_error(HTTPStatus.LENGTH_REQUIRED, "Content-Length is required")
            return None
        try:
            length = int(raw_length)
        except ValueError:
            self._send_error(HTTPStatus.BAD_REQUEST, "invalid Content-Length")
            return None
        if length > 1024 * 1024:
            self._send_error(HTTPStatus.REQUEST_ENTITY_TOO_LARGE, "request body is too large")
            return None
        body = self.rfile.read(length)
        try:
            payload = json.loads(body.decode("utf-8"))
        except (UnicodeDecodeError, json.JSONDecodeError) as exc:
            self._send_error(HTTPStatus.BAD_REQUEST, f"invalid JSON: {exc}")
            return None
        if not isinstance(payload, dict):
            self._send_error(HTTPStatus.BAD_REQUEST, "JSON body must be an object")
            return None
        return payload

    def _send_json(self, status: HTTPStatus, payload: dict[str, Any]) -> None:
        body = json.dumps(payload, ensure_ascii=False, indent=2).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(body)

    def _send_error(self, status: HTTPStatus, message: str) -> None:
        self._send_json(status, {"status": "error", "error": message})


def _first(values: list[str] | None) -> str | None:
    if not values:
        return None
    value = values[0].strip()
    return value or None


def create_tracker_server(
    host: str = "127.0.0.1",
    port: int = 8000,
    *,
    state_path: str | None = "tracker_state.json",
    node_ttl_seconds: int = 90,
    cleanup_interval_seconds: int = 10,
) -> TrackerHTTPServer:
    registry = TrackerRegistry(state_path=state_path, node_ttl_seconds=node_ttl_seconds)
    return TrackerHTTPServer((host, port), registry, cleanup_interval_seconds=cleanup_interval_seconds)


def run_tracker(
    host: str = "127.0.0.1",
    port: int = 8000,
    *,
    state_path: str | None = "tracker_state.json",
    node_ttl_seconds: int = 90,
    cleanup_interval_seconds: int = 10,
) -> None:
    server = create_tracker_server(
        host=host,
        port=port,
        state_path=state_path,
        node_ttl_seconds=node_ttl_seconds,
        cleanup_interval_seconds=cleanup_interval_seconds,
    )
    server.start_cleanup_thread()
    bound_host, bound_port = server.server_address
    print(f"HTTP Tracker listening on http://{bound_host}:{bound_port}")
    print(f"State file: {state_path or 'disabled'}")
    print("Press Ctrl+C to stop.")
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("\nStopping HTTP Tracker...")
    finally:
        server.stop_cleanup_thread()
        server.server_close()


def main() -> None:
    parser = argparse.ArgumentParser(description="Run the DCS HTTP Tracker update server.")
    parser.add_argument("--host", default="127.0.0.1", help="Host/IP to listen on. Use 0.0.0.0 for LAN demos.")
    parser.add_argument("--port", type=int, default=8000, help="TCP port to listen on.")
    parser.add_argument("--state", default="tracker_state.json", help="JSON file used to persist Tracker state.")
    parser.add_argument("--no-state", action="store_true", help="Disable JSON persistence.")
    parser.add_argument("--ttl", type=int, default=90, help="Seconds before a node expires without updates.")
    parser.add_argument("--cleanup-interval", type=int, default=10, help="Seconds between expired-node cleanup passes.")
    args = parser.parse_args()

    run_tracker(
        host=args.host,
        port=args.port,
        state_path=None if args.no_state else args.state,
        node_ttl_seconds=args.ttl,
        cleanup_interval_seconds=args.cleanup_interval,
    )


if __name__ == "__main__":
    main()
