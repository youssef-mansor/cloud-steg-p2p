import socket
import json
import time
import threading

HOST = "0.0.0.0"
PORT = 5000

# List of peer servers (host, port) 
PEERS = [("server2", 5000), ("server3", 5000)] # adjust as needed

# Store accounts and online clients 
accounts = {} # {username: password} 
online_clients = {} # {username: (ip, port)} 
lock = threading.Lock() # prevent race conditions

def broadcast_update(update): 
    """Send update to all peers (fire-and-forget).""" 
    envelope = {"type": "sync_update", "update": update}
    for host, port in PEERS: 
        try: 
            with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s: 
                s.connect((host, port)) 
                s.sendall(json.dumps(envelope).encode()) 
        except Exception: 
            pass # ignore peer errors for now

def handle_message(msg, addr, from_peer=False):
    global accounts, online_clients
    mtype = msg.get("type")

    if mtype == "signup":
        username = msg.get("username")
        password = msg.get("password")
        if not username or not password: 
            return {"status": "error", "message": "missing username/password"}
        with lock: 
            if username in accounts: 
                return {"status": "error", "message": "username already exists"} 
            accounts[username] = password
        if not from_peer: 
            broadcast_update(msg)
        return {"status": "ok", "message": "account created"}
    
    elif mtype == "register":
        username = msg.get("username") 
        password = msg.get("password")
        port = msg.get("port")
        ip = addr[0]
        time.sleep(5)  # simulate heavy work
        if not username or not password or not port: 
            return {"status": "error", "message": "missing fields"} 
        with lock: 
            if username not in accounts: 
                return {"status": "error", "message": "no such account"} 
            if accounts[username] != password: 
                return {"status": "error", "message": "wrong password"} 
            online_clients[username] = (ip, port) 
        if not from_peer: 
            broadcast_update(msg)        
        return {"status": "ok", "message": "registered"}
    
    elif mtype == "deregister": 
        username = msg.get("username") 
        password = msg.get("password")
        if not username: 
            return {"status": "error", "message": "missing username"} 
        if not password:
            return {"status": "error", "message": "missing password"}
        if username not in accounts: 
                return {"status": "error", "message": "no such account"} 
        with lock: 
            if accounts[username] != password: 
                return {"status": "error", "message": "wrong password"}
            if username in online_clients: 
                del online_clients[username] 
                if not from_peer: 
                    broadcast_update(msg)
                return {"status": "ok", "message": "deregistered"} 
            else: return {"status": "error", "message": "not online"}

    elif mtype == "peers":
        with lock:
            return {"status": "ok", "peers": online_clients}
    elif mtype == "sync_update":
        inner_msg = msg.get("update")
        if inner_msg:
            handle_message(inner_msg, addr, from_peer=True)
        return {"status": "ok", "message": "synced"}
    else:
        return {"status": "error", "message": "unknown type"}

def handle_client(conn, addr): 
    with conn: 
        try: 
            data = conn.recv(1024).decode() 
            if not data: 
                return 
            msg = json.loads(data) 
            # Wrap peer broadcast updates 
            if msg.get("type") in ("signup", "register", "deregister"): 
                response = handle_message(msg, addr, from_peer=False) 
            elif msg.get("type") == "sync_update": 
                response = handle_message(msg, addr, from_peer=True) 
            else: 
                response = handle_message(msg, addr) 
        except Exception as e: 
            response = {"status": "error", "message": str(e)} 
        
        conn.sendall(json.dumps(response).encode())

with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
    s.bind((HOST, PORT))
    s.listen()
    print(f"Threaded discovery server running on {HOST}:{PORT}")

    while True:
        conn, addr = s.accept()
        threading.Thread(target=handle_client, args=(conn, addr)).start()
