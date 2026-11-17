/**
 * Synthetic example: sending a notification and reading metrics via the JS SDK.
 *
 * Usage:
 *   node examples/api_notification_example.mjs --dry-run
 *   node examples/api_notification_example.mjs --url https://127.0.0.1:8765 --secret <client_token>
 */

import { NexDeckClient } from "../sdk/js/nexdeck_sdk.mjs";

const args = process.argv.slice(2);
const isDryRun = args.includes("--dry-run");
const urlIdx = args.indexOf("--url");
const secretIdx = args.indexOf("--secret");

const baseUrl = urlIdx >= 0 ? args[urlIdx + 1] : "https://127.0.0.1:8765";
const clientSecret = secretIdx >= 0 ? args[secretIdx + 1] : "";

const syntheticNotification = {
  title: "ExampleApp Task Completed",
  body: "Synthetic background job finished with status OK.",
  priority: "Normal",
};

if (isDryRun || !clientSecret) {
  console.log(JSON.stringify(syntheticNotification, null, 2));
  process.exit(0);
}

const client = new NexDeckClient(baseUrl, clientSecret, {
  rejectUnauthorized: false,
});

const status = await client.status();
console.log(`Authenticated as ${status.client_id} (${status.application_name})`);

const notifResult = await client.sendNotification(syntheticNotification);
console.log("Notification queued:", JSON.stringify(notifResult, null, 2));

const metrics = await client.getMetrics();
console.log("Latest CPU usage (%):", metrics.cpu?.total_usage_pct);
