"""NexDeck Universal API client SDK for Python 3 (zero external dependencies).

Provides a lightweight, synchronous HTTP/HTTPS client for local third-party
applications integrating with the NexDeck Core Agent (`/api/v1/*`).
"""

from __future__ import annotations

import json
import ssl
import time
import urllib.error
import urllib.request
from typing import Any, Optional


class NexDeckApiError(Exception):
    """Raised when the NexDeck Universal API returns an HTTP or protocol error."""

    def __init__(self, status_code: int, error_code: str, message: str) -> None:
        super().__init__(f"HTTP {status_code} ({error_code}): {message}")
        self.status_code = status_code
        self.error_code = error_code
        self.message = message


class NexDeckClient:
    """Client for the NexDeck Universal API (`/api/v1/*`).

    All requests authenticate via `Authorization: Bearer <client_token>`.
    """

    def __init__(
        self,
        base_url: str,
        client_secret: str,
        *,
        timeout_sec: float = 10.0,
        verify_tls: bool = True,
        ca_cert_path: Optional[str] = None,
    ) -> None:
        cleaned_url = base_url.strip().rstrip("/")
        if not cleaned_url.startswith(("http://", "https://")):
            raise ValueError("base_url must start with http:// or https://")
        cleaned_secret = client_secret.strip()
        if not cleaned_secret:
            raise ValueError("client_secret must not be empty")

        self.base_url = cleaned_url
        self._auth_header = f"Bearer {cleaned_secret}"
        self.timeout_sec = float(timeout_sec)
        self._ssl_context: Optional[ssl.SSLContext] = None

        if self.base_url.startswith("https://"):
            if verify_tls:
                ctx = ssl.create_default_context(cafile=ca_cert_path)
                self._ssl_context = ctx
            else:
                ctx = ssl.create_default_context()
                ctx.check_hostname = False
                ctx.verify_mode = ssl.CERT_NONE
                self._ssl_context = ctx

    def _request_json(
        self,
        method: str,
        path: str,
        payload: Optional[dict[str, Any]] = None,
    ) -> dict[str, Any]:
        url = f"{self.base_url}{path}"
        headers = {
            "Accept": "application/json",
            "Authorization": self._auth_header,
        }
        body_bytes: Optional[bytes] = None
        if payload is not None:
            body_bytes = json.dumps(payload).encode("utf-8")
            headers["Content-Type"] = "application/json"

        request = urllib.request.Request(
            url=url,
            data=body_bytes,
            headers=headers,
            method=method.upper(),
        )
        try:
            with urllib.request.urlopen(
                request,
                timeout=self.timeout_sec,
                context=self._ssl_context,
            ) as response:
                raw = response.read().decode("utf-8")
                return json.loads(raw) if raw else {}
        except urllib.error.HTTPError as exc:
            raw_err = exc.read().decode("utf-8", errors="replace")
            error_code = "http_error"
            message = raw_err or str(exc.reason)
            try:
                parsed = json.loads(raw_err)
                if isinstance(parsed, dict):
                    error_code = str(parsed.get("error", error_code))
                    message = str(parsed.get("message", message))
            except ValueError:
                pass
            raise NexDeckApiError(exc.code, error_code, message) from exc

    def status(self) -> dict[str, Any]:
        """Return authenticated client metadata and agent runtime status (`GET /api/v1/status`)."""
        return self._request_json("GET", "/api/v1/status")

    def get_metrics(self) -> dict[str, Any]:
        """Return the latest system telemetry snapshot (`GET /api/v1/metrics`)."""
        return self._request_json("GET", "/api/v1/metrics")

    def list_notifications(self) -> list[dict[str, Any]]:
        """Return queued notifications (`GET /api/v1/notifications`)."""
        resp = self._request_json("GET", "/api/v1/notifications")
        items = resp.get("notifications", [])
        return list(items) if isinstance(items, list) else []

    def send_notification(
        self,
        *,
        title: str,
        body: str = "",
        priority: str = "Normal",
    ) -> dict[str, Any]:
        """Publish a notification event (`POST /api/v1/notifications`)."""
        payload: dict[str, Any] = {
            "title": title,
            "body": body,
            "priority": priority,
        }
        return self._request_json("POST", "/api/v1/notifications", payload)

    def list_approvals(self) -> list[dict[str, Any]]:
        """Return registered approval requests (`GET /api/v1/approvals`)."""
        resp = self._request_json("GET", "/api/v1/approvals")
        items = resp.get("approvals", [])
        return list(items) if isinstance(items, list) else []

    def request_approval(
        self,
        *,
        request_id: str,
        title: str,
        description: str = "",
        actions: list[str],
        timeout_secs: int = 60,
        risk_level: str = "Medium",
        application: Optional[str] = None,
    ) -> dict[str, Any]:
        """Register a single-use approval request (`POST /api/v1/approvals`).

        Note: if `application` is provided, it must match the registered
        `application_name` of this Universal API client. Remote approvals never
        bypass or replace Windows UAC.
        """
        payload: dict[str, Any] = {
            "request_id": request_id,
            "title": title,
            "description": description,
            "timeout_secs": max(1, int(timeout_secs)),
            "actions": list(actions),
            "risk_level": risk_level,
        }
        if application is not None:
            payload["application"] = application
        return self._request_json("POST", "/api/v1/approvals", payload)

    def get_approval(self, request_id: str) -> dict[str, Any]:
        """Fetch current status of an approval request (`GET /api/v1/approvals/{request_id}`)."""
        return self._request_json("GET", f"/api/v1/approvals/{request_id}")

    def wait_for_approval(
        self,
        request_id: str,
        *,
        poll_interval_sec: float = 0.5,
        timeout_sec: float = 60.0,
    ) -> dict[str, Any]:
        """Poll an approval request until its state is no longer `Pending` or timeout expires."""
        deadline = time.monotonic() + max(0.1, float(timeout_sec))
        interval = max(0.05, float(poll_interval_sec))
        while True:
            resp = self.get_approval(request_id)
            req_obj = resp.get("request", {})
            status_obj = req_obj.get("status", {}) if isinstance(req_obj, dict) else {}
            state = status_obj.get("state") if isinstance(status_obj, dict) else None
            if state and state.lower() != "pending":
                return resp
            if time.monotonic() >= deadline:
                raise TimeoutError(
                    f"Approval request '{request_id}' remained pending after {timeout_sec}s"
                )
            time.sleep(interval)

    def resolve_approval(
        self,
        request_id: str,
        *,
        action: str,
    ) -> dict[str, Any]:
        """Resolve a pending approval request (`POST /api/v1/approvals/{request_id}/resolve`)."""
        return self._request_json(
            "POST",
            f"/api/v1/approvals/{request_id}/resolve",
            {"action": action},
        )

    def revoke_approval(self, request_id: str) -> dict[str, Any]:
        """Revoke a pending approval request (`POST /api/v1/approvals/{request_id}/revoke`)."""
        return self._request_json("POST", f"/api/v1/approvals/{request_id}/revoke", {})

    def send_control(
        self,
        *,
        command_id: str,
        action: dict[str, Any],
    ) -> dict[str, Any]:
        """Dispatch a system/media control command (`POST /api/v1/control`)."""
        payload: dict[str, Any] = {
            "command_id": command_id,
            "action": action,
        }
        return self._request_json("POST", "/api/v1/control", payload)
