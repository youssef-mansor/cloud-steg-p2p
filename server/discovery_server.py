# discovery_server.py
import socket
import json

HOST = "0.0.0.0"   # listen on all interfaces
PORT = 5000

# store registered clients: {username: (ip, port)}
clients = {}

def handle_message(msg, addr):
    global clients
    if msg["type"] == "register":
        clients[msg["username"]] = (msg["ip"], msg["port"])
        return {"status": "ok", "message": "registered"}
    elif msg["type"] == "peers":
        return {"status": "ok", "peers": clients}
    else:
        return {"status": "error", "message": "unknown type"}

with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
    s.bind((HOST, PORT))
    s.listen()
    print(f"Discovery server running on {HOST}:{PORT}")

    while True:
        conn, addr = s.accept()
        with conn:
            data = conn.recv(1024).decode()
            if not data:
                continue
            msg = json.loads(data)
            response = handle_message(msg, addr)
            conn.sendall(json.dumps(response).encode())
