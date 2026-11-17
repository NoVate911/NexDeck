export type RiskLevel = "Low" | "Medium" | "High" | "Critical";
export type NotificationPriority = "Low" | "Normal" | "High" | "Critical";

export interface ApprovalStatus {
  state: "Pending" | "Resolved" | "Expired" | "Revoked";
  selected_action?: string;
}

export interface ApprovalRequest {
  request_id: string;
  application: string;
  title: string;
  description: string;
  expires_at_epoch_ms: number;
  risk_level: RiskLevel;
  allowed_actions: string[];
  status: ApprovalStatus;
}

export interface NotificationEvent {
  event_id: string;
  source: {
    kind: "WindowsSystem" | "HardwareMonitor" | "ThirdPartyApp" | "Plugin";
    name?: string;
  };
  title: string;
  body: string;
  priority: NotificationPriority;
  created_at_epoch_ms: number;
}

export interface NexDeckClientOptions {
  timeoutMs?: number;
  rejectUnauthorized?: boolean;
  ca?: string | Buffer;
}

export declare class NexDeckApiError extends Error {
  readonly statusCode: number;
  readonly errorCode: string;
  readonly message: string;
  constructor(statusCode: number, errorCode: string, message: string);
}

export declare class NexDeckClient {
  readonly baseUrl: string;
  readonly timeoutMs: number;
  readonly rejectUnauthorized: boolean;
  constructor(
    baseUrl: string,
    clientSecret: string,
    options?: NexDeckClientOptions
  );

  status(): Promise<{
    status: string;
    protocol_version: number;
    client_id: string;
    application_name: string;
    permissions: string[];
  }>;

  getMetrics(): Promise<Record<string, unknown>>;

  listNotifications(): Promise<NotificationEvent[]>;

  sendNotification(params: {
    title: string;
    body?: string;
    priority?: NotificationPriority;
  }): Promise<{
    event: NotificationEvent;
    queued_count: number;
  }>;

  listApprovals(): Promise<ApprovalRequest[]>;

  requestApproval(params: {
    requestId: string;
    title: string;
    description?: string;
    actions: string[];
    timeoutSecs?: number;
    riskLevel?: RiskLevel;
    application?: string;
  }): Promise<{ request: ApprovalRequest }>;

  getApproval(requestId: string): Promise<{ request: ApprovalRequest }>;

  waitForApproval(
    requestId: string,
    options?: { pollIntervalMs?: number; timeoutMs?: number }
  ): Promise<{ request: ApprovalRequest }>;

  resolveApproval(
    requestId: string,
    params: { action: string }
  ): Promise<{ request: ApprovalRequest }>;

  revokeApproval(requestId: string): Promise<{ request: ApprovalRequest }>;

  sendControl(params: {
    commandId: string;
    action: Record<string, unknown>;
  }): Promise<{
    command_id: string;
    status: "accepted" | "rejected";
    message?: string | null;
  }>;
}
