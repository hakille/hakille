//! Mobile test rig managing loopback HTTP gateway, tokens, and bridge interaction.
//!
//! Replaces no networking: binds real `127.0.0.1:0`, writes real token files,
//! executes full HTTP roundtrips with Bearer authorization and `X-Hakille-Session`.

use super::daemon_mock::ControlledDaemon;
use hakille_ipc::Frame;
use hakille_ipc::transport::{fresh_token_hex, write_token_file};
use hakille_mobile::MobileBridge;
use hakille_mobile_gateway::{BearerToken, Gateway, GatewayConfig, serve};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::oneshot;

pub struct MobileTestRig {
    pub client: reqwest::Client,
    pub base_url: String,
    pub address: SocketAddr,
    pub gateway: Arc<Gateway>,
    gateway_task: tokio::task::JoinHandle<()>,
    stop_tx: Option<oneshot::Sender<()>>,
    pub temp_dir: tempfile::TempDir,
    pub ipc_endpoint: String,
    pub ipc_token: String,
    pub mobile_bearer: String,
}

impl Drop for MobileTestRig {
    fn drop(&mut self) {
        if let Some(stop) = self.stop_tx.take() {
            let _ = stop.send(());
        }
        self.gateway_task.abort();
        #[cfg(unix)]
        let _ = std::fs::remove_file(&self.ipc_endpoint);
    }
}

impl MobileTestRig {
    /// Bootstraps an ephemeral gateway on 127.0.0.1:0 and an IPC listener with default timeouts.
    pub async fn setup() -> (Self, ControlledDaemon) {
        Self::setup_with_timeouts(Duration::from_secs(5), Duration::from_secs(60)).await
    }

    /// Bootstraps an ephemeral gateway with customizable request and idle timeouts.
    pub async fn setup_with_timeouts(
        request_timeout: Duration,
        idle_timeout: Duration,
    ) -> (Self, ControlledDaemon) {
        let temp_dir = tempfile::tempdir().expect("create tempdir for test rig");
        let ipc_token = fresh_token_hex();
        let mobile_bearer = fresh_token_hex();

        let ipc_token_file = temp_dir.path().join("ipc.token");
        write_token_file(&ipc_token_file, &ipc_token).expect("write ipc.token file");

        #[cfg(unix)]
        let endpoint = temp_dir
            .path()
            .join("daemon.sock")
            .to_string_lossy()
            .into_owned();
        #[cfg(windows)]
        let endpoint = format!(r"\\.\pipe\hmg-{}", uuid::Uuid::new_v4());

        let daemon = ControlledDaemon::bind(&endpoint, &ipc_token)
            .await
            .expect("bind mock daemon IPC listener");

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind gateway TCP listener");
        let address = listener.local_addr().expect("retrieve local address");
        let base_url = format!("http://{address}");

        let config = GatewayConfig {
            listen: address,
            ipc_endpoint: endpoint.clone(),
            ipc_token_file,
            request_timeout,
            body_timeout: Duration::from_millis(500),
            session_idle_timeout: idle_timeout,
        };

        let bearer = BearerToken::parse(&mobile_bearer).expect("parse bearer token");
        let gateway = Arc::new(Gateway::new(config, bearer).expect("instantiate gateway"));

        let (stop_tx, stop_rx) = oneshot::channel();
        let running_gateway = gateway.clone();
        let gateway_task = tokio::spawn(async move {
            let _ = serve(listener, running_gateway, async {
                let _ = stop_rx.await;
            })
            .await;
        });

        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("build reqwest client");

        let rig = Self {
            client,
            base_url,
            address,
            gateway,
            gateway_task,
            stop_tx: Some(stop_tx),
            temp_dir,
            ipc_endpoint: endpoint,
            ipc_token,
            mobile_bearer,
        };

        (rig, daemon)
    }

    /// Creates and configures a new `MobileBridge` connected to this rig's gateway URL.
    pub fn new_bridge(&self) -> MobileBridge {
        let bridge = MobileBridge::new();
        let action = serde_json::json!({
            "type": "set_hub",
            "hub_id": self.base_url,
        });
        let res_str = bridge.dispatch_json(action.to_string());
        let res: serde_json::Value =
            serde_json::from_str(&res_str).expect("parse set_hub response JSON");
        assert_eq!(
            res["ok"], true,
            "set_hub must succeed on fresh bridge: {res_str}"
        );
        bridge
    }

    /// Dispatches an action JSON value to the bridge and parses the response.
    pub fn dispatch_action(
        &self,
        bridge: &MobileBridge,
        action: serde_json::Value,
    ) -> serde_json::Value {
        let res_str = bridge.dispatch_json(action.to_string());
        serde_json::from_str(&res_str).expect("parse dispatch_json response")
    }

    /// Prepares an action for HTTP submission, extracting the request Frame and gateway session.
    pub fn prepare_frame(
        &self,
        bridge: &MobileBridge,
        action: serde_json::Value,
    ) -> (Frame, Option<String>) {
        let result = self.dispatch_action(bridge, action);
        assert_eq!(
            result["ok"], true,
            "dispatch_action must return ok:true before send: {result}"
        );
        let session = result["snapshot"]["gateway_session"]
            .as_str()
            .map(str::to_owned);
        let frame: Frame =
            serde_json::from_value(result["request"].clone()).expect("parse request Frame");
        (frame, session)
    }

    /// Executes a full roundtrip: dispatch -> HTTP POST -> bridge.receive_json.
    pub async fn roundtrip(
        &self,
        bridge: &MobileBridge,
        action: serde_json::Value,
    ) -> (u16, serde_json::Value) {
        let (frame, session) = self.prepare_frame(bridge, action);
        let mut req = self
            .client
            .post(format!("{}/v1/request", self.base_url))
            .bearer_auth(&self.mobile_bearer)
            .json(&frame);
        if let Some(sess) = session {
            req = req.header("x-hakille-session", sess);
        }

        let response = req.send().await.expect("send HTTP request to gateway");
        let status = response.status().as_u16();
        let body = response.text().await.expect("read gateway response text");

        let received_str = bridge.receive_json(frame.id, body);
        let received: serde_json::Value =
            serde_json::from_str(&received_str).expect("parse receive_json output");
        (status, received)
    }

    /// Posts a raw Frame to `/v1/request` without updating the bridge.
    pub async fn post_frame_raw(
        &self,
        frame: &Frame,
        session: Option<&str>,
    ) -> Result<reqwest::Response, reqwest::Error> {
        let mut req = self
            .client
            .post(format!("{}/v1/request", self.base_url))
            .bearer_auth(&self.mobile_bearer)
            .json(frame);
        if let Some(sess) = session {
            req = req.header("x-hakille-session", sess);
        }
        req.send().await
    }

    pub async fn post_frame(
        &self,
        frame: &Frame,
        session: Option<&str>,
    ) -> (u16, serde_json::Value) {
        let resp = self
            .post_frame_raw(frame, session)
            .await
            .expect("send frame");
        let status = resp.status().as_u16();
        let body: serde_json::Value = resp.json().await.expect("parse response json");
        (status, body)
    }

    /// Returns a request builder for GET `/health`.
    pub fn health(&self) -> reqwest::RequestBuilder {
        self.client.get(format!("{}/health", self.base_url))
    }

    /// Retrieves and parses the current snapshot of a bridge.
    pub fn snapshot(&self, bridge: &MobileBridge) -> serde_json::Value {
        serde_json::from_str(&bridge.snapshot_json()).expect("parse snapshot_json output")
    }
}
