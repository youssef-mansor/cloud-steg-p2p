import socket
import json
import time
import threading

HOST = "0.0.0.0"
PORT = 5000

# store registered clients: {username: (ip, port)}
clients = {}
lock = threading.Lock()  # prevent race conditions

def handle_message(msg):
    global clients
    if msg["type"] == "register":
        time.sleep(30)  # simulate heavy work
        with lock:
            clients[msg["username"]] = (msg["ip"], msg["port"])
        return {"status": "ok", "message": "registered"}
    elif msg["type"] == "peers":
        with lock:
            return {"status": "ok", "peers": clients}
    else:
        return {"status": "error", "message": "unknown type"}

def handle_client(conn, addr):
    with conn:
        data = conn.recv(1024).decode()
        if not data:
            return
        msg = json.loads(data)
        response = handle_message(msg)
        conn.sendall(json.dumps(response).encode())

with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
    s.bind((HOST, PORT))
    s.listen()
    print(f"Threaded discovery server running on {HOST}:{PORT}")

    while True:
        conn, addr = s.accept()
        threading.Thread(target=handle_client, args=(conn, addr)).start()
