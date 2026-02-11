use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};

use tokio::sync::RwLock;

use crate::prism::tunnel::{
    protocol::{self, ProxyStreamKind, RegisteredService},
    transport::{BoxedStream, TransportSession},
};

#[derive(Debug, thiserror::Error)]
pub enum ManagerError {
    #[error("service not found")]
    ServiceNotFound,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ServiceSnapshot {
    pub service: RegisteredService,
    pub client_id: String,
    pub remote: String,
    pub primary: bool,
}

struct ClientConn {
    id: String,
    sess: Arc<dyn TransportSession>,
    services: HashMap<String, RegisteredService>,
    remote: String,
    started: Instant,
}

struct State {
    clients: HashMap<String, ClientConn>,
    primary: HashMap<String, String>,
}

pub struct Manager {
    id_seq: AtomicU64,
    state: RwLock<State>,
    changed: tokio::sync::watch::Sender<u64>,
}

impl std::fmt::Debug for Manager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Manager").finish_non_exhaustive()
    }
}

impl Manager {
    pub fn new() -> Self {
        let (tx, _rx) = tokio::sync::watch::channel(0u64);
        Self {
            id_seq: AtomicU64::new(1),
            state: RwLock::new(State {
                clients: HashMap::new(),
                primary: HashMap::new(),
            }),
            changed: tx,
        }
    }

    pub fn subscribe(&self) -> tokio::sync::watch::Receiver<u64> {
        self.changed.subscribe()
    }

    pub fn next_client_id(&self, prefix: &str) -> String {
        let p = if prefix.trim().is_empty() {
            "c"
        } else {
            prefix.trim()
        };
        let n = self.id_seq.fetch_add(1, Ordering::Relaxed);
        format!("{p}-{n}")
    }

    pub async fn register_client(
        &self,
        id: String,
        sess: Arc<dyn TransportSession>,
        services: Vec<RegisteredService>,
    ) -> anyhow::Result<()> {
        if id.trim().is_empty() {
            anyhow::bail!("tunnel: empty client id");
        }

        let mut cc = ClientConn {
            id: id.clone(),
            sess,
            services: HashMap::new(),
            remote: String::new(),
            started: Instant::now(),
        };
        if let Some(ra) = cc.sess.remote_addr() {
            cc.remote = ra.to_string();
        }
        for s in services {
            if let Some(ns) = s.normalize() {
                cc.services.insert(ns.name.clone(), ns);
            }
        }

        let mut st = self.state.write().await;

        // Replace any existing client with the same id.
        if let Some(old) = st.clients.remove(&id) {
            old.sess.close().await;
            for name in old.services.keys() {
                if st.primary.get(name).is_some_and(|v| v == &id) {
                    st.primary.remove(name);
                    promote_primary_locked(&mut st, name);
                }
            }
        }

        // First writer wins for routing ownership.
        for name in cc.services.keys() {
            st.primary.entry(name.clone()).or_insert_with(|| id.clone());
        }

        st.clients.insert(id.clone(), cc);
        drop(st);

        self.bump_changed();
        Ok(())
    }

    pub async fn unregister_client(&self, id: &str) {
        let id = id.trim();
        if id.is_empty() {
            return;
        }

        let mut st = self.state.write().await;
        let Some(old) = st.clients.remove(id) else {
            return;
        };

        for name in old.services.keys() {
            if st.primary.get(name).is_some_and(|v| v == id) {
                st.primary.remove(name);
                promote_primary_locked(&mut st, name);
            }
        }
        drop(st);
        old.sess.close().await;
        self.bump_changed();
    }

    pub async fn snapshot_services(&self) -> Vec<ServiceSnapshot> {
        let st = self.state.read().await;
        let mut out = Vec::new();
        for (cid, cc) in &st.clients {
            for (name, svc) in &cc.services {
                out.push(ServiceSnapshot {
                    service: svc.clone(),
                    client_id: cid.clone(),
                    remote: cc.remote.clone(),
                    primary: st.primary.get(name).is_some_and(|v| v == cid),
                });
            }
        }
        out
    }

    pub async fn has_service(&self, service: &str) -> bool {
        let st = self.state.read().await;
        st.primary.contains_key(service.trim())
    }

    pub async fn dial_service_tcp(&self, service: &str) -> Result<BoxedStream, ManagerError> {
        let (st, _svc) = self.dial_service_tcp_inner(None, service).await?;
        Ok(st)
    }

    pub async fn dial_service_tcp_with_meta(
        &self,
        service: &str,
    ) -> Result<(BoxedStream, RegisteredService), ManagerError> {
        self.dial_service_tcp_inner(None, service).await
    }

    pub async fn dial_service_tcp_from_client(
        &self,
        client_id: &str,
        service: &str,
    ) -> Result<BoxedStream, ManagerError> {
        let (st, _svc) = self
            .dial_service_tcp_inner(Some(client_id), service)
            .await?;
        Ok(st)
    }

    pub async fn dial_service_tcp_from_client_with_meta(
        &self,
        client_id: &str,
        service: &str,
    ) -> Result<(BoxedStream, RegisteredService), ManagerError> {
        self.dial_service_tcp_inner(Some(client_id), service).await
    }

    pub async fn dial_service_udp(&self, service: &str) -> Result<BoxedStream, ManagerError> {
        self.dial_service_udp_inner(None, service).await
    }

    pub async fn dial_service_udp_from_client(
        &self,
        client_id: &str,
        service: &str,
    ) -> Result<BoxedStream, ManagerError> {
        self.dial_service_udp_inner(Some(client_id), service).await
    }

    async fn dial_service_tcp_inner(
        &self,
        client_id: Option<&str>,
        service: &str,
    ) -> Result<(BoxedStream, RegisteredService), ManagerError> {
        let service = service.trim();
        if service.is_empty() {
            return Err(ManagerError::ServiceNotFound);
        }

        let (sess, svc): (Arc<dyn TransportSession>, RegisteredService) = {
            let st = self.state.read().await;
            let cid = if let Some(pinned) = client_id {
                pinned.trim().to_string()
            } else {
                st.primary
                    .get(service)
                    .cloned()
                    .ok_or(ManagerError::ServiceNotFound)?
            };

            let cc = st.clients.get(&cid).ok_or(ManagerError::ServiceNotFound)?;
            let svc = cc
                .services
                .get(service)
                .cloned()
                .ok_or(ManagerError::ServiceNotFound)?;
            (cc.sess.clone(), svc)
        };

        let mut st = sess
            .open_stream()
            .await
            .map_err(|_| ManagerError::ServiceNotFound)?;
        protocol::write_proxy_stream_header(&mut st, ProxyStreamKind::Tcp, service)
            .await
            .map_err(|_| ManagerError::ServiceNotFound)?;
        Ok((st, svc))
    }

    async fn dial_service_udp_inner(
        &self,
        client_id: Option<&str>,
        service: &str,
    ) -> Result<BoxedStream, ManagerError> {
        let service = service.trim();
        if service.is_empty() {
            return Err(ManagerError::ServiceNotFound);
        }

        let sess: Arc<dyn TransportSession> = {
            let st = self.state.read().await;
            let cid = if let Some(pinned) = client_id {
                pinned.trim().to_string()
            } else {
                st.primary
                    .get(service)
                    .cloned()
                    .ok_or(ManagerError::ServiceNotFound)?
            };

            let cc = st.clients.get(&cid).ok_or(ManagerError::ServiceNotFound)?;
            if !cc.services.contains_key(service) {
                return Err(ManagerError::ServiceNotFound);
            }
            cc.sess.clone()
        };

        let mut st = sess
            .open_stream()
            .await
            .map_err(|_| ManagerError::ServiceNotFound)?;
        protocol::write_proxy_stream_header(&mut st, ProxyStreamKind::Udp, service)
            .await
            .map_err(|_| ManagerError::ServiceNotFound)?;
        Ok(st)
    }

    fn bump_changed(&self) {
        let prev = *self.changed.borrow();
        let _ = self.changed.send(prev.wrapping_add(1));
    }
}

fn promote_primary_locked(st: &mut State, service_name: &str) {
    // Choose the oldest active client that provides this service.
    let mut chosen: Option<(String, Instant)> = None;
    for (cid, cc) in &st.clients {
        if !cc.services.contains_key(service_name) {
            continue;
        }
        if chosen.is_none() || cc.started < chosen.as_ref().unwrap().1 {
            chosen = Some((cid.clone(), cc.started));
        }
    }
    if let Some((cid, _)) = chosen {
        st.primary.insert(service_name.to_string(), cid);
    }
}
