from __future__ import annotations

import json
import threading
import time
import unittest
from urllib.request import urlopen

from tracker.client import send_update
from tracker.registry import TrackerRegistry, TrackerValidationError
from tracker.server import create_tracker_server


class TrackerRegistryTest(unittest.TestCase):
    def test_update_node_indexes_resources(self) -> None:
        registry = TrackerRegistry(state_path=None, node_ttl_seconds=30)
        result = registry.update_node(
            {
                "node_id": "node-a",
                "host": "127.0.0.1",
                "port": 9001,
                "resources": [
                    {
                        "file_hash": "hash-x",
                        "file_name": "x.txt",
                        "file_size": 20,
                        "block_size": 10,
                        "blocks": [0, 1],
                    }
                ],
            }
        )

        self.assertEqual(result["status"], "ok")
        self.assertEqual(result["registered_resources"], 1)
        locations = registry.find_file_locations(file_hash="hash-x")
        self.assertEqual(locations[0]["file_name"], "x.txt")
        self.assertEqual(locations[0]["available_blocks"], [0, 1])
        self.assertEqual(locations[0]["peers"][0]["node_id"], "node-a")

    def test_update_rejects_missing_block_index(self) -> None:
        registry = TrackerRegistry(state_path=None)
        with self.assertRaises(TrackerValidationError):
            registry.update_node(
                {
                    "node_id": "node-a",
                    "host": "127.0.0.1",
                    "port": 9001,
                    "resources": [
                        {
                            "file_hash": "hash-x",
                            "file_name": "x.txt",
                            "blocks": [{"hash": "missing-index"}],
                        }
                    ],
                }
            )

    def test_expired_node_is_removed(self) -> None:
        registry = TrackerRegistry(state_path=None, node_ttl_seconds=1)
        registry.update_node(
            {
                "node_id": "node-a",
                "host": "127.0.0.1",
                "port": 9001,
                "resources": [],
            }
        )
        time.sleep(1.1)
        self.assertEqual(registry.cleanup_expired(), ["node-a"])
        self.assertEqual(registry.list_nodes(), [])


class TrackerHTTPTest(unittest.TestCase):
    def test_http_update_and_query(self) -> None:
        server = create_tracker_server(host="127.0.0.1", port=0, state_path=None, node_ttl_seconds=30)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            base_url = f"http://127.0.0.1:{server.server_address[1]}"
            update = send_update(
                base_url,
                node_id="node-a",
                host="127.0.0.1",
                port=9001,
                resources=[
                    {
                        "file_hash": "hash-x",
                        "file_name": "x.txt",
                        "blocks": [0, 1],
                    }
                ],
            )
            self.assertEqual(update["status"], "ok")

            with urlopen(base_url + "/api/v1/query?file_hash=hash-x", timeout=5) as response:
                body = json.loads(response.read().decode("utf-8"))
            self.assertEqual(body["locations"][0]["file_hash"], "hash-x")
            self.assertEqual(body["locations"][0]["peers"][0]["node_id"], "node-a")
        finally:
            server.shutdown()
            server.server_close()


if __name__ == "__main__":
    unittest.main()
