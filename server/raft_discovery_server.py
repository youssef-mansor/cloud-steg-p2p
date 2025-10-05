#!/usr/bin/env python3
import socket, json, threading, time, argparse, random

# ---------------------------
# Args / config
# ---------------------------
parser = argparse.ArgumentParser()
parser.add_argument('--id', required=True, help='this server id/hostname e.g. server1')
parser.add_argument('--peers', default='', help='comma separated peer hostnames e.g. server2,server3')
parser.add_argument('--port', type=int, default=5000)
args = parser.parse_args()

self_id = args.id
peers = [p for p in args.peers.split(',') if p and p != self_id]
PORT = args.port
HOST = '0.0.0.0'

# ---------------------------
# Raft state & app state
# ---------------------------
current_term = 0
voted_for = None
role = 'follower'       # follower | candidate | leader
leader = None
last_heartbeat = time.time()
lock = threading.Lock()

# Minimal app state (leader will handle writes)
accounts = {}           # username -> password
online_clients = {}     # username -> (ip, port)

# Timing
heartbeat_interval = 2.0
election_min = 5.0
election_max = 8.0

def log(msg):
    print(f"[{self_id}] {time.strftime('%H:%M:%S')} {msg}", flush=True)

# ---------------------------
# Networking RPC helper
# ---------------------------
def send_rpc(host, port, message, timeout=1.5):
    try:
        with socket.create_connection((host, port), timeout=timeout) as s:
            s.sendall(json.dumps(message).encode())
            data = s.recv(4096).decode()
            if not data:
                return None
            return json.loads(data)
    except Exception:
        return None

# ---------------------------
# Message handler (clients + server RPCs)
# ---------------------------
def handle_message(msg, addr):
    global current_term, voted_for, role, leader, last_heartbeat, accounts, online_clients
    mtype = msg.get('type')

    # --- server RPC: append_entries (heartbeat) ---
    if mtype == 'append_entries':
        term = msg.get('term', 0)
        leader_id = msg.get('leader')
        with lock:
            if term >= current_term:
                current_term = term
                role = 'follower'
                leader = leader_id
                last_heartbeat = time.time()
                log(f"Received heartbeat from {leader_id} (term {term})")
                log(f"Updated leader to {leader}")  # <-- Add this
                return {'status':'ok'}
            else:
                return {'status':'fail', 'term': current_term}

    # --- server RPC: request_vote ---
    if mtype == 'request_vote':
        term = msg.get('term', 0)
        candidate = msg.get('candidate')
        with lock:
            if term < current_term:
                return {'term': current_term, 'vote_granted': False}
            if term > current_term:
                current_term = term
                voted_for = None
            if voted_for is None or voted_for == candidate:
                voted_for = candidate
                log(f"Voted for {candidate} in term {term}")
                return {'term': current_term, 'vote_granted': True}
            else:
                return {'term': current_term, 'vote_granted': False}

    # --- client write messages (leader-only) ---
    if mtype in ('signup','register','deregister'):
        with lock:
            if role != 'leader':
                if leader:
                    return {'status':'redirect', 'leader': leader, 'leader_port': PORT}
                else:
                    return {'status':'error', 'message':'no-leader-known'}

            # leader handles operations locally (minimal)
            if mtype == 'signup':
                u = msg.get('username'); p = msg.get('password')
                if not u or not p: return {'status':'error','message':'missing fields'}
                if u in accounts: return {'status':'error','message':'exists'}
                accounts[u] = p
                return {'status':'ok','message':'account created'}
            if mtype == 'register':
                u = msg.get('username'); p = msg.get('password'); port = msg.get('port')
                if not u or not p or not port: return {'status':'error','message':'missing fields'}
                if accounts.get(u) != p: return {'status':'error','message':'auth failed'}
                online_clients[u] = (addr[0], port)
                return {'status':'ok','message':'registered'}
            if mtype == 'deregister':
                u = msg.get('username'); p = msg.get('password')
                if accounts.get(u) != p: return {'status':'error','message':'auth failed'}
                online_clients.pop(u, None)
                return {'status':'ok','message':'deregistered'}

    # --- client read ---
    if mtype == 'peers':
        with lock:
            return {'status':'ok','peers': online_clients}

    return {'status':'error','message':'unknown type'}

# ---------------------------
# Per-connection handler
# ---------------------------
def handle_client(conn, addr):
    with conn:
        try:
            data = conn.recv(4096).decode()
            if not data: return
            msg = json.loads(data)
        except Exception:
            return
        resp = handle_message(msg, addr)
        try:
            conn.sendall(json.dumps(resp).encode())
        except Exception:
            pass

# ---------------------------
# Accept loop
# ---------------------------
def accept_loop():
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind((HOST, PORT))
        s.listen()
        log(f"Listening on {PORT}. Peers: {peers}")
        while True:
            conn, addr = s.accept()
            threading.Thread(target=handle_client, args=(conn, addr), daemon=True).start()

# ---------------------------
# Election / heartbeats
# ---------------------------
def become_leader():
    global role, leader, voted_for
    with lock:
        role = 'leader'
        leader = self_id
        voted_for = self_id
    log(f"Became leader (term {current_term})")
    start_heartbeat()

def start_heartbeat():
    def hb_loop():
        while True:
            time.sleep(heartbeat_interval)
            with lock:
                if role != 'leader':
                    return
                term = current_term
                leader_id = self_id
            for p in peers:
                # fire-and-forget heartbeats
                threading.Thread(target=send_rpc, args=(p, PORT, {'type':'append_entries','term':term,'leader':leader_id}), daemon=True).start()
            log("Sent heartbeats")
    threading.Thread(target=hb_loop, daemon=True).start()

def start_election():
    global current_term, role, voted_for
    with lock:
        current_term += 1
        term = current_term
        role = 'candidate'
        voted_for = self_id
    log(f"Starting election for term {term}")
    votes = 1
    for p in peers:
        resp = send_rpc(p, PORT, {'type':'request_vote','term':term,'candidate':self_id}, timeout=1.5)
        if resp and resp.get('vote_granted'):
            votes += 1
    cluster_size = len(peers) + 1
    log(f"Election result: {votes}/{cluster_size} votes")
    if votes > cluster_size // 2:
        become_leader()
    else:
        # remain follower after sleep (let timer pick up)
        with lock:
            role = 'follower'

def election_timer():
    global last_heartbeat
    while True:
        timeout = random.uniform(election_min, election_max)
        time.sleep(timeout)
        with lock:
            if role == 'leader':
                continue
            since = time.time() - last_heartbeat
        if since >= timeout:
            start_election()

# ---------------------------
# Main
# ---------------------------
if __name__ == '__main__':
    threading.Thread(target=accept_loop, daemon=True).start()
    threading.Thread(target=election_timer, daemon=True).start()
    log("Server started (raft scaffold)")
    while True:
        time.sleep(1)
