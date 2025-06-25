import redis
import time

def test_connection():
    """Testa a conexão com o servidor"""
    try:
        r = redis.Redis(host='localhost', port=6379, decode_responses=True, password="123456", socket_timeout=5)
        result = r.ping()
        print(f"✅ Ping resultado: {result}")
        return r
    except redis.exceptions.ConnectionError as e:
        print(f"❌ Falha ao conectar: {e}")
        return None
    except Exception as e:
        print(f"❌ Erro inesperado: {e}")
        return None

def main():
    print("🔗 Testando conexão com o servidor Altilium...")
    
    # Testa a conexão
    r = test_connection()
    if not r:
        print("❌ Não foi possível conectar ao servidor. Verifique se o servidor está rodando.")
        return

    print("Comandos suportados: SET, GET, HSET, DEL, KEYS, PING, EXIT")
    print("Exemplos:")
    print("  SET chave valor")
    print("  GET chave")
    print("  HSET hash campo valor")
    print("  DEL chave")
    print("  KEYS *")
    print("  PING")
    print()

    while True:
        try:
            user_input = input("> ").strip()
            if not user_input:
                continue

            parts = user_input.split()
            command = parts[0].upper()

            if command == "EXIT":
                print("👋 Saindo...")
                break
            elif command == "PING":
                result = r.ping()
                print(f"PONG: {result}")
            elif command == "SET" and len(parts) >= 3:
                key = parts[1]
                value = ' '.join(parts[2:])
                r.set(key, value)
                print("OK")
            elif command == "GET" and len(parts) == 2:
                result = r.get(parts[1])
                print(result if result is not None else "(nil)")
            elif command == "HSET" and len(parts) == 4:
                result = r.hset(parts[1], parts[2], parts[3])
                print(result)
            elif command == "DEL" and len(parts) >= 2:
                keys_to_delete = parts[1:]
                result = r.delete(*keys_to_delete)
                print(f"(integer) {result}")
            elif command == "KEYS" and len(parts) == 2:
                result = r.keys(parts[1])
                if result:
                    for i, key in enumerate(result, 1):
                        print(f"{i}) {key}")
                else:
                    print("(empty list or set)")
            elif command == "TEST":
                # Comando especial para testar várias operações
                print("🧪 Executando testes...")
                
                # Teste SET/GET
                r.set("test_key", "test_value")
                result = r.get("test_key")
                print(f"SET/GET: {result}")
                
                # Teste HSET
                r.hset("test_hash", "field1", "value1")
                print("HSET: OK")
                
                # Teste KEYS
                keys = r.keys("*")
                print(f"KEYS: {keys}")
                
                # Teste DEL
                deleted = r.delete("test_key", "test_hash")
                print(f"DEL: {deleted} chaves deletadas")
                
                print("✅ Testes concluídos!")
            else:
                print("❌ ERRO: Comando inválido ou argumentos incorretos.")
                print("Use: SET key value | GET key | HSET hash field value | DEL key | KEYS pattern | PING | EXIT")
        
        except redis.exceptions.ConnectionError as e:
            print(f"❌ Erro de conexão: {e}")
            print("Tentando reconectar...")
            time.sleep(1)
            r = test_connection()
            if not r:
                print("❌ Não foi possível reconectar. Saindo...")
                break
        except KeyboardInterrupt:
            print("\n👋 Saindo...")
            break
        except Exception as e:
            print(f"❌ Erro: {e}")

if __name__ == "__main__":
    main()