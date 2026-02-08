use std::sync::Arc;

use tokio::io::AsyncWriteExt;

use crate::prism::tunnel::{
    manager::Manager,
    protocol,
    transport::{transport_by_name, TransportListenOptions},
};

#[derive(Debug, Clone)]
pub struct QuicServerOptions {
    pub cert_file: String,
    pub key_file: String,
}

#[derive(Debug, Clone)]
pub struct ServerOptions {
    pub listen_addr: String,
    pub transport: String,
    pub auth_token: String,
    pub quic: QuicServerOptions,
    pub manager: Arc<Manager>,
}

pub struct Server {
    opts: ServerOptions,
}

impl Server {
    pub fn new(opts: ServerOptions) -> anyhow::Result<Self> {
        Ok(Self { opts })
    }

    pub fn manager(&self) -> Arc<Manager> {
        self.opts.manager.clone()
    }

    pub async fn listen_and_serve(&self, ctx: tokio::sync::watch::Receiver<bool>) -> anyhow::Result<()> {
        let tr = transport_by_name(&self.opts.transport)?;

        let ln = tr
            .listen(
                &self.opts.listen_addr,
                TransportListenOptions {
                    quic: crate::prism::tunnel::transport::QuicListenOptions {
                        cert_file: self.opts.quic.cert_file.clone(),
                        key_file: self.opts.quic.key_file.clone(),
                        next_protos: vec![],
                    },
                },
            )
            .await?;

        tracing::info!(
            addr = %self.opts.listen_addr,
            transport = %tr.name(),
            "tunnel: listening"
        );

        let mut shutdown = ctx.clone();
        loop {
            tokio::select! {
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        break;
                    }
                }
                sess = ln.accept() => {
                    let sess = sess?;
                    let mgr = self.opts.manager.clone();
                    let token = self.opts.auth_token.clone();
                    tokio::spawn(async move {
                        if let Err(err) = handle_session(mgr, sess, token).await {
                            tracing::warn!(err=%err, "tunnel: session ended with error");
                        }
                    });
                }
            }
        }

        ln.close().await?;
        Ok(())
    }
}

async fn handle_session(mgr: Arc<Manager>, sess: Arc<dyn crate::prism::tunnel::transport::TransportSession>, auth_token: String) -> anyhow::Result<()> {
    let cid = mgr.next_client_id("c");
    let remote = sess.remote_addr().map(|a| a.to_string()).unwrap_or_default();

    // First stream must be register.
    let mut reg = sess.accept_stream().await?;
    let req = protocol::read_register_request(&mut reg).await?;

    if !auth_token.trim().is_empty() && req.token != auth_token {
        tracing::warn!(client=%remote, "tunnel: bad token");
        sess.close().await;
        return Ok(());
    }

    mgr.register_client(cid.clone(), sess.clone(), req.services).await?;
    tracing::info!(cid=%cid, client=%remote, "tunnel: client connected");

    // Hold an accept loop to detect disconnects and close unexpected streams.
    loop {
        match sess.accept_stream().await {
            Ok(mut st) => {
                // Unexpected stream opened by client; close quietly.
                let _ = tokio::time::timeout(std::time::Duration::from_secs(1), st.shutdown()).await;
            }
            Err(_) => break,
        }
    }

    mgr.unregister_client(&cid).await;
    tracing::info!(cid=%cid, client=%remote, "tunnel: client disconnected");
    Ok(())
}
