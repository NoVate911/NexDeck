//! Stage 1 end-to-end integration tests for Core Agent storage, TLS/WSS runtime, and telemetry.

use std::fs;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use nexdeck_core::runtime::{RuntimeOptions, spawn_agent_runtime};
use nexdeck_core::storage::{AgentSettings, StoragePaths};
use nexdeck_monitoring::{CpuMetrics, MemoryMetrics, StaticCollector, SystemSnapshot};
use nexdeck_transport::{Envelope, MessageKind};

fn now_epoch_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(1_000_000, |d| {
            u64::try_from(d.as_millis()).unwrap_or(u64::MAX)
        })
}

fn synthetic_snapshot() -> SystemSnapshot {
    SystemSnapshot {
        captured_at_epoch_ms: 123_456,
        uptime_secs: 7_200,
        cpu: CpuMetrics {
            total_usage_pct: 22.5,
            per_core_usage_pct: vec![20.0, 25.0],
            frequency_mhz: Some(3_600),
            temperature_celsius: None,
        },
        memory: MemoryMetrics {
            total_bytes: 32 * 1024 * 1024 * 1024,
            used_bytes: 12 * 1024 * 1024 * 1024,
        },
        gpus: Vec::new(),
        disks: Vec::new(),
        networks: Vec::new(),
        battery: None,
    }
}

#[test]
fn core_agent_storage_pairing_and_telemetry_flow_end_to_end() {
    let temp_dir = std::env::temp_dir().join(format!(
        "nexdeck-stage1-int-{}-{}",
        std::process::id(),
        now_epoch_ms()
    ));
    let storage = StoragePaths::new(temp_dir.clone());

    let mut settings = storage
        .load_or_init_settings()
        .expect("default settings should initialize");
    assert_eq!(settings, AgentSettings::default());
    settings.agent.poll_interval_ms = 100;
    storage
        .save_settings(&settings)
        .expect("updated settings should persist");

    let tls = storage
        .load_or_create_tls_identity(&settings.san_hosts)
        .expect("TLS identity should generate");
    let devices = storage.load_devices().expect("empty devices should load");

    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("current_thread runtime should build");

    rt.block_on(async {
        let options = RuntimeOptions {
            bind_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
            san_hosts: settings.san_hosts.clone(),
            agent: settings.agent.clone(),
        };
        let collector = Box::new(StaticCollector::new(synthetic_snapshot()));
        let agent = spawn_agent_runtime(
            options,
            collector,
            tls.clone(),
            Some(storage.clone()),
            devices,
        )
        .await
        .expect("agent runtime should start");

        let mut rx = agent.subscribe_broadcast();
        let pairing = agent
            .start_pairing("127.0.0.1", 180)
            .await
            .expect("pairing should start");

        let raw_envelope = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("telemetry should publish")
            .expect("broadcast channel should stay open");
        let envelope: Envelope = serde_json::from_str(&raw_envelope).expect("valid envelope");
        assert_eq!(envelope.kind, MessageKind::Telemetry);

        let decoded: SystemSnapshot =
            serde_json::from_slice(&envelope.payload).expect("valid serialized snapshot");
        assert_eq!(decoded, synthetic_snapshot());

        {
            let mut reg = agent.registry().write().await;
            let creds = reg
                .complete_pairing(
                    &pairing.pin,
                    "example-client-1".to_owned(),
                    "ExamplePhone".to_owned(),
                    tls.fingerprint_sha256.clone(),
                    now_epoch_ms(),
                )
                .expect("pairing should succeed");
            assert_eq!(creds.fingerprint_sha256, tls.fingerprint_sha256);
        }

        agent.shutdown().await.expect("agent should stop cleanly");
    });

    let saved = storage
        .load_devices()
        .expect("devices.json should persist paired client");
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].device_id, "example-client-1");
    assert_eq!(saved[0].device_name, "ExamplePhone");

    let _ = fs::remove_dir_all(&temp_dir);
}
