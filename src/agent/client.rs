use super::{Event, Permission, ToolRequest};
use agent_client_protocol as acp;
use std::{cell::RefCell, rc::Rc, sync::mpsc::SyncSender};
use tokio::sync::oneshot;

pub(super) struct Client {
    pub(super) events: SyncSender<Event>,
    pub(super) session: Rc<RefCell<Option<acp::SessionId>>>,
    pub(super) fault: tokio::sync::mpsc::Sender<()>,
}

impl Client {
    fn send(&self, event: Event) -> acp::Result<()> {
        self.events.try_send(event).map_err(|_| {
            let _ = self.fault.try_send(());
            acp::Error::internal_error()
        })
    }
}

#[async_trait::async_trait(?Send)]
impl acp::Client for Client {
    async fn read_text_file(
        &self,
        request: acp::ReadTextFileRequest,
    ) -> acp::Result<acp::ReadTextFileResponse> {
        if self.session.borrow().as_ref() != Some(&request.session_id) {
            return Err(acp::Error::invalid_params());
        }
        let (reply, result) = std::sync::mpsc::sync_channel(1);
        self.send(Event::Tool(ToolRequest {
            name: "_acp_read_file".into(),
            arguments: serde_json::json!({"path":request.path,"line":request.line,"line_limit":request.limit}),
            reply, deadline: std::time::Instant::now() + std::time::Duration::from_secs(15),
        }))?;
        let result = tokio::task::spawn_blocking(move || {
            result.recv_timeout(std::time::Duration::from_secs(15))
        })
        .await
        .map_err(|_| acp::Error::internal_error())?
        .map_err(|_| acp::Error::internal_error())?;
        if result["status"] != "ok" || result["data"]["truncated"] == true {
            return Err(acp::Error::invalid_params());
        }
        Ok(acp::ReadTextFileResponse::new(
            result["data"]["text"].as_str().unwrap_or_default(),
        ))
    }
    async fn request_permission(
        &self,
        request: acp::RequestPermissionRequest,
    ) -> acp::Result<acp::RequestPermissionResponse> {
        let cancelled =
            || acp::RequestPermissionResponse::new(acp::RequestPermissionOutcome::Cancelled);
        if self.session.borrow().as_ref() != Some(&request.session_id) {
            return Ok(cancelled());
        }
        let (reply, receiver) = oneshot::channel();
        self.send(Event::Permission(Box::new(Permission { request, reply })))?;
        Ok(
            tokio::time::timeout(std::time::Duration::from_secs(120), receiver)
                .await
                .ok()
                .and_then(Result::ok)
                .unwrap_or_else(cancelled),
        )
    }

    async fn session_notification(
        &self,
        notification: acp::SessionNotification,
    ) -> acp::Result<()> {
        if self.session.borrow().as_ref() == Some(&notification.session_id) {
            let update = serde_json::to_value(notification.update)
                .map_err(|_| acp::Error::internal_error())?;
            self.send(Event::Update(update))?;
        }
        Ok(())
    }
}
