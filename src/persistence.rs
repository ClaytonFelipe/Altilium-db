use crate::data_types::{Command, KeyMetadata, Value};
use crate::resp::{serialize_resp, RespValue};
use crate::store::Store;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::fs;
use tokio::time::interval;

#[derive(Serialize, Deserialize)]
struct Snapshot {
    data: HashMap<String, Value>,
    metadata: HashMap<String, KeyMetadata>,
}

// Adicionamos Clone
#[derive(Clone)]
pub struct PersistenceManager {
    store: Arc<Store>,
    snapshot_path: PathBuf,
    aof_path: PathBuf,
    snapshot_interval: Duration,
}

impl PersistenceManager {
    pub fn new(
        store: Arc<Store>,
        snapshot_path: PathBuf,
        aof_path: PathBuf,
        snapshot_interval_secs: u64,
    ) -> Self {
        Self {
            store,
            snapshot_path,
            aof_path,
            snapshot_interval: Duration::from_secs(snapshot_interval_secs),
        }
    }
    
    // As tasks agora são iniciadas em `main` para facilitar o gerenciamento do Arc.
    // Esta função não é mais necessária. Você pode removê-la ou deixá-la comentada.
    // pub async fn run_tasks(self: Arc<Self>) { ... }

    pub async fn load_from_disk(&self) -> io::Result<()> {
        if self.snapshot_path.exists() {
            self.load_snapshot().await?;
            println!("[Persistence] Snapshot carregado de {}", self.snapshot_path.display());
        }
        Ok(())
    }

    pub async fn run_snapshot_task(self: Arc<Self>) {
        let mut interval = interval(self.snapshot_interval);
        loop {
            interval.tick().await;
            if let Err(e) = self.create_snapshot().await {
                eprintln!("[Persistence] Erro ao criar snapshot: {}", e);
            }
        }
    }

    pub async fn run_aof_persistence(self: Arc<Self>) {
        let mut file = match OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.aof_path)
        {
            Ok(f) => f,
            Err(e) => {
                eprintln!("[Persistence] Falha ao abrir arquivo AOF: {}", e);
                return;
            }
        };

        let mut cmd_rx = self.store.cmd_tx.subscribe();
        while let Ok(cmd) = cmd_rx.recv().await {
            let resp_cmd = self.command_to_resp(cmd);
            let bytes = serialize_resp(resp_cmd);
            if let Err(e) = file.write_all(&bytes) {
                eprintln!("[Persistence] Erro ao escrever no arquivo AOF: {}", e);
            }
        }
    }

    async fn create_snapshot(&self) -> io::Result<()> {
        let temp_path = self.snapshot_path.with_extension("tmp");

        let snapshot = Snapshot {
            data: self.store.data.read().await.clone(),
            metadata: self.store.metadata.read().await.clone(),
        };

        let file = File::create(&temp_path)?;
        serde_json::to_writer(BufWriter::new(file), &snapshot)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        fs::rename(temp_path, &self.snapshot_path).await?;
        println!("[Persistence] Snapshot salvo em {}", self.snapshot_path.display());
        Ok(())
    }

    async fn load_snapshot(&self) -> io::Result<()> {
        let content = fs::read(&self.snapshot_path).await?;
        let snapshot: Snapshot = serde_json::from_slice(&content)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        *self.store.data.write().await = snapshot.data;
        *self.store.metadata.write().await = snapshot.metadata;
        Ok(())
    }

    fn command_to_resp(&self, cmd: Command) -> RespValue {
        match cmd {
            Command::Set { key, value, expiry } => {
                let mut args = vec![
                    RespValue::BulkString(b"SET".to_vec()),
                    RespValue::BulkString(key.into_bytes()),
                ];
                if let Value::String(s) = value {
                    args.push(RespValue::BulkString(s.into_bytes()));
                }
                if let Some(d) = expiry {
                    args.push(RespValue::BulkString(b"PX".to_vec()));
                    args.push(RespValue::BulkString(
                        d.as_millis().to_string().into_bytes(),
                    ));
                }
                RespValue::Array(args)
            }
            Command::HSet { key, field, value } => RespValue::Array(vec![
                RespValue::BulkString(b"HSET".to_vec()),
                RespValue::BulkString(key.into_bytes()),
                RespValue::BulkString(field.into_bytes()),
                RespValue::BulkString(value.into_bytes()),
            ]),
            Command::Delete { key } => RespValue::Array(vec![
    RespValue::BulkString(b"DEL".to_vec()),
    RespValue::BulkString(key.into_bytes()),
]),
        }
    }
}