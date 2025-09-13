import socket
import json

SERVER_HOST = "server1"   # discovery server container name
SERVER_PORT = 5000

# Ask user for unique identity
USERNAME = input("Enter your username: ").strip()
MY_IP = input("Enter your container name (e.g., client1): ").strip()
MY_PORT = int(input("Enter your port (e.g., 6000): ").strip())

def send_message(message):
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.connect((SERVER_HOST, SERVER_PORT))
        s.sendall(json.dumps(message).encode())
        data = s.recv(1024).decode()
        return json.loads(data)

# Step 1: Register
register_msg = {"type": "register", "username": USERNAME, "ip": MY_IP, "port": MY_PORT}
print("Register:", send_message(register_msg))

# Step 2: Ask for peers
peers_msg = {"type": "peers"}
print("Peers:", send_message(peers_msg))
