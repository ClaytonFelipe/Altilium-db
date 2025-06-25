mod data_types;
mod persistence;
mod resp;
mod store;

use crate::data_types::Value;
use crate::persistence::PersistenceManager;
use crate::resp::{parse_resp, serialize_resp, RespValue};
use crate::store::Store;

use bytes::{Buf, BytesMut};
use config::Config;
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

#[derive(Deserialize)]
struct Settings {
    host: String,
    port: u16,
    requirepass: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let settings = Config::builder()
        .add_source(config::File::with_name("Config"))
        .build()?
        .try_deserialize::<Settings>()?;

    let bind_address = format!("{}:{}", settings.host, settings.port);
    let password = settings.requirepass.clone();

    // 1. Inicializa o Store
    let (store, store_bg_task) = Store::new();
    let store = Arc::new(store);
    tokio::spawn(store_bg_task);

    // 2. Inicializa o gerenciador de persistência
    let persistence = Arc::new(PersistenceManager::new(
        store.clone(),
        PathBuf::from("data.snapshot.json"),
        PathBuf::from("data.aof"),
        60, // Intervalo de snapshot em segundos
    ));

    // 3. Carrega dados do disco
    if let Err(e) = persistence.load_from_disk().await {
        eprintln!("Não foi possível carregar dados do disco: {}", e);
    }

    // 4. Inicia as tasks de persistência em background
    let p_clone_snap = persistence.clone();
    tokio::spawn(async move {
        p_clone_snap.run_snapshot_task().await;
    });
    let p_clone_aof = persistence.clone();
    tokio::spawn(async move {
        p_clone_aof.run_aof_persistence().await;
    });

    // 5. Inicia a task de limpeza
    let store_clone_for_cleaning = store.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        loop {
            interval.tick().await;
            store_clone_for_cleaning.clean_expired().await;
        }
    });

    // 6. Inicia o servidor TCP
    let listener = TcpListener::bind(&bind_address).await?;
    println!("🚀 Servidor Altilium rodando em {}", bind_address);

    loop {
        let (socket, addr) = listener.accept().await?;
        println!("Nova conexão de: {}", addr);
        let store_clone = store.clone();
        let password_clone = password.clone();

        tokio::spawn(async move {
            if let Err(e) = handle_connection(socket, store_clone, password_clone).await {
                if !e.to_string().contains("reset by peer") && !e.to_string().contains("fechada com buffer incompleto") {
                    eprintln!("Erro na conexão {}: {}", addr, e);
                }
            }
            println!("Conexão {} encerrada", addr);
        });
    }
}

async fn handle_connection(
    socket: TcpStream,
    store: Arc<Store>,
    password: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut reader = BufReader::new(socket);
    let mut buffer = BytesMut::with_capacity(4096);
    let mut authenticated = password.is_none();

      loop {
        
        // A leitura bloqueará até que o cliente envie algo.
        let bytes_read = reader.read_buf(&mut buffer).await?;
        
        // Se o cliente fechou a conexão, `bytes_read` será 0.
        if bytes_read == 0 {
            // Se o buffer estiver vazio, foi uma desconexão limpa.
            if buffer.is_empty() {
                println!("Conexão encerrada de forma limpa.");
                return Ok(());
            } else {
                // Se não, o cliente desconectou no meio de um comando.
                return Err("Conexão fechada com buffer incompleto".into());
            }
        }


        // Processa todos os comandos completos no buffer
        loop {
            match parse_resp(&buffer) {
                Ok((remaining, frame)) => {
                    let consumed = buffer.len() - remaining.len();
                    //buffer.advance(consumed);

                    let response = process_command(frame, &store, &mut authenticated, &password).await;
                    let response_bytes = serialize_resp(response);
                    
                    // Escreve a resposta de volta
                    if let Err(e) = reader.get_mut().write_all(&response_bytes).await {
                        eprintln!("Erro ao escrever resposta: {}", e);
                        return Err(e.into());
                    }

                    buffer.advance(consumed);

                    if buffer.is_empty() {
                        break;
                    }

                    
                }
                Err(nom::Err::Incomplete(_)) => {
                    // Precisa de mais dados
                    break;
                }
                Err(e) => {
                    eprintln!("Erro de parsing: {:?}", e);
                    // let response = RespValue::Error(format!("ERR parse error: {:?}", e));
                    // let response_bytes = serialize_resp(response);
                    // let _ = reader.get_mut().write_all(&response_bytes).await;
                    return Err(format!("Erro de parsing: {:?}", e).into());
                }
            }

        }
    }
    //Ok(())
}

async fn process_command(
    cmd: RespValue,
    store: &Store,
    authenticated: &mut bool,
    password: &Option<String>,
) -> RespValue {
    // 1. Garante que o comando é um Array e extrai os argumentos
    let mut args = match cmd {
        RespValue::Array(args) => args,
        _ => return RespValue::Error("ERR command must be an array".into()),
    };

    if args.is_empty() {
        return RespValue::Error("ERR empty command".into());
    }

    // 2. Extrai o nome do comando
    let command_name = match args.remove(0).to_string() {
        Ok(name) => name.to_uppercase(),
        Err(_) => return RespValue::Error("ERR invalid command name".into()),
    };

    println!("Processando comando: {} com {} args", command_name, args.len());

    // 3. Verifica a autenticação
    if !*authenticated && command_name != "AUTH" {
        return RespValue::Error("NOAUTH Authentication required.".into());
    }

    // 4. Executa o comando
    match command_name.as_str() {
        "AUTH" => {
            if let Some(pass) = password {
                if args.len() == 1 && args[0].clone().to_string().unwrap_or_default() == *pass {
                    *authenticated = true;
                    RespValue::SimpleString("OK".into())
                } else {
                    RespValue::Error("ERR invalid password".into())
                }
            } else {
                RespValue::Error("ERR AUTH is not needed".into())
            }
        }

        "PING" => {
            if args.is_empty() {
                RespValue::SimpleString("PONG".into())
            } else {
                match args[0].clone().to_string() {
                    Ok(msg) => RespValue::BulkString(msg.into_bytes()),
                    Err(_) => RespValue::SimpleString("PONG".into()),
                }
            }
        }

        "GET" => {
            if args.len() != 1 {
                return RespValue::Error("ERR wrong number of arguments for 'GET'".into());
            }
            let Ok(key) = args.remove(0).to_string() else {
                return RespValue::Error("ERR invalid key".into());
            };
            match store.get(&key).await {
                Some(Value::String(s)) => RespValue::BulkString(s.into_bytes()),
                Some(_) => RespValue::Error(
                    "WRONGTYPE Operation against a key holding the wrong kind of value".into(),
                ),
                None => RespValue::Null,
            }
        }

        "SET" => {
            if args.len() < 2 {
                return RespValue::Error("ERR wrong number of arguments for 'SET'".into());
            }
            let Ok(key) = args.remove(0).to_string() else {
                return RespValue::Error("ERR invalid key".into());
            };
            let Ok(value) = args.remove(0).to_string() else {
                return RespValue::Error("ERR invalid value".into());
            };

            let mut expiry = None;
            if !args.is_empty() {
                if let Ok(opt) = args.remove(0).to_string() {
                    if opt.to_uppercase() == "PX" && !args.is_empty() {
                        if let Ok(millis_str) = args.remove(0).to_string() {
                            if let Ok(millis) = millis_str.parse::<u64>() {
                                expiry = Some(Duration::from_millis(millis));
                            }
                        }
                    } else if opt.to_uppercase() == "EX" && !args.is_empty() {
                        if let Ok(secs_str) = args.remove(0).to_string() {
                            if let Ok(secs) = secs_str.parse::<u64>() {
                                expiry = Some(Duration::from_secs(secs));
                            }
                        }
                    }
                }
            }
            store.set(key, Value::String(value), expiry).await;
            RespValue::SimpleString("OK".into())
        }

        "KEYS" => {
            if args.len() != 1 || args[0].clone().to_string().unwrap_or_default() != "*" {
                return RespValue::Error("ERR a sintaxe suportada é 'KEYS *'".into());
            }
            let data_lock = store.data.read().await;
            let keys: Vec<RespValue> = data_lock
                .keys()
                .map(|k| RespValue::BulkString(k.clone().into_bytes()))
                .collect();
            RespValue::Array(keys)
        }

        "HSET" => {
            if args.len() != 3 {
                return RespValue::Error("ERR wrong number of arguments for 'HSET'".into());
            }
            let Ok(key) = args.remove(0).to_string() else { 
                return RespValue::Error("ERR invalid key".into()); 
            };
            let Ok(field) = args.remove(0).to_string() else { 
                return RespValue::Error("ERR invalid field".into()); 
            };
            let Ok(value) = args.remove(0).to_string() else { 
                return RespValue::Error("ERR invalid value".into()); 
            };

            match store.hset(key, field, value).await {
                Ok(i) => RespValue::Integer(i),
                Err(e) => RespValue::Error(e.to_string()),
            }
        }

        "DEL" => {
            if args.is_empty() {
                return RespValue::Error("ERR wrong number of arguments for 'DEL'".into());
            }
            let mut deleted_count = 0;
            for arg in args {
                if let Ok(key) = arg.to_string() {
                    if store.delete(&key).await {
                        deleted_count += 1;
                    }
                }
            }
            RespValue::Integer(deleted_count)
        }

        _ => RespValue::Error(format!("ERR unknown command '{}'", command_name)),
    }
}