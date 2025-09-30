import socket
import json

SERVER_HOST = "server1"   # discovery server container name
SERVER_PORT = 5000

def send_message(message):
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.connect((SERVER_HOST, SERVER_PORT))
        s.sendall(json.dumps(message).encode())
        data = s.recv(1024).decode()
        try:
            return json.loads(data)
        except Exception:
            return {"status": "error", "message": f"invalid response: {data}"}

def main():
    print("Discovery Client\nAvailable commands: signup, register, peers, deregister, quit")

    while True:
        cmd = input("\nEnter command: ").strip().lower()

        if cmd == "signup":
            username = input("Username: ").strip()
            password = input("Password: ").strip()
            msg = {"type": "signup", "username": username, "password": password}
            print("Signup:", send_message(msg))

        elif cmd == "register":
            username = input("Username: ").strip()
            password = input("Password: ").strip()
            port = int(input("Your service port (e.g., 6000): ").strip())
            msg = {
                "type": "register",
                "username": username,
                "password": password,
                "port": port
            }
            print("Register:", send_message(msg))

        elif cmd == "peers":
            msg = {"type": "peers"}
            print("Peers:", send_message(msg))

        elif cmd == "deregister":
            username = input("Username: ").strip()
            password = input("Password: ").strip()
            msg = {"type": "deregister", "username": username, "password": password}
            print("Deregister:", send_message(msg))

        elif cmd == "quit":
            print("Exiting client.")
            break

        else:
            print("Unknown command. Try: signup, register, peers, deregister, quit.")

if __name__ == "__main__":
    main()
