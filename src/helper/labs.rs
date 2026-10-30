//! Lab orchestration on a Docker engine (through a socket proxy, see docker-compose.yml).
//! Each instance gets its own bridge network and one container whose port is published on
//! a random host port: players reach it at INSTANCE_HOST:port (HTTP or raw TCP).
//! ponytail: one Docker host; a second backend (Kubernetes) would implement launch/destroy/sweep.
use crate::helper::config::CONFIG;
use crate::helper::trace::{trace_error, trace_logs};
use crate::models::instance::{self, Runtime};
use bollard::Docker;
use bollard::container::{
    Config, CreateContainerOptions, ListContainersOptions, RemoveContainerOptions,
};
use bollard::image::CreateImageOptions;
use bollard::models::{HostConfig, PortBinding};
use bollard::network::{CreateNetworkOptions, ListNetworksOptions};
use futures::StreamExt;
use once_cell::sync::Lazy;
use std::collections::{HashMap, HashSet};

static DOCKER: Lazy<Option<Docker>> = Lazy::new(|| {
    std::env::var("DOCKER_HOST")
        .ok()
        .filter(|h| !h.is_empty())?;
    match Docker::connect_with_defaults() {
        Ok(d) => Some(d),
        Err(e) => {
            trace_error(format!("labs disabled: {e}"));
            None
        }
    }
});

/// Host shown to players; defaults to the host of BASE_URL.
pub static INSTANCE_HOST: Lazy<String> = Lazy::new(|| {
    std::env::var("INSTANCE_HOST")
        .ok()
        .filter(|h| !h.is_empty())
        .unwrap_or_else(|| {
            let after = CONFIG.base_url.split("://").nth(1).unwrap_or("localhost");
            let host = after.split('/').next().unwrap_or("localhost");
            host.rsplit_once(':').map_or(host, |(h, _)| h).to_string()
        })
});
static BIND: Lazy<String> =
    Lazy::new(|| std::env::var("INSTANCE_BIND").unwrap_or_else(|_| "0.0.0.0".into()));
pub static MAX_INSTANCES: Lazy<i64> = Lazy::new(|| {
    std::env::var("MAX_INSTANCES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(50)
});

pub fn enabled() -> bool {
    DOCKER.is_some()
}

const LABEL: &str = "org.livehack.instance";
const NS_LABEL: &str = "org.livehack.ns";
/// Deployment namespace: several platforms can share one Docker host without touching
/// each other's labs (all replicas of one platform share it).
static NAMESPACE: Lazy<String> = Lazy::new(|| {
    std::env::var("LABS_NAMESPACE")
        .ok()
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| "livehack".into())
});

async fn ensure_image(d: &Docker, image: &str) -> Result<(), String> {
    if d.inspect_image(image).await.is_ok() {
        return Ok(());
    }
    let mut pull = d.create_image(
        Some(CreateImageOptions {
            from_image: image,
            ..Default::default()
        }),
        None,
        None,
    );
    while let Some(step) = pull.next().await {
        step.map_err(|e| format!("pull {image}: {e}"))?;
    }
    Ok(())
}

/// Starts the lab; returns (container id, network id, host port).
pub async fn launch(
    inst: &str,
    tenant: &str,
    rt: &Runtime,
    flag: &str,
) -> Result<(String, String, i32), String> {
    let d = DOCKER
        .as_ref()
        .ok_or("Labs are not configured on this platform.")?;
    ensure_image(d, &rt.image).await?;
    let labels: HashMap<&str, &str> = HashMap::from([
        (LABEL, inst),
        (NS_LABEL, NAMESPACE.as_str()),
        ("org.livehack.tenant", tenant),
    ]);
    let net_name = format!("lh-net-{inst}");
    let net = d
        .create_network(CreateNetworkOptions {
            name: net_name.as_str(),
            driver: "bridge",
            labels: labels.clone(),
            options: HashMap::from([("com.docker.network.bridge.enable_icc", "false")]),
            ..Default::default()
        })
        .await
        .map_err(|e| format!("network: {e}"))?;
    let net_id = if net.id.is_empty() {
        net_name.clone()
    } else {
        net.id
    };

    let port_key = format!("{}/tcp", rt.port);
    let mut env = rt.env_pairs();
    env.push(format!("FLAG={flag}"));
    let tmpfs = rt
        .readonly_rootfs
        .then(|| HashMap::from([("/tmp".to_string(), "rw,size=64m".to_string())]));
    let host = HostConfig {
        port_bindings: Some(HashMap::from([(
            port_key.clone(),
            Some(vec![PortBinding {
                host_ip: Some(BIND.clone()),
                host_port: Some(String::new()),
            }]),
        )])),
        memory: Some(rt.memory_mb as i64 * 1024 * 1024),
        memory_swap: Some(rt.memory_mb as i64 * 1024 * 1024),
        nano_cpus: Some(rt.cpu_millis as i64 * 1_000_000),
        pids_limit: Some(256),
        cap_drop: Some(vec![
            "NET_RAW".into(),
            "MKNOD".into(),
            "AUDIT_WRITE".into(),
            "SYS_CHROOT".into(),
            "SETFCAP".into(),
        ]),
        security_opt: Some(vec!["no-new-privileges:true".into()]),
        readonly_rootfs: Some(rt.readonly_rootfs),
        tmpfs,
        network_mode: Some(net_name.clone()),
        ..Default::default()
    };
    let config = Config {
        image: Some(rt.image.clone()),
        env: Some(env),
        exposed_ports: Some(HashMap::from([(port_key.clone(), HashMap::new())])),
        labels: Some(
            labels
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        ),
        host_config: Some(host),
        ..Default::default()
    };
    let name = format!("lh-inst-{inst}");
    let started = async {
        let c = d
            .create_container(
                Some(CreateContainerOptions {
                    name: name.as_str(),
                    platform: None,
                }),
                config,
            )
            .await
            .map_err(|e| format!("create: {e}"))?;
        d.start_container::<String>(&c.id, None)
            .await
            .map_err(|e| format!("start: {e}"))?;
        let info = d
            .inspect_container(&c.id, None)
            .await
            .map_err(|e| format!("inspect: {e}"))?;
        let port = info
            .network_settings
            .and_then(|n| n.ports)
            .and_then(|p| p.get(&port_key).cloned().flatten())
            .and_then(|b| b.into_iter().find_map(|b| b.host_port?.parse::<i32>().ok()))
            .ok_or("no published port")?;
        Ok::<_, String>((c.id, port))
    }
    .await;
    match started {
        Ok((id, port)) => Ok((id, net_id, port)),
        Err(e) => {
            destroy(&name, &net_id).await;
            Err(e)
        }
    }
}

/// Removes container and network; errors are logged, never fatal.
pub async fn destroy(container: &str, network: &str) {
    let Some(d) = DOCKER.as_ref() else { return };
    let opts = RemoveContainerOptions {
        force: true,
        ..Default::default()
    };
    if !container.is_empty()
        && let Err(e) = d.remove_container(container, Some(opts)).await
        && !e.to_string().contains("No such container")
    {
        trace_error(format!("remove container {container}: {e}"));
    }
    if !network.is_empty() {
        let _ = d.remove_network(network).await;
    }
}

/// Starts a reserved instance in the background and records the outcome.
pub fn spawn_launch(inst: String, tenant: String, rt: Runtime, flag: String) {
    actix_web::rt::spawn(async move {
        match launch(&inst, &tenant, &rt, &flag).await {
            Ok((container, network, port)) => {
                if let Err(e) = instance::mark_running(&inst, &container, &network, port).await {
                    trace_error(format!("instance {inst}: {e}"));
                    destroy(&container, &network).await;
                }
            }
            Err(e) => {
                trace_error(format!("instance {inst} failed: {e}"));
                let _ = instance::mark_stopped(&inst, Some(&e)).await;
            }
        }
    });
}

/// Stops expired instances and removes containers/networks that no live instance owns
/// (crash, restart, manual deletes). Safe to run on every replica.
pub async fn sweep() {
    let Some(d) = DOCKER.as_ref() else { return };
    match instance::expired().await {
        Ok(list) => {
            for i in list {
                destroy(&i.container_id, &i.network_id).await;
                let _ = instance::mark_stopped(&i.uuid, None).await;
                trace_logs(format!("instance {} stopped (expired)", i.uuid));
            }
        }
        Err(e) => trace_error(format!("sweep: {e}")),
    }
    let Ok(live) = instance::live().await else {
        return;
    };
    let keep: HashSet<String> = live.iter().map(|i| i.uuid.clone()).collect();
    let ns = format!("{NS_LABEL}={}", *NAMESPACE);
    let filters = HashMap::from([("label", vec![LABEL, ns.as_str()])]);
    if let Ok(containers) = d
        .list_containers(Some(ListContainersOptions {
            all: true,
            filters: filters.clone(),
            ..Default::default()
        }))
        .await
    {
        for c in containers {
            let owner = c
                .labels
                .as_ref()
                .and_then(|l| l.get(LABEL))
                .cloned()
                .unwrap_or_default();
            if !keep.contains(&owner) {
                destroy(c.id.as_deref().unwrap_or(""), "").await;
            }
        }
    }
    if let Ok(nets) = d.list_networks(Some(ListNetworksOptions { filters })).await {
        for n in nets {
            let owner = n
                .labels
                .as_ref()
                .and_then(|l| l.get(LABEL))
                .cloned()
                .unwrap_or_default();
            if !keep.contains(&owner) {
                let _ = d.remove_network(n.id.as_deref().unwrap_or("")).await;
            }
        }
    }
}

/// A fresh flag following the platform format: "LH{...}" → "LH{3f9c…}".
pub fn new_flag(format: &str) -> String {
    let secret = &crate::helper::crypto::random_token()[..24];
    match format.split_once("...") {
        Some((a, b)) => format!("{a}{secret}{b}"),
        None => format!("LH{{{secret}}}"),
    }
}

#[cfg(test)]
mod tests {
    use super::new_flag;

    #[test]
    fn flags_follow_the_format() {
        let f = new_flag("CTF{...}");
        assert!(f.starts_with("CTF{") && f.ends_with('}') && f.len() == 29);
        assert!(new_flag("").starts_with("LH{"));
        assert_ne!(new_flag("x..."), new_flag("x..."));
    }
}
