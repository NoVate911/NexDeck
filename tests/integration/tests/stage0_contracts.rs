//! Stage 0 end-to-end contract integration tests across workspace crates.

use std::collections::HashSet;

use nexdeck_api::{
    ApiPermission, ApprovalRequestPayload, ClientPolicy, build_notification_event,
    submit_approval_request,
};
use nexdeck_approval::{ApprovalError, ApprovalStatus, RiskLevel};
use nexdeck_core::{AgentConfig, CoreAgent};
use nexdeck_monitoring::MemoryMetrics;
use nexdeck_notifications::{NotificationFilter, NotificationPriority};
use nexdeck_plugin_engine::{PluginManifest, PluginPermission, PluginRuntimeKind, ResourceLimits};
use nexdeck_transport::{Envelope, MessageKind};

#[test]
fn universal_api_approval_and_notification_flow_through_core() {
    let mut agent = CoreAgent::new(AgentConfig::default()).expect("default config should be valid");
    let client = ClientPolicy::new(
        "client-ide-1",
        "MyApplication",
        [
            ApiPermission::RequestApproval,
            ApiPermission::SendNotification,
        ],
    );

    let payload = ApprovalRequestPayload {
        nonce: "req-2026-0001".to_owned(),
        application: "MyApplication".to_owned(),
        title: "Confirm Action".to_owned(),
        description: "Allow execution of the requested operation?".to_owned(),
        timeout_secs: 60,
        actions: vec!["approve".to_owned(), "deny".to_owned()],
        risk_level: RiskLevel::High,
    };

    submit_approval_request(&client, agent.approvals_mut(), payload, 1_000_000)
        .expect("authorized approval request should register");

    let resolved = agent
        .approvals_mut()
        .resolve("req-2026-0001", "approve", 1_010_000)
        .expect("timely allowed decision should resolve");
    assert_eq!(resolved.status, ApprovalStatus::Resolved("approve".into()));

    assert_eq!(
        agent
            .approvals_mut()
            .resolve("req-2026-0001", "approve", 1_015_000),
        Err(ApprovalError::AlreadyFinalized)
    );

    let event = build_notification_event(
        &client,
        1,
        "Action approved".to_owned(),
        "Operation permitted from mobile device.".to_owned(),
        NotificationPriority::High,
        1_010_500,
    )
    .expect("authorized notification should build");

    let filter = NotificationFilter::default();
    assert!(filter.matches(&event));
    agent.notifications_mut().push(event);
    assert_eq!(agent.notifications_mut().drain_all().len(), 1);
}

#[test]
fn transport_monitoring_and_plugin_contracts_validate() {
    let env = Envelope::new(42, MessageKind::Telemetry, b"{\"cpu\":12.5}".to_vec());
    assert_eq!(env.validate(), Ok(()));

    let mem = MemoryMetrics {
        total_bytes: 32 * 1024 * 1024 * 1024,
        used_bytes: 8 * 1024 * 1024 * 1024,
    };
    assert!((mem.usage_pct() - 25.0).abs() < 0.01);

    let plugin = PluginManifest {
        id: "dev.nexdeck.metrics".to_owned(),
        name: "System Metrics".to_owned(),
        version: "0.1.0".to_owned(),
        runtime: PluginRuntimeKind::Wasm,
        permissions: HashSet::from([PluginPermission::MetricsRead]),
        limits: ResourceLimits::default(),
    };
    assert_eq!(plugin.validate(), Ok(()));
}
