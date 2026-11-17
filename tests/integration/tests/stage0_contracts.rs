//! Stage 0 end-to-end contract integration tests across workspace crates.

use std::collections::HashSet;

use nexdeck_api::{
    ApiPermission, ApprovalRequestPayload, ClientPolicy, build_notification_event,
    submit_approval_request,
};
use nexdeck_approval::{ApprovalError, ApprovalStatus, RiskLevel};
use nexdeck_core::{AgentConfig, CoreAgent};
use nexdeck_monitoring::{
    CpuMetrics, MemoryMetrics, MetricCollector, StaticCollector, SystemSnapshot, default_collector,
};
use nexdeck_notifications::{NotificationFilter, NotificationPriority};
use nexdeck_plugin_engine::{PluginManifest, PluginPermission, PluginRuntimeKind, ResourceLimits};
use nexdeck_transport::{Envelope, MessageKind};

#[test]
fn universal_api_approval_and_notification_flow_through_core() {
    let mut agent = CoreAgent::new(AgentConfig::default()).expect("default config should be valid");
    let client = ClientPolicy::new(
        "example-client-1",
        "ExampleApp",
        [
            ApiPermission::RequestApproval,
            ApiPermission::SendNotification,
        ],
    );
    let req_id = format!("req-example-{}", 1);

    let payload = ApprovalRequestPayload {
        request_id: req_id.clone(),
        application: "ExampleApp".to_owned(),
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
        .resolve(&req_id, "approve", 1_010_000)
        .expect("timely allowed decision should resolve");
    assert_eq!(resolved.status, ApprovalStatus::Resolved("approve".into()));

    assert_eq!(
        agent.approvals_mut().resolve(&req_id, "approve", 1_015_000),
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
        id: "org.example.metrics".to_owned(),
        name: "System Metrics".to_owned(),
        version: "0.1.0".to_owned(),
        runtime: PluginRuntimeKind::Wasm,
        permissions: HashSet::from([PluginPermission::MetricsRead]),
        limits: ResourceLimits::default(),
    };
    assert_eq!(plugin.validate(), Ok(()));
}

#[test]
fn monitoring_static_and_platform_collectors_integrate_with_core() {
    let mut agent = CoreAgent::new(AgentConfig::default()).expect("default config should be valid");
    let sample = SystemSnapshot {
        captured_at_epoch_ms: 1_000,
        uptime_secs: 120,
        cpu: CpuMetrics {
            total_usage_pct: 18.5,
            per_core_usage_pct: vec![15.0, 22.0],
            frequency_mhz: Some(3_800),
            temperature_celsius: None,
        },
        memory: MemoryMetrics {
            total_bytes: 16 * 1024 * 1024 * 1024,
            used_bytes: 4 * 1024 * 1024 * 1024,
        },
        gpus: Vec::new(),
        disks: Vec::new(),
        networks: Vec::new(),
        battery: None,
    };

    let mut static_collector = StaticCollector::new(sample.clone());
    let collected = static_collector
        .collect()
        .expect("static collector should succeed");
    agent.record_snapshot(collected);
    assert_eq!(agent.latest_snapshot(), Some(&sample));

    let mut platform = default_collector().expect("default_collector should initialize");
    #[cfg(windows)]
    {
        let live = platform
            .collect()
            .expect("Windows collector should sample metrics");
        assert!(live.memory.total_bytes > 0);
        assert!(live.uptime_secs > 0);
    }
    #[cfg(not(windows))]
    {
        assert!(platform.collect().is_err());
    }
}
