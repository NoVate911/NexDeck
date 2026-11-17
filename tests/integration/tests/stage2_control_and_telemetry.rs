//! Stage 2 integration tests verifying allowlisted control commands and runtime routing.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

use nexdeck_core::AgentConfig;
use nexdeck_core::control::RecordingControlExecutor;
use nexdeck_core::runtime::{
    RuntimeOptions, build_inbound_reply, spawn_agent_runtime_with_executor,
};
use nexdeck_monitoring::{CpuMetrics, MemoryMetrics, StaticCollector, SystemSnapshot};
use nexdeck_transport::control::{ControlAction, ControlCommandRequest, ControlCommandResponse};
use nexdeck_transport::server::InboundFrame;
use nexdeck_transport::tls::HostTlsIdentity;
use nexdeck_transport::{Envelope, MessageKind};

fn sample_snapshot() -> SystemSnapshot {
    SystemSnapshot {
        captured_at_epoch_ms: 99_000,
        uptime_secs: 4_200,
        cpu: CpuMetrics {
            total_usage_pct: 33.0,
            per_core_usage_pct: vec![30.0, 36.0],
            frequency_mhz: Some(4_000),
            temperature_celsius: Some(54.5),
        },
        memory: MemoryMetrics {
            total_bytes: 16 * 1024 * 1024 * 1024,
            used_bytes: 8 * 1024 * 1024 * 1024,
        },
        gpus: Vec::new(),
        disks: Vec::new(),
        networks: Vec::new(),
        battery: None,
    }
}

#[test]
fn control_executor_and_runtime_reply_handle_all_media_actions() {
    let executor = RecordingControlExecutor::new();
    let actions = [
        ControlAction::VolumeUp,
        ControlAction::VolumeDown,
        ControlAction::VolumeMuteToggle,
        ControlAction::MediaPlayPause,
        ControlAction::MediaNextTrack,
        ControlAction::MediaPreviousTrack,
    ];

    for (idx, action) in actions.iter().enumerate() {
        let req = ControlCommandRequest {
            command_id: format!("cmd-stage2-{idx}"),
            action: *action,
        };
        let frame = InboundFrame {
            device_id: "example-client-1".to_owned(),
            envelope: Envelope::new(
                (idx + 1) as u64,
                MessageKind::ControlCommand,
                serde_json::to_vec(&req).expect("encode control request"),
            ),
        };
        let reply = build_inbound_reply(&executor, &frame, (idx + 10) as u64)
            .expect("control reply should be built");
        let parsed: ControlCommandResponse =
            serde_json::from_slice(&reply.payload).expect("decode control response");
        assert!(parsed.ok);
        assert_eq!(parsed.command_id, format!("cmd-stage2-{idx}"));
        assert_eq!(parsed.action, *action);
    }

    assert_eq!(executor.actions(), actions);
}

#[test]
fn runtime_with_custom_executor_starts_and_publishes_detailed_snapshot() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio runtime");

    rt.block_on(async {
        let tls =
            HostTlsIdentity::generate_self_signed(&["localhost".to_owned()]).expect("TLS identity");
        let options = RuntimeOptions {
            bind_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
            san_hosts: vec!["localhost".to_owned()],
            agent: AgentConfig {
                poll_interval_ms: 100,
                ..AgentConfig::default()
            },
        };
        let executor = RecordingControlExecutor::new();
        let collector = Box::new(StaticCollector::new(sample_snapshot()));

        let agent = spawn_agent_runtime_with_executor(
            options,
            collector,
            Box::new(executor),
            tls,
            None,
            Vec::new(),
        )
        .await
        .expect("runtime should start");

        let mut rx = agent.subscribe_broadcast();
        let json = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("telemetry should publish")
            .expect("channel open");
        let envelope: Envelope = serde_json::from_str(&json).expect("decode envelope");
        let snapshot: SystemSnapshot =
            serde_json::from_slice(&envelope.payload).expect("decode snapshot");
        assert_eq!(snapshot.cpu.per_core_usage_pct, vec![30.0, 36.0]);
        assert_eq!(snapshot.cpu.temperature_celsius, Some(54.5));

        agent.shutdown().await.expect("clean shutdown");
    });
}
