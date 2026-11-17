import assert from "node:assert/strict";
import http from "node:http";
import test from "node:test";
import { NexDeckApiError, NexDeckClient } from "./nexdeck_sdk.mjs";

test("NexDeckClient authenticates and exercises status, metrics, notifications, approvals, and control", async () => {
  const secretVal = `synthetic-js-secret-${process.pid}`;
  const expectedAuth = `Bearer ${secretVal}`;
  const approvals = new Map();
  const pollCounts = new Map();

  const server = http.createServer((req, res) => {
    const sendJson = (code, body) => {
      const encoded = Buffer.from(JSON.stringify(body), "utf8");
      res.writeHead(code, {
        "Content-Type": "application/json",
        "Content-Length": String(encoded.length),
      });
      res.end(encoded);
    };

    if (req.headers.authorization !== expectedAuth) {
      sendJson(401, {
        error: "unauthorized",
        message: "Invalid client credentials",
      });
      return;
    }

    const chunks = [];
    req.on("data", (c) => chunks.push(c));
    req.on("end", () => {
      const raw = Buffer.concat(chunks).toString("utf8");
      const body = raw ? JSON.parse(raw) : {};

      if (req.method === "GET" && req.url === "/api/v1/status") {
        sendJson(200, {
          status: "ok",
          protocol_version: 1,
          client_id: "org.example.app",
          application_name: "ExampleApp",
          permissions: [
            "read_metrics",
            "send_notification",
            "request_approval",
            "execute_control",
          ],
        });
        return;
      }

      if (req.method === "GET" && req.url === "/api/v1/metrics") {
        sendJson(200, {
          captured_at_epoch_ms: 1700000000000,
          cpu: { total_usage_pct: 22.0, frequency_mhz: 3500 },
        });
        return;
      }

      if (req.method === "POST" && req.url === "/api/v1/notifications") {
        sendJson(200, {
          event: {
            event_id: "notif-example-1",
            source: { kind: "ThirdPartyApp", name: "ExampleApp" },
            title: body.title,
            body: body.body ?? "",
            priority: body.priority ?? "Normal",
            created_at_epoch_ms: 1700000000000,
          },
          queued_count: 1,
        });
        return;
      }

      if (req.method === "POST" && req.url === "/api/v1/approvals") {
        const record = {
          request_id: body.request_id,
          application: body.application ?? "ExampleApp",
          title: body.title,
          description: body.description ?? "",
          expires_at_epoch_ms: 1700000060000,
          risk_level: body.risk_level ?? "Medium",
          allowed_actions: body.actions,
          status: { state: "Pending" },
        };
        approvals.set(body.request_id, record);
        pollCounts.set(body.request_id, 0);
        sendJson(200, { request: record });
        return;
      }

      if (req.method === "GET" && req.url?.startsWith("/api/v1/approvals/")) {
        const reqId = req.url.slice("/api/v1/approvals/".length);
        const record = approvals.get(reqId);
        if (!record) {
          sendJson(404, { error: "not_found", message: "Unknown request" });
          return;
        }
        const count = (pollCounts.get(reqId) ?? 0) + 1;
        pollCounts.set(reqId, count);
        if (count >= 2 && record.status.state === "Pending") {
          record.status = { state: "Resolved", selected_action: "approve" };
        }
        sendJson(200, { request: record });
        return;
      }

      if (req.method === "POST" && req.url === "/api/v1/control") {
        sendJson(200, {
          command_id: body.command_id,
          status: "accepted",
          message: null,
        });
        return;
      }

      sendJson(404, { error: "not_found", message: "Not found" });
    });
  });

  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  const addr = server.address();
  const baseUrl = `http://127.0.0.1:${addr.port}`;

  try {
    const client = new NexDeckClient(baseUrl, secretVal);

    const status = await client.status();
    assert.equal(status.client_id, "org.example.app");
    assert.equal(status.application_name, "ExampleApp");

    const metrics = await client.getMetrics();
    assert.equal(metrics.cpu.total_usage_pct, 22.0);

    const notif = await client.sendNotification({
      title: "Example notification",
      body: "Synthetic workflow finished",
    });
    assert.equal(notif.queued_count, 1);

    const created = await client.requestApproval({
      requestId: "req-example-1",
      application: "ExampleApp",
      title: "Confirm synthetic action",
      description: "Apply synthetic preset",
      actions: ["approve", "deny"],
    });
    assert.equal(created.request.status.state, "Pending");

    const resolved = await client.waitForApproval("req-example-1", {
      pollIntervalMs: 50,
      timeoutMs: 2000,
    });
    assert.equal(resolved.request.status.state, "Resolved");
    assert.equal(resolved.request.status.selected_action, "approve");

    const ctrl = await client.sendControl({
      commandId: "cmd-example-1",
      action: { kind: "media_play_pause" },
    });
    assert.equal(ctrl.status, "accepted");

    const badClient = new NexDeckClient(baseUrl, "wrong-secret");
    await assert.rejects(
      () => badClient.status(),
      (err) =>
        err instanceof NexDeckApiError &&
        err.statusCode === 401 &&
        err.errorCode === "unauthorized"
    );
  } finally {
    await new Promise((resolve) => server.close(resolve));
  }
});
