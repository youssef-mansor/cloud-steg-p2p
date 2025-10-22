import socket, json, random, time

SERVERS = [
    ("server1", 5000),
    ("server2", 5000),
    ("server3", 5000),
]
FAILED = {}  # map (host, port) -> last_fail_time
RETRY_AFTER = 10  # seconds

def send_message(message):
    while True:
        # Choose from healthy servers first, but retry failed ones if enough time passed
        now = time.time()
        available = [
            s for s in SERVERS
            if s not in FAILED or (now - FAILED[s]) > RETRY_AFTER
        ]

        if not available:
            print("DEBUG: no available servers, retrying in 2s...")
            time.sleep(2)
            continue

        host, port = random.choice(available)
        try:
            with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
                s.settimeout(2)
                s.connect((host, port))
                s.sendall(json.dumps(message).encode())
                data = s.recv(1024).decode()
                print(f"DEBUG raw server response from {host}:{port} -> {data}")
                response = json.loads(data)
        except Exception as e:
            print(f"DEBUG: failed to reach {host}:{port} ({e})")
            FAILED[(host, port)] = time.time()
            continue

        # --- handle redirect ---
        if response.get("status") == "redirect":
            leader = response.get("leader")
            leader_port = response.get("leader_port")
            if leader and leader_port:
                print(f"DEBUG: redirecting to leader {leader}:{leader_port}")
                try:
                    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
                        s.settimeout(2)
                        s.connect((leader, leader_port))
                        s.sendall(json.dumps(message).encode())
                        data = s.recv(1024).decode()
                        print(f"DEBUG raw server response from {leader}:{leader_port} -> {data}")
                        return json.loads(data)
                except Exception as e:
                    print(f"DEBUG: failed to reach leader {leader}:{leader_port} ({e})")
                    FAILED[(leader, leader_port)] = time.time()
                    continue


        # success: clear failure mark
        if (host, port) in FAILED:
            del FAILED[(host, port)]
        return response


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
