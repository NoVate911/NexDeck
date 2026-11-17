/**
 * NexDeck Universal API client SDK for Node.js (ESM, zero external dependencies).
 *
 * Provides a lightweight client for local third-party applications integrating
 * with the NexDeck Core Agent (`/api/v1/*`).
 */

import http from "node:http";
import https from "node:https";
import { URL } from "node:url";

export class NexDeckApiError extends Error {
  /**
   * @param {number} statusCode
   * @param {string} errorCode
   * @param {string} message
   */
  constructor(statusCode, errorCode, message) {
    super(`HTTP ${statusCode} (${errorCode}): ${message}`);
    this.name = "NexDeckApiError";
    this.statusCode = statusCode;
    this.errorCode = errorCode;
    this.message = message;
  }
}

export class NexDeckClient {
  /**
   * @param {string} baseUrl
   * @param {string} clientSecret
   * @param {{ timeoutMs?: number, rejectUnauthorized?: boolean, ca?: string | Buffer }} [options]
   */
  constructor(baseUrl, clientSecret, options = {}) {
    const cleanedUrl = String(baseUrl ?? "").trim().replace(/\/+$/, "");
    if (!cleanedUrl.startsWith("http://") && !cleanedUrl.startsWith("https://")) {
      throw new Error("baseUrl must start with http:// or https://");
    }
    const cleanedSecret = String(clientSecret ?? "").trim();
    if (!cleanedSecret) {
      throw new Error("clientSecret must not be empty");
    }

    this.baseUrl = cleanedUrl;
    this._authHeader = `Bearer ${cleanedSecret}`;
    this.timeoutMs = Number(options.timeoutMs ?? 10_000);
    this.rejectUnauthorized = options.rejectUnauthorized ?? true;
    this.ca = options.ca;
  }

  /**
   * @param {string} method
   * @param {string} path
   * @param {Record<string, unknown> | undefined} [payload]
   * @returns {Promise<Record<string, any>>}
   */
  _requestJson(method, path, payload) {
    const target = new URL(`${this.baseUrl}${path}`);
    const isHttps = target.protocol === "https:";
    const bodyString = payload !== undefined ? JSON.stringify(payload) : undefined;
    const headers = {
      Accept: "application/json",
      Authorization: this._authHeader,
    };
    if (bodyString !== undefined) {
      headers["Content-Type"] = "application/json";
      headers["Content-Length"] = String(Buffer.byteLength(bodyString, "utf8"));
    }

    const reqOptions = {
      method: method.toUpperCase(),
      hostname: target.hostname,
      port: target.port || (isHttps ? 443 : 80),
      path: `${target.pathname}${target.search}`,
      headers,
      timeout: this.timeoutMs,
      ...(isHttps
        ? {
            rejectUnauthorized: this.rejectUnauthorized,
            ...(this.ca !== undefined ? { ca: this.ca } : {}),
          }
        : {}),
    };

    const transport = isHttps ? https : http;
    return new Promise((resolve, reject) => {
      const req = transport.request(reqOptions, (res) => {
        const chunks = [];
        res.on("data", (chunk) => chunks.push(chunk));
        res.on("end", () => {
          const raw = Buffer.concat(chunks).toString("utf8");
          const statusCode = res.statusCode ?? 500;
          if (statusCode >= 200 && statusCode < 300) {
            try {
              resolve(raw ? JSON.parse(raw) : {});
            } catch (err) {
              reject(err);
            }
            return;
          }

          let errorCode = "http_error";
          let message = raw || res.statusMessage || "Request failed";
          try {
            const parsed = JSON.parse(raw);
            if (parsed && typeof parsed === "object") {
              errorCode = String(parsed.error ?? errorCode);
              message = String(parsed.message ?? message);
            }
          } catch {
            // Preserve raw message when response is not JSON.
          }
          reject(new NexDeckApiError(statusCode, errorCode, message));
        });
      });

      req.on("timeout", () => {
        req.destroy(new Error(`Request timed out after ${this.timeoutMs}ms`));
      });
      req.on("error", reject);
      if (bodyString !== undefined) {
        req.write(bodyString);
      }
      req.end();
    });
  }

  /** @returns {Promise<Record<string, any>>} */
  status() {
    return this._requestJson("GET", "/api/v1/status");
  }

  /** @returns {Promise<Record<string, any>>} */
  getMetrics() {
    return this._requestJson("GET", "/api/v1/metrics");
  }

  /** @returns {Promise<Array<Record<string, any>>>} */
  async listNotifications() {
    const resp = await this._requestJson("GET", "/api/v1/notifications");
    return Array.isArray(resp.notifications) ? resp.notifications : [];
  }

  /**
   * @param {{ title: string, body?: string, priority?: string }} params
   */
  sendNotification({ title, body = "", priority = "Normal" }) {
    return this._requestJson("POST", "/api/v1/notifications", {
      title,
      body,
      priority,
    });
  }

  /** @returns {Promise<Array<Record<string, any>>>} */
  async listApprovals() {
    const resp = await this._requestJson("GET", "/api/v1/approvals");
    return Array.isArray(resp.approvals) ? resp.approvals : [];
  }

  /**
   * @param {{
   *   requestId: string,
   *   title: string,
   *   description?: string,
   *   actions: string[],
   *   timeoutSecs?: number,
   *   riskLevel?: string,
   *   application?: string
   * }} params
   */
  requestApproval({
    requestId,
    title,
    description = "",
    actions,
    timeoutSecs = 60,
    riskLevel = "Medium",
    application,
  }) {
    const payload = {
      request_id: requestId,
      title,
      description,
      timeout_secs: Math.max(1, Math.trunc(timeoutSecs)),
      actions,
      risk_level: riskLevel,
      ...(application !== undefined ? { application } : {}),
    };
    return this._requestJson("POST", "/api/v1/approvals", payload);
  }

  /** @param {string} requestId */
  getApproval(requestId) {
    return this._requestJson("GET", `/api/v1/approvals/${encodeURIComponent(requestId)}`);
  }

  /**
   * @param {string} requestId
   * @param {{ pollIntervalMs?: number, timeoutMs?: number }} [options]
   */
  async waitForApproval(requestId, options = {}) {
    const pollIntervalMs = Math.max(50, Number(options.pollIntervalMs ?? 500));
    const timeoutMs = Math.max(100, Number(options.timeoutMs ?? 60_000));
    const deadline = Date.now() + timeoutMs;

    while (true) {
      const resp = await this.getApproval(requestId);
      const state = resp?.request?.status?.state;
      if (state && String(state).toLowerCase() !== "pending") {
        return resp;
      }
      if (Date.now() >= deadline) {
        throw new Error(
          `Approval request '${requestId}' remained pending after ${timeoutMs}ms`
        );
      }
      await new Promise((r) => setTimeout(r, pollIntervalMs));
    }
  }

  /**
   * @param {string} requestId
   * @param {{ action: string }} params
   */
  resolveApproval(requestId, { action }) {
    return this._requestJson(
      "POST",
      `/api/v1/approvals/${encodeURIComponent(requestId)}/resolve`,
      { action }
    );
  }

  /** @param {string} requestId */
  revokeApproval(requestId) {
    return this._requestJson(
      "POST",
      `/api/v1/approvals/${encodeURIComponent(requestId)}/revoke`,
      {}
    );
  }

  /**
   * @param {{ commandId: string, action: Record<string, unknown> }} params
   */
  sendControl({ commandId, action }) {
    return this._requestJson("POST", "/api/v1/control", {
      command_id: commandId,
      action,
    });
  }
}
