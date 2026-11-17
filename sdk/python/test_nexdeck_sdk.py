"""Unit tests for the NexDeck Python Universal API SDK."""

from __future__ import annotations

import json
import os
import sys
import threading
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from typing import Any

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from nexdeck_sdk import NexDeckApiError, NexDeckClient  # noqa: E402


class _MockAgentHandler(BaseHTTPRequestHandler):
    expected_auth: str = ""
    approvals: dict[str, dict[str, Any]] = {}
    poll_counts: dict[str, int] = {}

    def log_message(self, format: str, *args: Any) -> None:  # noqa: A002
        return

    def _send_json(self, status: int, body: dict[str, Any]) -> None:
        encoded = json.dumps(body).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(encoded)))
        self.end_headers()
        self.wfile.write(encoded)

    def _check_auth(self) -> bool:
        header = self.headers.get("Authorization", "")
        if header != self.expected_auth:
            self._send_json(
                401,
                {"error": "unauthorized", "message": "Invalid client credentials"},
            )
            return False
        return True

    def do_GET(self) -> None:  # noqa: N802
        if not self._check_auth():
            return
        if self.path == "/api/v1/status":
            self._send_json(
                200,
                {
                    "status": "ok",
                    "protocol_version": 1,
                    "client_id": "org.example.app",
                    "application_name": "ExampleApp",
                    "permissions": [
                        "read_metrics",
                        "send_notification",
                        "request_approval",
                        "execute_control",
                    ],
                },
            )
            return
        if self.path == "/api/v1/metrics":
            self._send_json(
                200,
                {
                    "captured_at_epoch_ms": 1700000000000,
                    "cpu": {"total_usage_pct": 18.5, "frequency_mhz": 3600},
                },
            )
            return
        if self.path == "/api/v1/notifications":
            self._send_json(
                200,
                {
                    "notifications": [
                        {
                            "event_id": "notif-example-1",
                            "source": {"kind": "ThirdPartyApp", "name": "ExampleApp"},
                            "title": "Build finished",
                            "body": "Synthetic task completed",
                            "priority": "Normal",
                            "created_at_epoch_ms": 1700000000000,
                        }
                    ]
                },
            )
            return
        if self.path.startswith("/api/v1/approvals/"):
            req_id = self.path.removeprefix("/api/v1/approvals/")
            item = self.approvals.get(req_id)
            if item is None:
                self._send_json(404, {"error": "not_found", "message": "Unknown request"})
                return
            count = self.poll_counts.get(req_id, 0) + 1
            self.poll_counts[req_id] = count
            if count >= 2 and item["status"]["state"] == "Pending":
                item["status"] = {"state": "Resolved", "selected_action": "approve"}
            self._send_json(200, {"request": item})
            return
        self._send_json(404, {"error": "not_found", "message": "Route not found"})

    def do_POST(self) -> None:  # noqa: N802
        if not self._check_auth():
            return
        length = int(self.headers.get("Content-Length", "0"))
        raw = self.rfile.read(length).decode("utf-8") if length > 0 else "{}"
        body = json.loads(raw)

        if self.path == "/api/v1/notifications":
            self._send_json(
                200,
                {
                    "event": {
                        "event_id": "notif-example-1",
                        "source": {"kind": "ThirdPartyApp", "name": "ExampleApp"},
                        "title": body["title"],
                        "body": body.get("body", ""),
                        "priority": body.get("priority", "Normal"),
                        "created_at_epoch_ms": 1700000000000,
                    },
                    "queued_count": 1,
                },
            )
            return
        if self.path == "/api/v1/approvals":
            req_id = body["request_id"]
            record = {
                "request_id": req_id,
                "application": body.get("application", "ExampleApp"),
                "title": body["title"],
                "description": body.get("description", ""),
                "expires_at_epoch_ms": 1700000060000,
                "risk_level": body.get("risk_level", "Medium"),
                "allowed_actions": body["actions"],
                "status": {"state": "Pending"},
            }
            self.approvals[req_id] = record
            self.poll_counts[req_id] = 0
            self._send_json(200, {"request": record})
            return
        if self.path == "/api/v1/control":
            self._send_json(
                200,
                {
                    "command_id": body["command_id"],
                    "status": "accepted",
                    "message": None,
                },
            )
            return
        self._send_json(404, {"error": "not_found", "message": "Route not found"})


class NexDeckPythonSdkTest(unittest.TestCase):
    def setUp(self) -> None:
        self.secret_val = f"synthetic-client-secret-{os.getpid()}"
        _MockAgentHandler.expected_auth = f"Bearer {self.secret_val}"
        _MockAgentHandler.approvals = {}
        _MockAgentHandler.poll_counts = {}
        self.server = ThreadingHTTPServer(("127.0.0.1", 0), _MockAgentHandler)
        port = self.server.server_address[1]
        self.base_url = f"http://127.0.0.1:{port}"
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()

    def tearDown(self) -> None:
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(timeout=2.0)

    def test_status_metrics_notifications_approvals_and_controls(self) -> None:
        client = NexDeckClient(self.base_url, self.secret_val)

        status = client.status()
        self.assertEqual(status["client_id"], "org.example.app")
        self.assertEqual(status["application_name"], "ExampleApp")

        metrics = client.get_metrics()
        self.assertAlmostEqual(metrics["cpu"]["total_usage_pct"], 18.5)

        notif_resp = client.send_notification(
            title="Build finished",
            body="Synthetic task completed",
        )
        self.assertEqual(notif_resp["queued_count"], 1)

        items = client.list_notifications()
        self.assertEqual(len(items), 1)
        self.assertEqual(items[0]["event_id"], "notif-example-1")

        created = client.request_approval(
            request_id="req-example-1",
            application="ExampleApp",
            title="Confirm synthetic operation",
            description="Apply synthetic preset",
            actions=["approve", "deny"],
        )
        self.assertEqual(created["request"]["status"]["state"], "Pending")

        resolved = client.wait_for_approval(
            "req-example-1",
            poll_interval_sec=0.05,
            timeout_sec=2.0,
        )
        self.assertEqual(resolved["request"]["status"]["state"], "Resolved")
        self.assertEqual(
            resolved["request"]["status"]["selected_action"],
            "approve",
        )

        ctrl = client.send_control(
            command_id="cmd-example-1",
            action={"kind": "media_play_pause"},
        )
        self.assertEqual(ctrl["status"], "accepted")

    def test_invalid_credentials_raise_api_error(self) -> None:
        bad_client = NexDeckClient(self.base_url, "wrong-secret")
        with self.assertRaises(NexDeckApiError) as ctx:
            bad_client.status()
        self.assertEqual(ctx.exception.status_code, 401)
        self.assertEqual(ctx.exception.error_code, "unauthorized")


if __name__ == "__main__":
    unittest.main()
