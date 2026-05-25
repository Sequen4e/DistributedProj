"""Small HTTP client used by user nodes to update the Tracker."""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any
from urllib.error import HTTPError, URLError
from urllib.parse import urljoin
from urllib.request import Request, urlopen


def post_json(url: str, payload: dict[str, Any], timeout: float = 5.0) -> dict[str, Any]:
    body = json.dumps(payload, ensure_ascii=False).encode("utf-8")
    request = Request(
        url,
        data=body,
        headers={"Content-Type": "application/json; charset=utf-8"},
        method="POST",
    )
    try:
        with urlopen(request, timeout=timeout) as response:
            return json.loads(response.read().decode("utf-8"))
    except HTTPError as exc:
        detail = exc.read().decode("utf-8", errors="replace")
        raise RuntimeError(f"Tracker returned HTTP {exc.code}: {detail}") from exc
    except URLError as exc:
        raise RuntimeError(f"Could not connect to Tracker: {exc.reason}") from exc


def send_update(
    tracker_url: str,
    *,
    node_id: str,
    host: str,
    port: int,
    resources: list[dict[str, Any]],
    metadata: dict[str, Any] | None = None,
    timeout: float = 5.0,
) -> dict[str, Any]:
    payload = {
        "node_id": node_id,
        "host": host,
        "port": port,
        "resources": resources,
        "metadata": metadata or {},
    }
    return post_json(urljoin(_with_slash(tracker_url), "api/v1/update"), payload, timeout=timeout)


def send_offline(tracker_url: str, *, node_id: str, timeout: float = 5.0) -> dict[str, Any]:
    return post_json(urljoin(_with_slash(tracker_url), "api/v1/offline"), {"node_id": node_id}, timeout=timeout)


def load_resources(path: str | Path) -> list[dict[str, Any]]:
    with Path(path).open("r", encoding="utf-8") as handle:
        data = json.load(handle)
    if isinstance(data, dict) and isinstance(data.get("resources"), list):
        return data["resources"]
    if isinstance(data, list):
        return data
    raise ValueError("resource file must be a list or an object containing resources")


def _with_slash(url: str) -> str:
    return url if url.endswith("/") else url + "/"


def main() -> None:
    parser = argparse.ArgumentParser(description="Send one resource update from a user node to the Tracker.")
    parser.add_argument("--tracker", default="http://127.0.0.1:8000", help="Tracker base URL.")
    parser.add_argument("--node-id", required=True, help="Unique user node id.")
    parser.add_argument("--host", required=True, help="Host/IP where this user node accepts peer downloads.")
    parser.add_argument("--port", type=int, required=True, help="Port where this user node accepts peer downloads.")
    parser.add_argument("--resources", required=True, help="Path to JSON resource list.")
    parser.add_argument("--meta", default="{}", help="Optional JSON metadata object.")
    args = parser.parse_args()

    metadata = json.loads(args.meta)
    if not isinstance(metadata, dict):
        raise SystemExit("--meta must be a JSON object")

    result = send_update(
        args.tracker,
        node_id=args.node_id,
        host=args.host,
        port=args.port,
        resources=load_resources(args.resources),
        metadata=metadata,
    )
    print(json.dumps(result, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
