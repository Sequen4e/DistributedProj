"""Thread-safe resource index used by the HTTP Tracker.

The registry owns the data model. HTTP handlers and future query/download
modules should call this class instead of touching the JSON state directly.
"""

from __future__ import annotations

import json
import os
import threading
import time
from copy import deepcopy
from pathlib import Path
from typing import Any


class TrackerValidationError(ValueError):
    """Raised when a node update payload is invalid."""


def _now() -> float:
    return time.time()


def _iso(ts: float | None = None) -> str:
    return time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime(_now() if ts is None else ts))


def _as_non_empty_string(value: Any, field_name: str) -> str:
    if not isinstance(value, str) or not value.strip():
        raise TrackerValidationError(f"{field_name} must be a non-empty string")
    return value.strip()


def _as_optional_string(value: Any) -> str | None:
    if value is None:
        return None
    if not isinstance(value, str):
        return str(value)
    return value.strip() or None


def _as_int(value: Any, field_name: str, minimum: int | None = None) -> int:
    if isinstance(value, bool):
        raise TrackerValidationError(f"{field_name} must be an integer")
    try:
        number = int(value)
    except (TypeError, ValueError) as exc:
        raise TrackerValidationError(f"{field_name} must be an integer") from exc
    if minimum is not None and number < minimum:
        raise TrackerValidationError(f"{field_name} must be >= {minimum}")
    return number


def _normalize_blocks(raw_blocks: Any) -> list[dict[str, Any]]:
    if raw_blocks is None:
        raise TrackerValidationError("each resource must include blocks")
    if not isinstance(raw_blocks, list):
        raise TrackerValidationError("blocks must be a list")

    blocks: list[dict[str, Any]] = []
    seen_indexes: set[int] = set()
    for raw_block in raw_blocks:
        if isinstance(raw_block, dict):
            index = _as_int(raw_block.get("index"), "block.index", minimum=0)
            block: dict[str, Any] = {"index": index}
            block_hash = _as_optional_string(raw_block.get("hash"))
            if block_hash:
                block["hash"] = block_hash
            if raw_block.get("size") is not None:
                block["size"] = _as_int(raw_block.get("size"), "block.size", minimum=0)
        else:
            index = _as_int(raw_block, "block index", minimum=0)
            block = {"index": index}

        if index in seen_indexes:
            continue
        seen_indexes.add(index)
        blocks.append(block)

    blocks.sort(key=lambda item: item["index"])
    return blocks


def _normalize_resource(raw_resource: Any) -> dict[str, Any]:
    if not isinstance(raw_resource, dict):
        raise TrackerValidationError("each resource must be an object")

    file_hash = _as_non_empty_string(raw_resource.get("file_hash"), "file_hash")
    file_name = _as_non_empty_string(
        raw_resource.get("file_name") or raw_resource.get("filename"),
        "file_name",
    )
    blocks = _normalize_blocks(raw_resource.get("blocks") or raw_resource.get("block_indexes"))

    resource: dict[str, Any] = {
        "file_hash": file_hash,
        "file_name": file_name,
        "blocks": blocks,
    }
    for optional_int in ("file_size", "block_size"):
        if raw_resource.get(optional_int) is not None:
            resource[optional_int] = _as_int(raw_resource.get(optional_int), optional_int, minimum=0)
    content_type = _as_optional_string(raw_resource.get("content_type"))
    if content_type:
        resource["content_type"] = content_type
    return resource


class TrackerRegistry:
    """Stores node updates and builds file indexes for later query/download work."""

    def __init__(self, state_path: str | os.PathLike[str] | None = None, node_ttl_seconds: int = 90):
        self.state_path = Path(state_path) if state_path else None
        self.node_ttl_seconds = _as_int(node_ttl_seconds, "node_ttl_seconds", minimum=1)
        self._lock = threading.RLock()
        self._nodes: dict[str, dict[str, Any]] = {}
        if self.state_path:
            self._load_state()

    def update_node(self, payload: dict[str, Any], remote_host: str | None = None) -> dict[str, Any]:
        """Record the complete resource index for one node.

        The update is intentionally full-state: every update replaces the
        previous resource list for the same node. User nodes should send this
        when they come online and then again at a fixed interval.
        """

        if not isinstance(payload, dict):
            raise TrackerValidationError("update payload must be a JSON object")

        node_id = _as_non_empty_string(payload.get("node_id"), "node_id")
        host = _as_non_empty_string(
            payload.get("host") or payload.get("address") or payload.get("ip") or remote_host,
            "host",
        )
        port = _as_int(payload.get("port"), "port", minimum=1)
        if port > 65535:
            raise TrackerValidationError("port must be <= 65535")

        raw_resources = payload.get("resources", payload.get("files"))
        if raw_resources is None:
            raw_resources = []
        if not isinstance(raw_resources, list):
            raise TrackerValidationError("resources must be a list")
        resources = [_normalize_resource(item) for item in raw_resources]

        now_ts = _now()
        expires_ts = now_ts + self.node_ttl_seconds
        record = {
            "node_id": node_id,
            "host": host,
            "port": port,
            "last_seen": _iso(now_ts),
            "last_seen_epoch": now_ts,
            "expires_at": _iso(expires_ts),
            "expires_at_epoch": expires_ts,
            "resources": resources,
            "metadata": payload.get("metadata") if isinstance(payload.get("metadata"), dict) else {},
        }

        with self._lock:
            self._nodes[node_id] = record
            self._save_state()
            known_files = len(self.build_file_index(include_expired=True))

        return {
            "status": "ok",
            "node_id": node_id,
            "registered_resources": len(resources),
            "known_files": known_files,
            "expires_at": record["expires_at"],
            "server_time": _iso(now_ts),
        }

    def mark_node_offline(self, node_id: str) -> bool:
        node_id = _as_non_empty_string(node_id, "node_id")
        with self._lock:
            existed = node_id in self._nodes
            self._nodes.pop(node_id, None)
            if existed:
                self._save_state()
            return existed

    def cleanup_expired(self) -> list[str]:
        now_ts = _now()
        with self._lock:
            expired = [
                node_id
                for node_id, record in self._nodes.items()
                if float(record.get("expires_at_epoch", 0)) <= now_ts
            ]
            for node_id in expired:
                self._nodes.pop(node_id, None)
            if expired:
                self._save_state()
            return expired

    def list_nodes(self, include_expired: bool = False) -> list[dict[str, Any]]:
        with self._lock:
            if not include_expired:
                self.cleanup_expired()
            return [self._public_node(record) for record in self._nodes.values()]

    def get_node(self, node_id: str) -> dict[str, Any] | None:
        node_id = _as_non_empty_string(node_id, "node_id")
        with self._lock:
            self.cleanup_expired()
            record = self._nodes.get(node_id)
            return self._public_node(record) if record else None

    def build_file_index(self, include_expired: bool = False) -> dict[str, dict[str, Any]]:
        with self._lock:
            if not include_expired:
                self.cleanup_expired()
            index: dict[str, dict[str, Any]] = {}
            for node in self._nodes.values():
                peer = {
                    "node_id": node["node_id"],
                    "host": node["host"],
                    "port": node["port"],
                    "last_seen": node["last_seen"],
                    "expires_at": node["expires_at"],
                }
                for resource in node.get("resources", []):
                    file_hash = resource["file_hash"]
                    entry = index.setdefault(
                        file_hash,
                        {
                            "file_hash": file_hash,
                            "file_name": resource["file_name"],
                            "file_size": resource.get("file_size"),
                            "block_size": resource.get("block_size"),
                            "peers": [],
                            "available_blocks": [],
                        },
                    )
                    block_indexes = [block["index"] for block in resource.get("blocks", [])]
                    entry["peers"].append({**peer, "blocks": deepcopy(resource.get("blocks", []))})
                    entry["available_blocks"] = sorted(set(entry["available_blocks"]) | set(block_indexes))
                    if entry.get("file_size") is None and resource.get("file_size") is not None:
                        entry["file_size"] = resource["file_size"]
                    if entry.get("block_size") is None and resource.get("block_size") is not None:
                        entry["block_size"] = resource["block_size"]
            return index

    def list_files(self) -> list[dict[str, Any]]:
        file_index = self.build_file_index()
        files = []
        for item in file_index.values():
            files.append(
                {
                    "file_hash": item["file_hash"],
                    "file_name": item["file_name"],
                    "file_size": item.get("file_size"),
                    "block_size": item.get("block_size"),
                    "peer_count": len(item["peers"]),
                    "available_blocks": item["available_blocks"],
                }
            )
        return sorted(files, key=lambda item: (item["file_name"], item["file_hash"]))

    def find_file_locations(
        self,
        *,
        file_hash: str | None = None,
        file_name: str | None = None,
    ) -> list[dict[str, Any]]:
        """Read-only interface reserved for the query and download modules."""

        if not file_hash and not file_name:
            raise TrackerValidationError("file_hash or file_name is required")
        normalized_hash = file_hash.strip() if file_hash else None
        normalized_name = file_name.strip() if file_name else None

        results = []
        for entry in self.build_file_index().values():
            if normalized_hash and entry["file_hash"] != normalized_hash:
                continue
            if normalized_name and entry["file_name"] != normalized_name:
                continue
            results.append(deepcopy(entry))
        return sorted(results, key=lambda item: (item["file_name"], item["file_hash"]))

    def snapshot(self) -> dict[str, Any]:
        self.cleanup_expired()
        return {
            "server_time": _iso(),
            "node_ttl_seconds": self.node_ttl_seconds,
            "nodes": self.list_nodes(),
            "files": self.list_files(),
        }

    def _public_node(self, record: dict[str, Any]) -> dict[str, Any]:
        return {
            "node_id": record["node_id"],
            "host": record["host"],
            "port": record["port"],
            "last_seen": record["last_seen"],
            "expires_at": record["expires_at"],
            "resource_count": len(record.get("resources", [])),
            "resources": deepcopy(record.get("resources", [])),
            "metadata": deepcopy(record.get("metadata", {})),
        }

    def _load_state(self) -> None:
        if not self.state_path or not self.state_path.exists():
            return
        with self.state_path.open("r", encoding="utf-8") as handle:
            state = json.load(handle)
        nodes = state.get("nodes", {})
        if isinstance(nodes, dict):
            self._nodes = nodes
            self.cleanup_expired()

    def _save_state(self) -> None:
        if not self.state_path:
            return
        self.state_path.parent.mkdir(parents=True, exist_ok=True)
        temp_path = self.state_path.with_suffix(self.state_path.suffix + ".tmp")
        state = {
            "saved_at": _iso(),
            "node_ttl_seconds": self.node_ttl_seconds,
            "nodes": self._nodes,
        }
        with temp_path.open("w", encoding="utf-8") as handle:
            json.dump(state, handle, ensure_ascii=False, indent=2, sort_keys=True)
        os.replace(temp_path, self.state_path)
