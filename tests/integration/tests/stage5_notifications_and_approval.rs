//! Stage 5 integration tests for Notification Center sleep buffering, hardware threshold alerts,
//! and end-to-end Remote Approval expiration & single-use resolution guarantees.

use std::fs;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use nexdeck_api::clients::ClientRegistry;
use nexdeck_api::{
    ApiPermission, ApprovalRequestPayload, build_notification_event, submit_approval_request,
};
use nexdeck_approval::{ApprovalDecisionMessage, ApprovalRequest, ApprovalStatus, RiskLevel};
use nexdeck_core::AgentConfig;
use nexdeck_core::alerts::AlertThresholds;
use nexdeck_core::control::RecordingControlExecutor;
use nexdeck_core::runtime::{
    RuntimeOptions, process_approval_decision, process_notification_sync, spawn_agent_runtime_full,
};
use nexdeck_core::storage::StoragePaths;
use nexdeck_monitoring::{
    BatteryMetrics, CpuMetrics, MemoryMetrics, StaticCollector, SystemSnapshot,
};
use nexdeck_notifications::{
    NotificationFilter, NotificationPriority, NotificationSource, NotificationSyncCommand,
};
use nexdeck_transport::tls::HostTlsIdentity;
use nexdeck_transport::{Envelope, MessageKind};

fn now_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(1_000_000, |d| {
            u64::try_from(d.as_millis()).unwrap_or(u64::MAX)
        })
}

fn high_load_snapshot() -> SystemSnapshot {
    SystemSnapshot {
        captured_at_epoch_ms: now_epoch_ms(),
        uptime_secs: 1_200,
        cpu: CpuMetrics {
            total_usage_pct: 97.5,
            per_core_usage_pct: vec![97.0, 98.0],
            frequency_mhz: Some(4_200),
            temperature_celsius: Some(92.0),
        },
        memory: MemoryMetrics {
            total_bytes: 16 * 1024 * 1024 * 1024,
            used_bytes: 4 * 1024 * 1024 * 1024,
        },
        gpus: Vec::new(),
        disks: Vec::new(),
        networks: Vec::new(),
        battery: Some(BatteryMetrics {
            charge_pct: 10.0,
            ac_connected: false,
        }),
    }
}

#[test]
fn sleep_buffer_persists_across_restarts_and_approval_lifecycle_completes() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");

    rt.block_on(async {
        let temp_dir = std::env::temp_dir().join(format!(
            "nexdeck_stage5_test_{}_{}",
            std::process::id(),
            now_epoch_ms()
        ));
        let paths = StoragePaths::new(temp_dir.clone());
        let tls = HostTlsIdentity::generate_self_signed(&["localhost".to_owned()])
            .expect("generate self-signed TLS identity");

        let mut clients_reg = ClientRegistry::new();
        let issued = clients_reg
            .register_client(
                "org.example.stage5".to_owned(),
                "ExampleApp".to_owned(),
                [
                    ApiPermission::SendNotification,
                    ApiPermission::RequestApproval,
                    ApiPermission::ReadMetrics,
                ],
                1_700_000_000_000,
            )
            .expect("register client");
        let policy = clients_reg
            .authenticate(&issued.access_secret)
            .expect("authenticate policy");

        let options = RuntimeOptions {
            bind_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
            san_hosts: vec!["localhost".to_owned()],
            agent: AgentConfig {
                poll_interval_ms: 100,
                notification_queue_capacity: 32,
                alert_thresholds: AlertThresholds {
                    enabled: true,
                    cpu_usage_pct: Some(90),
                    ram_usage_pct: Some(95),
                    cpu_temp_c: Some(85),
                    gpu_temp_c: Some(90),
                    battery_low_pct: Some(15),
                    cooldown_secs: 60,
                },
                ..AgentConfig::default()
            },
        };

        // Phase 1: Start runtime with high-load snapshot so HardwareAlertMonitor triggers events,
        // and submit both an app notification and an expiring + an active approval request.
        let agent = spawn_agent_runtime_full(
            options.clone(),
            Box::new(StaticCollector::new(high_load_snapshot())),
            Box::new(RecordingControlExecutor::new()),
            tls.clone(),
            Some(paths.clone()),
            Vec::new(),
            clients_reg.export_records(),
        )
        .await
        .expect("spawn phase 1 agent");

        let mut broadcast_rx = agent.subscribe_broadcast();

        let app_notif = build_notification_event(
            &policy,
            900,
            "Deploy Ready".to_owned(),
            "Synthetic build artifact ready for review".to_owned(),
            NotificationPriority::High,
            now_epoch_ms(),
        )
        .expect("build notification");
        agent.notifications().write().await.push(app_notif);

        let now = now_epoch_ms();
        {
            let mut approvals = agent.approvals().write().await;
            submit_approval_request(
                &policy,
                &mut approvals,
                ApprovalRequestPayload {
                    request_id: "req-example-expire".to_owned(),
                    application: "ExampleApp".to_owned(),
                    title: "Short Timeout Task".to_owned(),
                    description: "Should auto-expire quickly".to_owned(),
                    timeout_secs: 1,
                    actions: vec!["approve".to_owned(), "deny".to_owned()],
                    risk_level: RiskLevel::Low,
                },
                now.saturating_sub(2_500),
            )
            .expect("register soon-to-expire request");

            submit_approval_request(
                &policy,
                &mut approvals,
                ApprovalRequestPayload {
                    request_id: "req-example-live".to_owned(),
                    application: "ExampleApp".to_owned(),
                    title: "Apply Workspace Config".to_owned(),
                    description: "Allow ExampleApp to apply settings preset?".to_owned(),
                    timeout_secs: 120,
                    actions: vec!["approve".to_owned(), "deny".to_owned()],
                    risk_level: RiskLevel::Medium,
                },
                now,
            )
            .expect("register live request");
        }

        // Wait briefly for ticks to evaluate alerts & expire stale request.
        let mut saw_hw_alert = false;
        let mut saw_expired_broadcast = false;
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        while tokio::time::Instant::now() < deadline && (!saw_hw_alert || !saw_expired_broadcast) {
            if let Ok(Ok(json)) =
                tokio::time::timeout(Duration::from_millis(250), broadcast_rx.recv()).await
                && let Ok(env) = serde_json::from_str::<Envelope>(&json)
            {
                match env.kind {
                    MessageKind::Notification => {
                        if let Ok(event) = serde_json::from_slice::<
                            nexdeck_notifications::NotificationEvent,
                        >(&env.payload)
                            && event.source == NotificationSource::HardwareAlert
                        {
                            saw_hw_alert = true;
                        }
                    }
                    MessageKind::Approval => {
                        if let Ok(req) = serde_json::from_slice::<ApprovalRequest>(&env.payload)
                            && req.request_id == "req-example-expire"
                            && req.status == ApprovalStatus::Expired
                        {
                            saw_expired_broadcast = true;
                        }
                    }
                    _ => {}
                }
            }
        }
        assert!(saw_hw_alert, "HardwareAlertMonitor must emit alert event");
        assert!(
            saw_expired_broadcast,
            "runtime must sweep and broadcast expired approval requests"
        );

        // Resolve live request once and block replay.
        let decision_env = process_approval_decision(
            agent.approvals(),
            &ApprovalDecisionMessage {
                request_id: "req-example-live".to_owned(),
                action: "approve".to_owned(),
            },
            100,
            now_epoch_ms(),
        )
        .await
        .expect("live request resolves cleanly");
        let resolved: ApprovalRequest =
            serde_json::from_slice(&decision_env.payload).expect("decode resolved request");
        assert_eq!(
            resolved.status,
            ApprovalStatus::Resolved("approve".to_owned())
        );

        let replay_env = process_approval_decision(
            agent.approvals(),
            &ApprovalDecisionMessage {
                request_id: "req-example-live".to_owned(),
                action: "deny".to_owned(),
            },
            101,
            now_epoch_ms(),
        )
        .await
        .expect("replay check returns authoritative status");
        let replay_req: ApprovalRequest =
            serde_json::from_slice(&replay_env.payload).expect("decode replay request");
        assert_eq!(
            replay_req.status,
            ApprovalStatus::Resolved("approve".to_owned())
        );

        agent.shutdown().await.expect("clean phase 1 shutdown");

        // Phase 2: Verify sleep-buffered notifications survived restart in notifications.json,
        // then dismiss all via NotificationSyncCommand.
        let persisted = paths
            .load_notifications()
            .expect("notifications.json persisted on disk");
        assert!(
            persisted.len() >= 2,
            "both app and hardware notifications must be retained in sleep buffer"
        );

        let agent2 = spawn_agent_runtime_full(
            options,
            Box::new(StaticCollector::new(high_load_snapshot())),
            Box::new(RecordingControlExecutor::new()),
            tls,
            Some(paths.clone()),
            Vec::new(),
            clients_reg.export_records(),
        )
        .await
        .expect("spawn phase 2 agent");

        {
            let q = agent2.notifications().read().await;
            let items = q.items_matching(&NotificationFilter::default());
            assert_eq!(items.len(), persisted.len());
        }

        process_notification_sync(
            agent2.notifications(),
            agent2.notification_filter(),
            Some(&paths),
            NotificationSyncCommand::ClearAll,
        )
        .await;
        agent2.shutdown().await.expect("clean phase 2 shutdown");

        assert!(
            paths
                .load_notifications()
                .expect("reload cleared notifications")
                .is_empty()
        );

        let _ = fs::remove_dir_all(&temp_dir);
    });
}
