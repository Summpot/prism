use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use tokio::sync::{Mutex, broadcast, mpsc, oneshot, watch};
use tokio_util::codec::Framed;

use crate::prism::auth::AuthIdentity;
use crate::prism::telemetry::{self, ReloadSignal};
use crate::prism::tunnel::manager::Manager;
use crate::prism::tunnel::transport::BoxedStream;

use super::codec::{
    decode_envelope, decode_msg, encode_msg, length_codec, reject_unsupported_version,
};
use super::types::{
    ADMIN_PROTO_V1, AdminError, AdminEvent, AdminMethod, AdminMsg, AdminPayload, CLIENT_FEATURES,
    ControlError, FEATURE_EVENTS, FEATURE_RPC, HelloRejectReason, TOPIC_CONNECTIONS,
    TOPIC_OPTIMIZER, TOPIC_RELOAD, TOPIC_SERVICES,
};

const HELLO_TIMEOUT: Duration = Duration::from_secs(5);
const RPC_TIMEOUT: Duration = Duration::from_secs(15);
const EVENT_POLL: Duration = Duration::from_secs(1);

#[derive(Clone, Debug)]
pub struct AdminCallContext {
    pub identity: Option<AuthIdentity>,
    pub features: u64,
}

impl AdminCallContext {
    pub fn has_feature(&self, feature: u64) -> bool {
        self.features & feature == feature
    }
}

#[derive(Clone, Default)]
pub struct AdminEventWatches {
    pub sessions: Option<telemetry::SharedSessions>,
    pub optimizer: Option<telemetry::SharedOptimizerRegistry>,
    pub manager: Option<Arc<Manager>>,
    pub reload: Option<watch::Receiver<ReloadSignal>>,
}

#[async_trait]
pub trait AdminControl: Send + Sync {
    fn features(&self) -> u64;
    fn auth_enabled(&self) -> bool;
    fn event_watches(&self) -> AdminEventWatches;
    async fn authenticate(&self, token: &str) -> Result<AuthIdentity, AdminError>;
    async fn dispatch(
        &self,
        method: AdminMethod,
        ctx: &AdminCallContext,
    ) -> Result<AdminPayload, AdminError>;
}

struct PendingMap {
    inner: Mutex<HashMap<u32, oneshot::Sender<Result<AdminPayload, AdminError>>>>,
}

impl PendingMap {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(HashMap::new()),
        })
    }

    async fn insert(&self, id: u32, tx: oneshot::Sender<Result<AdminPayload, AdminError>>) {
        self.inner.lock().await.insert(id, tx);
    }

    async fn take(&self, id: u32) -> Option<oneshot::Sender<Result<AdminPayload, AdminError>>> {
        self.inner.lock().await.remove(&id)
    }

    async fn fail_all(&self, err: AdminError) {
        let mut guard = self.inner.lock().await;
        for (_, tx) in guard.drain() {
            let _ = tx.send(Err(err.clone()));
        }
    }
}

/// Client handle for a negotiated `$admin` control channel.
pub struct ControlChannel {
    write_tx: mpsc::Sender<AdminMsg>,
    pending: Arc<PendingMap>,
    events: broadcast::Sender<AdminEvent>,
    features: u64,
    next_id: AtomicU32,
    closed: Arc<AtomicBool>,
    shutdown: watch::Sender<bool>,
}

impl ControlChannel {
    pub fn features(&self) -> u64 {
        self.features
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<AdminEvent> {
        self.events.subscribe()
    }

    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Relaxed)
    }

    pub async fn call(&self, method: AdminMethod) -> Result<AdminPayload, AdminError> {
        if self.is_closed() {
            return Err(AdminError::unavailable("admin channel closed"));
        }
        if self.features & method.feature() == 0 {
            return Err(AdminError::unavailable("method not negotiated"));
        }
        let mut id = self.next_id.fetch_add(1, Ordering::Relaxed);
        if id == 0 {
            id = self.next_id.fetch_add(1, Ordering::Relaxed);
        }
        let (tx, rx) = oneshot::channel();
        self.pending.insert(id, tx).await;
        if self
            .write_tx
            .send(AdminMsg::Request { id, method })
            .await
            .is_err()
        {
            let _ = self.pending.take(id).await;
            return Err(AdminError::unavailable("admin channel closed"));
        }
        match tokio::time::timeout(RPC_TIMEOUT, rx).await {
            Ok(Ok(res)) => res,
            Ok(Err(_)) => Err(AdminError::unavailable("admin channel closed")),
            Err(_) => {
                let _ = self.pending.take(id).await;
                Err(AdminError::internal("admin request timed out"))
            }
        }
    }

    pub fn close(&self) {
        let _ = self.shutdown.send(true);
        self.closed.store(true, Ordering::Relaxed);
    }
}

impl Drop for ControlChannel {
    fn drop(&mut self) {
        self.close();
    }
}

pub async fn connect(
    stream: BoxedStream,
    requested_features: u64,
) -> Result<Arc<ControlChannel>, ControlError> {
    let mut framed = Framed::new(stream, length_codec());
    let hello = encode_msg(&AdminMsg::Hello {
        features: requested_features,
    })?;
    framed.send(hello).await.map_err(frame_io)?;

    let first = tokio::time::timeout(HELLO_TIMEOUT, framed.next())
        .await
        .map_err(|_| ControlError::Handshake("hello timed out".into()))?
        .ok_or_else(|| ControlError::Handshake("stream closed during hello".into()))?
        .map_err(frame_io)?;

    let env = decode_envelope(&first)?;
    if env.proto != ADMIN_PROTO_V1 {
        return Err(ControlError::UnsupportedVersion(env.proto));
    }
    match decode_msg(&env)? {
        AdminMsg::HelloAck { features } => {
            if features & FEATURE_RPC == 0 {
                return Err(ControlError::Handshake(
                    "server did not agree FEATURE_RPC".into(),
                ));
            }
            Ok(spawn_client_session(framed, features))
        }
        AdminMsg::HelloReject { reason } => Err(ControlError::Rejected(reason)),
        other => Err(ControlError::Handshake(format!(
            "expected HelloAck, got {other:?}"
        ))),
    }
}

pub async fn serve(
    stream: BoxedStream,
    handler: Arc<dyn AdminControl>,
    identity: Option<AuthIdentity>,
) -> Result<(), ControlError> {
    serve_with_shared_identity(
        stream,
        handler,
        Arc::new(Mutex::new(identity)),
        None,
    )
    .await
}

/// Same as [`serve`], but identity is shared with the tunnel session.
///
/// `$admin` Authenticate writes through to `identity` and bumps
/// `on_identity_change` so the sidecar catalog can be re-filtered.
pub async fn serve_with_shared_identity(
    stream: BoxedStream,
    handler: Arc<dyn AdminControl>,
    identity: Arc<Mutex<Option<AuthIdentity>>>,
    on_identity_change: Option<watch::Sender<u64>>,
) -> Result<(), ControlError> {
    let mut framed = Framed::new(stream, length_codec());
    let first = tokio::time::timeout(HELLO_TIMEOUT, framed.next())
        .await
        .map_err(|_| ControlError::Handshake("hello timed out".into()))?
        .ok_or_else(|| ControlError::Handshake("stream closed during hello".into()))?
        .map_err(frame_io)?;

    let env = match decode_envelope(&first) {
        Ok(env) => env,
        Err(err) => {
            let _ = framed
                .send(encode_msg(&AdminMsg::HelloReject {
                    reason: HelloRejectReason::Malformed,
                })?)
                .await;
            return Err(err);
        }
    };

    if env.proto != ADMIN_PROTO_V1 {
        let _ = framed.send(reject_unsupported_version(env.proto)?).await;
        return Err(ControlError::UnsupportedVersion(env.proto));
    }

    let requested = match decode_msg(&env) {
        Ok(AdminMsg::Hello { features }) => features,
        Ok(_) => {
            let _ = framed
                .send(encode_msg(&AdminMsg::HelloReject {
                    reason: HelloRejectReason::Malformed,
                })?)
                .await;
            return Err(ControlError::Handshake("first message was not Hello".into()));
        }
        Err(err) => {
            let _ = framed
                .send(encode_msg(&AdminMsg::HelloReject {
                    reason: HelloRejectReason::Malformed,
                })?)
                .await;
            return Err(err);
        }
    };

    if requested & FEATURE_RPC == 0 {
        let _ = framed
            .send(encode_msg(&AdminMsg::HelloReject {
                reason: HelloRejectReason::Malformed,
            })?)
            .await;
        return Err(ControlError::Handshake(
            "client Hello omitted FEATURE_RPC".into(),
        ));
    }

    let agreed = requested & handler.features();
    framed
        .send(encode_msg(&AdminMsg::HelloAck { features: agreed })?)
        .await
        .map_err(frame_io)?;

    let (write_tx, mut write_rx) = mpsc::channel::<AdminMsg>(64);
    let topics = Arc::new(Mutex::new(0u64));
    let (shutdown_tx, mut shutdown_rx) = watch::channel(false);
    let mut event_task = None;
    if agreed & FEATURE_EVENTS != 0 {
        let watches = handler.event_watches();
        let write_tx = write_tx.clone();
        let topics = topics.clone();
        let mut shutdown = shutdown_tx.subscribe();
        event_task = Some(tokio::spawn(async move {
            run_event_loop(watches, topics, write_tx, &mut shutdown).await;
        }));
    }

    loop {
        tokio::select! {
            _ = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() {
                    break;
                }
            }
            out = write_rx.recv() => {
                let Some(msg) = out else { break };
                if framed.send(encode_msg(&msg)?).await.is_err() {
                    break;
                }
            }
            incoming = framed.next() => {
                let Some(frame) = incoming else { break };
                let frame = match frame {
                    Ok(f) => f,
                    Err(_) => break,
                };
                let msg = match decode_frame_or_reject(&frame) {
                    Ok(m) => m,
                    Err(_) => break,
                };
                match msg {
                    AdminMsg::Request { id, method } => {
                        if id == 0 {
                            break;
                        }
                        let result = handle_request(
                            handler.as_ref(),
                            &identity,
                            on_identity_change.as_ref(),
                            agreed,
                            topics.as_ref(),
                            method,
                        )
                        .await;
                        if framed
                            .send(encode_msg(&AdminMsg::Response { id, result })?)
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                    AdminMsg::Hello { .. } => {
                        let _ = framed
                            .send(encode_msg(&AdminMsg::HelloReject {
                                reason: HelloRejectReason::AlreadyNegotiated,
                            })?)
                            .await;
                        break;
                    }
                    _ => {}
                }
            }
        }
    }

    let _ = shutdown_tx.send(true);
    if let Some(task) = event_task {
        task.abort();
    }
    Ok(())
}

fn decode_frame_or_reject(buf: &[u8]) -> Result<AdminMsg, ControlError> {
    decode_msg(&decode_envelope(buf)?)
}

async fn handle_request(
    handler: &dyn AdminControl,
    identity: &Mutex<Option<AuthIdentity>>,
    on_identity_change: Option<&watch::Sender<u64>>,
    features: u64,
    topics: &Mutex<u64>,
    method: AdminMethod,
) -> Result<AdminPayload, AdminError> {
    if features & method.feature() == 0 {
        return Err(AdminError::unavailable("method not negotiated"));
    }

    if let AdminMethod::Authenticate { token } = &method {
        let ident = handler.authenticate(token).await?;
        *identity.lock().await = Some(ident.clone());
        if let Some(tx) = on_identity_change {
            let next = tx.borrow().saturating_add(1);
            let _ = tx.send(next);
        }
        return Ok(AdminPayload::AuthSession(session_from_identity(&ident)));
    }

    let current = identity.lock().await.clone();
    if let AdminMethod::Subscribe { topics: want } = method {
        if features & FEATURE_EVENTS == 0 {
            return Err(AdminError::unavailable("events not negotiated"));
        }
        if method_needs_auth(handler, &current, true) {
            return Err(session_error(&current));
        }
        let agreed_topics = want
            & (TOPIC_CONNECTIONS | TOPIC_SERVICES | TOPIC_OPTIMIZER | TOPIC_RELOAD);
        *topics.lock().await = agreed_topics;
        return Ok(AdminPayload::Subscribed {
            topics: agreed_topics,
        });
    }

    if method.requires_session() && method_needs_auth(handler, &current, method.requires_session())
    {
        return Err(session_error(&current));
    }

    let ctx = AdminCallContext {
        identity: current,
        features,
    };
    handler.dispatch(method, &ctx).await
}

fn method_needs_auth(
    handler: &dyn AdminControl,
    identity: &Option<AuthIdentity>,
    requires_session: bool,
) -> bool {
    if !requires_session || !handler.auth_enabled() {
        return false;
    }
    match identity {
        Some(id) if id.is_admin => false,
        Some(_) => true,
        None => true,
    }
}

fn session_error(identity: &Option<AuthIdentity>) -> AdminError {
    match identity {
        Some(id) if !id.is_admin => AdminError::forbidden("admin role required"),
        _ => AdminError::unauthorized("authentication required"),
    }
}

fn session_from_identity(ident: &AuthIdentity) -> super::types::AuthSessionSnapshot {
    super::types::AuthSessionSnapshot {
        authenticated: true,
        user_id: Some(ident.user_id.clone()),
        username: Some(ident.username.clone()),
        display_name: None,
        avatar_url: None,
        role: Some(format!("{:?}", ident.role).to_lowercase()),
        service_rules: ident.service_rules.clone(),
        is_admin: ident.is_admin,
    }
}

fn spawn_client_session(
    framed: Framed<BoxedStream, tokio_util::codec::LengthDelimitedCodec>,
    features: u64,
) -> Arc<ControlChannel> {
    let (mut sink, mut stream) = framed.split();
    let (write_tx, mut write_rx) = mpsc::channel::<AdminMsg>(64);
    let pending = PendingMap::new();
    let (events, _) = broadcast::channel(64);
    let (shutdown_tx, _) = watch::channel(false);
    let closed = Arc::new(AtomicBool::new(false));

    let pending_w = pending.clone();
    let closed_w = closed.clone();
    let mut shutdown_w = shutdown_tx.subscribe();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = shutdown_w.changed() => {
                    if *shutdown_w.borrow() {
                        break;
                    }
                }
                msg = write_rx.recv() => {
                    let Some(msg) = msg else { break };
                    match encode_msg(&msg) {
                        Ok(bytes) => {
                            if sink.send(bytes).await.is_err() {
                                break;
                            }
                        }
                        Err(_) => break,
                    }
                }
            }
        }
        let _ = SinkExt::close(&mut sink).await;
        closed_w.store(true, Ordering::Relaxed);
        pending_w
            .fail_all(AdminError::unavailable("admin channel closed"))
            .await;
    });

    let pending_r = pending.clone();
    let events_r = events.clone();
    let closed_r = closed.clone();
    let mut shutdown_r = shutdown_tx.subscribe();
    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = shutdown_r.changed() => {
                    if *shutdown_r.borrow() {
                        break;
                    }
                }
                next = stream.next() => {
                    let Some(frame) = next else { break };
                    let Ok(buf) = frame else { break };
                    let Ok(msg) = decode_frame_or_reject(&buf) else { break };
                    match msg {
                        AdminMsg::Response { id, result } => {
                            if let Some(tx) = pending_r.take(id).await {
                                let _ = tx.send(result);
                            }
                        }
                        AdminMsg::Event { event, .. } => {
                            let _ = events_r.send(event);
                        }
                        AdminMsg::HelloReject { .. } => break,
                        _ => {}
                    }
                }
            }
        }
        closed_r.store(true, Ordering::Relaxed);
        pending_r
            .fail_all(AdminError::unavailable("admin channel closed"))
            .await;
    });

    Arc::new(ControlChannel {
        write_tx,
        pending,
        events,
        features,
        next_id: AtomicU32::new(1),
        closed,
        shutdown: shutdown_tx,
    })
}

async fn run_event_loop(
    watches: AdminEventWatches,
    topics: Arc<Mutex<u64>>,
    write_tx: mpsc::Sender<AdminMsg>,
    shutdown: &mut watch::Receiver<bool>,
) {
    let mut seq = 1u32;
    let mut last: HashMap<u64, Vec<u8>> = HashMap::new();
    let mut reload_rx = watches.reload.clone();
    let mut services_rx = watches.manager.as_ref().map(|m| m.subscribe());
    let mut ticker = tokio::time::interval(EVENT_POLL);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    break;
                }
            }
            _ = async {
                if let Some(rx) = services_rx.as_mut() {
                    let _ = rx.changed().await;
                } else {
                    std::future::pending::<()>().await;
                }
            } => {
                let want = *topics.lock().await;
                if want & TOPIC_SERVICES != 0
                    && let Some(mgr) = &watches.manager
                {
                    let snap = mgr.snapshot_services().await;
                    push_event(
                        &write_tx,
                        &mut last,
                        &mut seq,
                        AdminEvent::TunnelServices(snap),
                    )
                    .await;
                }
            }
            _ = async {
                if let Some(rx) = reload_rx.as_mut() {
                    let _ = rx.changed().await;
                } else {
                    std::future::pending::<()>().await;
                }
            } => {
                let want = *topics.lock().await;
                if want & TOPIC_RELOAD != 0
                    && let Some(rx) = &reload_rx
                {
                    let seq_no = rx.borrow().seq;
                    push_event(
                        &write_tx,
                        &mut last,
                        &mut seq,
                        AdminEvent::Reload { seq: seq_no },
                    )
                    .await;
                }
            }
            _ = ticker.tick() => {
                let want = *topics.lock().await;
                if want & TOPIC_CONNECTIONS != 0
                    && let Some(sessions) = &watches.sessions
                {
                    push_event(
                        &write_tx,
                        &mut last,
                        &mut seq,
                        AdminEvent::Connections(sessions.snapshot()),
                    )
                    .await;
                }
                if want & TOPIC_OPTIMIZER != 0
                    && let Some(opt) = &watches.optimizer
                {
                    let (global, services) = opt.snapshot();
                    push_event(
                        &write_tx,
                        &mut last,
                        &mut seq,
                        AdminEvent::Optimizer { global, services },
                    )
                    .await;
                }
            }
        }
    }
}

async fn push_event(
    write_tx: &mpsc::Sender<AdminMsg>,
    last: &mut HashMap<u64, Vec<u8>>,
    seq: &mut u32,
    event: AdminEvent,
) {
    let topic = event.topic();
    let fingerprint = postcard::to_allocvec(&event).unwrap_or_default();
    if last.get(&topic) == Some(&fingerprint) {
        return;
    }
    last.insert(topic, fingerprint);
    let id = *seq;
    *seq = seq.wrapping_add(1);
    if *seq == 0 {
        *seq = 1;
    }
    let _ = write_tx
        .send(AdminMsg::Event { seq: id, event })
        .await;
}

fn frame_io<E: std::fmt::Display>(err: E) -> ControlError {
    ControlError::Codec(err.to_string())
}

/// Default feature mask a client sidecar requests.
pub fn client_features() -> u64 {
    CLIENT_FEATURES
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prism::control::types::{FEATURE_AUTH, FEATURE_PANEL, TOPIC_CONNECTIONS};

    struct MockControl {
        features: u64,
        auth: bool,
    }

    #[async_trait]
    impl AdminControl for MockControl {
        fn features(&self) -> u64 {
            self.features
        }
        fn auth_enabled(&self) -> bool {
            self.auth
        }
        fn event_watches(&self) -> AdminEventWatches {
            AdminEventWatches::default()
        }
        async fn authenticate(&self, token: &str) -> Result<AuthIdentity, AdminError> {
            if token == "ok" {
                Ok(AuthIdentity {
                    user_id: "u1".into(),
                    username: "admin".into(),
                    role: crate::prism::auth::UserRole::Admin,
                    service_rules: vec!["*".into()],
                    is_admin: true,
                })
            } else {
                Err(AdminError::unauthorized("bad token"))
            }
        }
        async fn dispatch(
            &self,
            method: AdminMethod,
            _ctx: &AdminCallContext,
        ) -> Result<AdminPayload, AdminError> {
            match method {
                AdminMethod::Health => Ok(AdminPayload::Health { ok: true }),
                AdminMethod::Connections => Ok(AdminPayload::Connections(Vec::new())),
                _ => Err(AdminError::not_found("unhandled")),
            }
        }
    }

    #[tokio::test]
    async fn hello_negotiates_intersection() {
        let (a, b) = tokio::io::duplex(64 * 1024);
        let handler: Arc<dyn AdminControl> = Arc::new(MockControl {
            features: FEATURE_RPC | FEATURE_AUTH,
            auth: false,
        });
        let server = tokio::spawn(async move {
            serve(Box::new(b), handler, None).await.unwrap();
        });
        let ch = connect(
            Box::new(a),
            FEATURE_RPC | FEATURE_AUTH | FEATURE_PANEL | FEATURE_EVENTS,
        )
        .await
        .unwrap();
        assert_eq!(ch.features(), FEATURE_RPC | FEATURE_AUTH);
        let payload = ch.call(AdminMethod::Health).await.unwrap();
        assert_eq!(payload, AdminPayload::Health { ok: true });
        let err = ch.call(AdminMethod::Connections).await.unwrap_err();
        assert_eq!(err.code, crate::prism::control::types::AdminErrorCode::Unavailable);
        ch.close();
        tokio::time::timeout(Duration::from_secs(2), server)
            .await
            .expect("server serve() should exit after client close")
            .unwrap();
    }

    #[tokio::test]
    async fn reject_unknown_protocol_version() {
        use futures_util::SinkExt;
        let (a, b) = tokio::io::duplex(64 * 1024);
        let handler: Arc<dyn AdminControl> = Arc::new(MockControl {
            features: FEATURE_RPC,
            auth: false,
        });
        let server = tokio::spawn(async move {
            let err = serve(Box::new(b), handler, None).await.unwrap_err();
            assert!(matches!(err, ControlError::UnsupportedVersion(9)));
        });

        let mut framed = Framed::new(a, length_codec());
        let env = crate::prism::control::types::Envelope {
            proto: 9,
            msg: vec![0],
        };
        framed
            .send(super::super::codec::encode_envelope(&env).unwrap())
            .await
            .unwrap();
        let reply = framed.next().await.unwrap().unwrap();
        let msg = crate::prism::control::codec::decode_frame(&reply).unwrap();
        match msg {
            AdminMsg::HelloReject {
                reason: HelloRejectReason::UnsupportedVersion { peer: 9, server: 1 },
            } => {}
            other => panic!("unexpected {other:?}"),
        }
        let _ = server.await;
    }

    #[tokio::test]
    async fn session_methods_require_auth_when_enabled() {
        let (a, b) = tokio::io::duplex(64 * 1024);
        let handler: Arc<dyn AdminControl> = Arc::new(MockControl {
            features: FEATURE_RPC | FEATURE_PANEL | FEATURE_AUTH,
            auth: true,
        });
        tokio::spawn(async move {
            let _ = serve(Box::new(b), handler, None).await;
        });
        let ch = connect(
            Box::new(a),
            FEATURE_RPC | FEATURE_PANEL | FEATURE_AUTH,
        )
        .await
        .unwrap();
        let err = ch.call(AdminMethod::Connections).await.unwrap_err();
        assert_eq!(
            err.code,
            crate::prism::control::types::AdminErrorCode::Unauthorized
        );
        let session = ch
            .call(AdminMethod::Authenticate {
                token: "ok".into(),
            })
            .await
            .unwrap();
        match session {
            AdminPayload::AuthSession(s) => assert!(s.is_admin),
            other => panic!("{other:?}"),
        }
        let ok = ch.call(AdminMethod::Connections).await.unwrap();
        assert_eq!(ok, AdminPayload::Connections(Vec::new()));
        ch.close();
    }

    #[tokio::test]
    async fn subscribe_without_events_feature_is_unavailable() {
        let (a, b) = tokio::io::duplex(64 * 1024);
        let handler: Arc<dyn AdminControl> = Arc::new(MockControl {
            features: FEATURE_RPC,
            auth: false,
        });
        tokio::spawn(async move {
            let _ = serve(Box::new(b), handler, None).await;
        });
        let ch = connect(Box::new(a), FEATURE_RPC | FEATURE_EVENTS)
            .await
            .unwrap();
        let err = ch
            .call(AdminMethod::Subscribe {
                topics: TOPIC_CONNECTIONS,
            })
            .await
            .unwrap_err();
        assert_eq!(
            err.code,
            crate::prism::control::types::AdminErrorCode::Unavailable
        );
        ch.close();
    }
}
