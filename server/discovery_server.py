import socket
import json
import time
import threading

HOST = "0.0.0.0"
PORT = 5000

# Store accounts and online clients 
accounts = {} # {username: password} 
online_clients = {} # {username: (ip, port)} 
lock = threading.Lock() # prevent race conditions

def handle_message(msg, addr):
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
                return {"status": "ok", "message": "deregistered"} 
            else: return {"status": "error", "message": "not online"}
    elif mtype == "peers":
        with lock:
            return {"status": "ok", "peers": online_clients}
    else:
        return {"status": "error", "message": "unknown type"}

def handle_client(conn, addr):
    with conn:
        data = conn.recv(1024).decode()
        if not data:
            return
        msg = json.loads(data)
        response = handle_message(msg, addr)
        conn.sendall(json.dumps(response).encode())

with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
    s.bind((HOST, PORT))
    s.listen()
    print(f"Threaded discovery server running on {HOST}:{PORT}")

    while True:
        conn, addr = s.accept()
        threading.Thread(target=handle_client, args=(conn, addr)).start()
