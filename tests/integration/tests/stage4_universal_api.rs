//! Stage 4 integration tests for Universal API client registry persistence and runtime synchronization.

use std::fs;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use nexdeck_api::ApiPermission;
use nexdeck_api::clients::ClientRegistry;
use nexdeck_core::AgentConfig;
use nexdeck_core::control::RecordingControlExecutor;
use nexdeck_core::runtime::{RuntimeOptions, spawn_agent_runtime_full};
use nexdeck_core::storage::StoragePaths;
use nexdeck_monitoring::{CpuMetrics, MemoryMetrics, StaticCollector, SystemSnapshot};
use nexdeck_transport::tls::HostTlsIdentity;

fn sample_snapshot() -> SystemSnapshot {
    SystemSnapshot {
        captured_at_epoch_ms: 50_000,
        uptime_secs: 900,
        cpu: CpuMetrics {
            total_usage_pct: 19.5,
            per_core_usage_pct: vec![19.5],
            frequency_mhz: Some(3_900),
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
    }
}

#[test]
fn universal_api_clients_persist_and_sync_with_agent_runtime() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build runtime");

    rt.block_on(async {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let temp_dir = std::env::temp_dir().join(format!(
            "nexdeck_stage4_test_{}_{nanos}",
            std::process::id()
        ));
        let paths = StoragePaths::new(temp_dir.clone());

        let mut initial_reg = ClientRegistry::new();
        let issued = initial_reg
            .register_client(
                "org.example.integration".to_owned(),
                "ExampleIntegration".to_owned(),
                [ApiPermission::ReadMetrics, ApiPermission::RequestApproval],
                1_700_000_000_000,
            )
            .expect("register initial client");
        paths
            .save_clients(&initial_reg.export_records())
            .expect("save initial clients.json");

        let tls = HostTlsIdentity::generate_self_signed(&["localhost".to_owned()])
            .expect("generate self-signed cert");
        let options = RuntimeOptions {
            bind_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 0),
            san_hosts: vec!["localhost".to_owned()],
            agent: AgentConfig {
                poll_interval_ms: 100,
                ..AgentConfig::default()
            },
        };

        let agent = spawn_agent_runtime_full(
            options,
            Box::new(StaticCollector::new(sample_snapshot())),
            Box::new(RecordingControlExecutor::new()),
            tls,
            Some(paths.clone()),
            Vec::new(),
            paths.load_clients().expect("load clients.json"),
        )
        .await
        .expect("spawn runtime");

        {
            let guard = agent.clients().read().await;
            let policy = guard
                .authenticate(&issued.access_secret)
                .expect("authenticate registered client");
            assert_eq!(policy.application_name, "ExampleIntegration");
        }

        // Register a second client dynamically while runtime is active and verify disk flush.
        {
            let mut guard = agent.clients().write().await;
            let _ = guard
                .register_client(
                    "org.example.second".to_owned(),
                    "ExampleSecond".to_owned(),
                    [ApiPermission::SendNotification],
                    1_700_000_001_000,
                )
                .expect("register second client");
        }

        tokio::time::sleep(Duration::from_millis(160)).await;
        agent.shutdown().await.expect("clean shutdown");

        let persisted = paths.load_clients().expect("reload clients.json");
        assert_eq!(persisted.len(), 2);
        assert_eq!(persisted[0].client_id, "org.example.integration");
        assert_eq!(persisted[1].client_id, "org.example.second");

        let _ = fs::remove_dir_all(&temp_dir);
    });
}
