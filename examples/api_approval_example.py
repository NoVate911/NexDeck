"""Synthetic example: submitting a Remote Approval request via the Python SDK.

Usage:
    python examples/api_approval_example.py --dry-run
    python examples/api_approval_example.py --url https://127.0.0.1:8765 --secret <client_token>
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import time

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(REPO_ROOT, "sdk", "python"))

from nexdeck_sdk import NexDeckClient  # noqa: E402


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Synthetic ExampleApp integration with NexDeck Universal API"
    )
    parser.add_argument(
        "--url",
        default="https://127.0.0.1:8765",
        help="Base URL of the local NexDeck Core Agent",
    )
    parser.add_argument(
        "--secret",
        default="",
        help="Issued Universal API client_token for org.example.app",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Print the synthetic request payload without contacting a live server",
    )
    args = parser.parse_args()

    request_id = f"req-example-{int(time.time())}"
    synthetic_payload = {
        "request_id": request_id,
        "application": "ExampleApp",
        "title": "Apply synthetic workspace preset",
        "description": "Allow ExampleApp to apply synthetic workspace configuration preset",
        "timeout_secs": 60,
        "actions": ["approve", "deny"],
        "risk_level": "Medium",
    }

    if args.dry_run or not args.secret.strip():
        print(json.dumps(synthetic_payload, indent=2))
        return 0

    client = NexDeckClient(args.url, args.secret, verify_tls=False)
    status = client.status()
    print(f"Connected as {status['client_id']} ({status['application_name']})")

    created = client.request_approval(
        request_id=request_id,
        application="ExampleApp",
        title=synthetic_payload["title"],
        description=synthetic_payload["description"],
        actions=synthetic_payload["actions"],
        timeout_secs=synthetic_payload["timeout_secs"],
        risk_level=synthetic_payload["risk_level"],
    )
    print("Submitted approval request:", json.dumps(created, indent=2))

    resolved = client.wait_for_approval(request_id, poll_interval_sec=0.5, timeout_sec=60.0)
    print("Resolved approval decision:", json.dumps(resolved, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
