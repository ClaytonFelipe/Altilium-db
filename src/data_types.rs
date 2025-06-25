use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{Duration, SystemTime};
use serde::{Deserialize, Serialize};

/// Enum que representa os diferentes tipos de valores que podem ser armazenados.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Value {
    String(String),
    List(VecDeque<String>),
    Set(HashSet<String>),
    Hash(HashMap<String, String>),
}

/// Enum que representa os comandos que modificam o estado. Usado no canal de comunicação.
#[derive(Debug, Clone)]
pub enum Command {
    Set {
        key: String,
        value: Value,
        expiry: Option<Duration>,
    },
    HSet {
        key: String,
        field: String,
        value: String,
    },
    Delete {
        key: String,
    },
   
}

/// Metadados associados a uma chave, como o tempo de expiração.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyMetadata {
    pub expiry: Option<SystemTime>,
}