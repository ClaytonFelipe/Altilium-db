use crate::data_types::{Command, KeyMetadata, Value};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::{broadcast, RwLock};

#[derive(Clone)]
pub struct Store {
    pub data: Arc<RwLock<HashMap<String, Value>>>,
    pub metadata: Arc<RwLock<HashMap<String, KeyMetadata>>>,
    pub cmd_tx: broadcast::Sender<Command>,
}

impl Store {
    /// Cria uma nova instância da Store e a task de background para processar comandos.
    pub fn new() -> (Self, impl std::future::Future<Output = ()>) {
        let (cmd_tx, _) = broadcast::channel(128);

        let store = Self {
            data: Arc::new(RwLock::new(HashMap::new())),
            metadata: Arc::new(RwLock::new(HashMap::new())),
            cmd_tx: cmd_tx.clone(),
        };

        let cmd_rx = cmd_tx.subscribe();
        let background_task = store.clone().process_commands(cmd_rx);

        (store, background_task)
    }

    /// Task que roda em background, ouvindo por comandos de escrita e aplicando-os.
    /// Centraliza as escritas, evitando a necessidade de locks complexos nos handlers.
    async fn process_commands(self, mut cmd_rx: broadcast::Receiver<Command>) {
        while let Ok(cmd) = cmd_rx.recv().await {
            match cmd {
                Command::Set { key, value, expiry } => {
                    let mut data_lock = self.data.write().await;
                    data_lock.insert(key.clone(), value);
                    drop(data_lock); // Libera o lock o mais rápido possível

                    let mut meta_lock = self.metadata.write().await;
                    if let Some(duration) = expiry {
                        meta_lock.insert(
                            key,
                            KeyMetadata {
                                expiry: Some(SystemTime::now() + duration),
                            },
                        );
                    } else {
                        meta_lock.remove(&key);
                    }
                }
                Command::HSet { key, field, value } => {
                    let mut data_lock = self.data.write().await;
                    let entry = data_lock
                        .entry(key.clone())
                        .or_insert_with(|| Value::Hash(HashMap::new()));

                    if let Value::Hash(hash) = entry {
                        hash.insert(field, value);
                    }
                    // Se o tipo não for Hash, a operação falha silenciosamente aqui,
                    // mas o `hset` no `process_command` já deve ter verificado o tipo.
                }
                Command::Delete { key } => {
                    let mut data_lock = self.data.write().await;
                    data_lock.remove(&key);
                    drop(data_lock);

                    let mut meta_lock = self.metadata.write().await;
                    meta_lock.remove(&key);
                }
            }
        }
    }

    /// Envia um comando `GET`. Operação de leitura, acessa diretamente o `RwLock`.
    pub async fn get(&self, key: &str) -> Option<Value> {
        self.data.read().await.get(key).cloned()
    }

    /// Envia um comando `SET` para a task de processamento.
    pub async fn set(&self, key: String, value: Value, expiry: Option<Duration>) {
        let cmd = Command::Set { key, value, expiry };
        // O erro é ignorado pois só ocorre se não houver receptores.
        let _ = self.cmd_tx.send(cmd);
    }

    /// Envia um comando `HSET`. Verifica o tipo antes de enviar o comando.
    pub async fn hset(&self, key: String, field: String, value: String) -> Result<i64, &'static str> {
        let data_lock = self.data.read().await;
        let mut created = true; 

        if let Some(v) = data_lock.get(&key) {
            match v {
                Value::Hash(hash) => {
                    if hash.contains_key(&field) {
                        created = false;
                    }
                }
                _ => return Err("WRONGTYPE Operation against a key holding the wrong kind of value"),
            }
        }
        drop(data_lock);

        let cmd = Command::HSet { key, field, value };
        let _ = self.cmd_tx.send(cmd);

        Ok(if created { 1 } else { 0 })
    }

    /// Deleta uma chave do store
    pub async fn delete(&self, key: &str) -> bool {
        let data_lock = self.data.read().await;
        let exists = data_lock.contains_key(key);
        drop(data_lock);

        if exists {
            let cmd = Command::Delete { key: key.to_string() };
            let _ = self.cmd_tx.send(cmd);
            true
        } else {
            false
        }
    }

    /// Varre e remove todas as chaves expiradas.
    pub async fn clean_expired(&self) {
        let now = SystemTime::now();
        let mut expired_keys = Vec::new();

        let meta_lock = self.metadata.read().await;
        for (key, meta) in meta_lock.iter() {
            if let Some(expiry_time) = meta.expiry {
                if now >= expiry_time {
                    expired_keys.push(key.clone());
                }
            }
        }
        drop(meta_lock);

        if !expired_keys.is_empty() {
            let mut data_lock = self.data.write().await;
            let mut meta_lock = self.metadata.write().await;
            for key in expired_keys {
                data_lock.remove(&key);
                meta_lock.remove(&key);
            }
        }
    }
}