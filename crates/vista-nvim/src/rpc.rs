//! A minimal msgpack-RPC client over a child process's stdin/stdout.

use std::collections::HashMap;
use std::io::{BufReader, BufWriter, Write};
use std::process::{ChildStdin, ChildStdout};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rmpv::Value;

#[derive(Debug, thiserror::Error)]
pub enum RpcError {
    #[error("Neovim is not running")]
    Closed,
    #[error("Neovim did not answer `{0}` in time")]
    Timeout(String),
    #[error("Neovim error in `{method}`: {message}")]
    Remote { method: String, message: String },
    #[error("cannot talk to Neovim: {0}")]
    Io(String),
}

/// A notification (`[2, method, params]`) from Neovim.
#[derive(Debug, Clone, PartialEq)]
pub struct Notification {
    pub method: String,
    pub params: Vec<Value>,
}

type Pending = Arc<Mutex<HashMap<u64, Sender<Result<Value, Value>>>>>;

pub struct Rpc {
    writer: Mutex<BufWriter<ChildStdin>>,
    next_id: AtomicU64,
    pending: Pending,
}

impl Rpc {
    /// Starts the reader thread. Notifications go to `on_notify`; when the
    /// stream ends (Neovim exited), `on_close` runs once.
    pub fn start(
        stdin: ChildStdin,
        stdout: ChildStdout,
        on_notify: impl Fn(Notification) + Send + 'static,
        on_close: impl FnOnce() + Send + 'static,
    ) -> Rpc {
        let pending: Pending = Arc::default();
        let reader_pending = pending.clone();
        std::thread::Builder::new()
            .name("nvim-rpc".into())
            .spawn(move || {
                let mut rd = BufReader::new(stdout);
                while let Ok(msg) = rmpv::decode::read_value(&mut rd) {
                    let Some(parts) = msg.as_array() else { continue };
                    match parts.first().and_then(Value::as_u64) {
                        // [1, msgid, error, result]
                        Some(1) if parts.len() == 4 => {
                            let id = parts[1].as_u64().unwrap_or(u64::MAX);
                            let res = if parts[2].is_nil() {
                                Ok(parts[3].clone())
                            } else {
                                Err(parts[2].clone())
                            };
                            if let Some(tx) = reader_pending.lock().expect("pending").remove(&id) {
                                let _ = tx.send(res);
                            }
                        }
                        // [2, method, params]
                        Some(2) if parts.len() == 3 => {
                            let method = parts[1].as_str().unwrap_or_default().to_string();
                            let params = parts[2].as_array().cloned().unwrap_or_default();
                            on_notify(Notification { method, params });
                        }
                        _ => {}
                    }
                }
                reader_pending.lock().expect("pending").clear();
                on_close();
            })
            .expect("spawn rpc reader");
        Rpc {
            writer: Mutex::new(BufWriter::new(stdin)),
            next_id: AtomicU64::new(1),
            pending,
        }
    }

    fn write(&self, v: &Value) -> Result<(), RpcError> {
        let mut w = self.writer.lock().expect("writer");
        rmpv::encode::write_value(&mut *w, v).map_err(|e| RpcError::Io(e.to_string()))?;
        w.flush().map_err(|_| RpcError::Closed)
    }

    /// Sends a request and waits for its response.
    pub fn request(&self, method: &str, params: Vec<Value>, timeout: Duration) -> Result<Value, RpcError> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = mpsc::channel();
        self.pending.lock().expect("pending").insert(id, tx);
        let msg = Value::Array(vec![
            Value::from(0),
            Value::from(id),
            Value::from(method),
            Value::Array(params),
        ]);
        if let Err(e) = self.write(&msg) {
            self.pending.lock().expect("pending").remove(&id);
            return Err(e);
        }
        match rx.recv_timeout(timeout) {
            Ok(Ok(v)) => Ok(v),
            Ok(Err(err)) => Err(RpcError::Remote {
                method: method.to_string(),
                message: error_message(&err),
            }),
            Err(mpsc::RecvTimeoutError::Timeout) => {
                self.pending.lock().expect("pending").remove(&id);
                Err(RpcError::Timeout(method.to_string()))
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => Err(RpcError::Closed),
        }
    }

    /// Sends a request without waiting (the response is discarded).
    pub fn request_async(&self, method: &str, params: Vec<Value>) -> Result<(), RpcError> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let msg = Value::Array(vec![
            Value::from(0),
            Value::from(id),
            Value::from(method),
            Value::Array(params),
        ]);
        self.write(&msg)
    }
}

/// Neovim errors are `[type, message]`.
fn error_message(err: &Value) -> String {
    err.as_array()
        .and_then(|a| a.get(1))
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| err.to_string())
}
