#!/usr/bin/env python3
import socket, json, threading, time, argparse, random, os

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
# Log files
# ---------------------------
STATE_FILE = f"/{self_id}_state.json"
LOG_FILE = f"/{self_id}_log.json"


def save_state():
    print(f"[{self_id}] DEBUG saving state to disk")
    with lock:
        state = {
            'current_term': current_term,
            'voted_for': voted_for,
            'log_entries': log_entries,
            'commit_index': commit_index,
            'last_applied': last_applied,
            'accounts': accounts,
            'online_clients': online_clients
        }
        with open(STATE_FILE, 'w') as f:
            json.dump(state, f)
            print(f"[{self_id}] DEBUG state saved: {state}")
        with open(LOG_FILE, 'w') as f:
            json.dump(log_entries, f)
            print(f"[{self_id}] DEBUG log entries saved: {log_entries}")

def load_state():
    global current_term, voted_for, log_entries, commit_index, last_applied, accounts, online_clients
    if os.path.exists(STATE_FILE):
        with open(STATE_FILE, 'r') as f:
            try:
                state = json.load(f)
                current_term = state.get('current_term', 0)
                voted_for = state.get('voted_for')
                log_entries[:] = state.get('log_entries', [])
                commit_index = state.get('commit_index', -1)
                last_applied = state.get('last_applied', -1)
                accounts = state.get('accounts', {})
                online_clients = state.get('online_clients', {})
                log(f"Recovered state from disk: term={current_term}, commit_index={commit_index}, log_len={len(log_entries)}")
            except Exception as e:
                log(f"Failed to load state: {e}")
    else:
        log("No existing state file; starting fresh")


# ---------------------------
# Raft state & app state
# ---------------------------
current_term = 0
voted_for = None
role = 'follower'       # follower | candidate | leader
leader = None
last_heartbeat = time.time()
lock = threading.RLock()
log_entries = []       # existing: list of {'term':int,'op':str,'data':dict}
commit_index = -1      # index (into log_entries) of highest committed entry
last_applied = -1      # index of highest entry applied to state machine
# leader-only replication tracking (only used while leader)
next_index = {}        # peer -> next log index to send (optional)
match_index = {}       # peer -> highest index known replicated on peer


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
# apply committed entries to state machine helper
# ---------------------------  
def apply_committed_entries():
    global last_applied, commit_index, accounts, online_clients
    with lock:
        while last_applied < commit_index:
            last_applied += 1
            entry = log_entries[last_applied]
            print(f"[{self_id}] DEBUG applying committed entry at idx {last_applied}: {entry}")
            # print the whole list
            print(f"[{self_id}] DEBUG full log entries: {log_entries}")
            op = entry.get('op')
            data = entry.get('data', {})
            if op == 'signup':
                u = data.get('username'); p = data.get('password')
                if u and p:
                    accounts[u] = p
                    save_state()
                    log(f"Applied signup for {u} (idx={last_applied})")
            elif op == 'register':
                u = data.get('username'); p = data.get('password'); port = data.get('port')
                if accounts.get(u) == p:
                    online_clients[u] = (data.get('_client_ip', 'unknown'), port)
                    save_state()
                    log(f"Applied register for {u} (idx={last_applied})")
            elif op == 'deregister':
                u = data.get('username'); p = data.get('password')
                if accounts.get(u) == p:
                    online_clients.pop(u, None)
                    save_state()
                    log(f"Applied deregister for {u} (idx={last_applied})")
            else:
                log(f"Unknown op {op} at idx {last_applied}")


# ---------------------------
# Leader: replicate entry and udpate commit index helper
# ---------------------------    
# def replicate_entry_and_commit(entry, timeout=1.5):
#     """
#     Leader: send entry to all peers, gather replies, and if majority acked,
#     update commit_index and apply locally.
#     """
#     global commit_index, last_applied, match_index

#     # append to local log first (leader already did this in your flow)
#     entry_idx = len(log_entries) - 1

#     # send entry to peers and count successes
#     acks = 1  # leader itself
#     responses = []

#     for p in peers:
#         resp = send_rpc(p, PORT, {
#             'type': 'append_entries',
#             'term': current_term,
#             'leader': self_id,
#             'entries': [entry],
#             'leader_commit': commit_index
#         }, timeout=timeout)
#         if resp and resp.get('status') == 'ok':
#             acks += 1
#             match_index[p] = entry_idx
#         else:
#             # leave match_index as-is; future heartbeats/replicas can fix up
#             pass

#     cluster_size = len(peers) + 1
#     if acks > cluster_size // 2:
#         with lock:
#             commit_index = entry_idx
#         log(f"Entry {entry_idx} committed (acks={acks}/{cluster_size})")
#         # apply committed entries locally
#         apply_committed_entries()
#     else:
#         log(f"Entry {entry_idx} NOT committed (acks={acks}/{cluster_size})")
def replicate_entry_and_commit(entry, timeout=1.5):
    global commit_index, last_applied, match_index, next_index

    # entry index on leader
    entry_idx = len(log_entries) - 1

    # initialize next_index for any new peer (should be done in become_leader, but be safe)
    for p in peers:
        next_index.setdefault(p, len(log_entries))
        match_index.setdefault(p, -1)

    acks = 1  # leader itself

    for p in peers:
        # try to replicate until follower accepts or we can't go further back
        while True:
            ni = next_index.get(p, len(log_entries))
            prev_idx = ni - 1
            prev_term = log_entries[prev_idx]['term'] if prev_idx >= 0 else -1
            # send all entries from next_index onwards (could be 1 entry or more)
            entries_to_send = log_entries[ni:]
            msg = {
                'type': 'append_entries',
                'term': current_term,
                'leader': self_id,
                'prev_log_index': prev_idx,
                'prev_log_term': prev_term,
                'entries': entries_to_send,
                'leader_commit': commit_index
            }
            resp = send_rpc(p, PORT, msg, timeout=timeout)
            if resp is None:
                # RPC failure: treat as transient; break and let future heartbeats retry
                break
            if resp.get('status') == 'ok':
                follower_last = resp.get('last_index')
                print(f"[{self_id}] DEBUG replicate_entry_and_commit: peer {p} responded OK with last_index {follower_last}")
                if follower_last is not None:
                    match_index[p] = follower_last
                    next_index[p] = follower_last + 1
                    print(f"[{self_id}] DEBUG replicate_entry_and_commit: peer {p} accepted up to index {follower_last}")
                else:
                    match_index[p] = entry_idx
                    next_index[p] = entry_idx + 1
                    print(f"[{self_id}] DEBUG replicate_entry_and_commit: peer {p} still behind at index {entry_idx}")
                acks += 1
                break
            else:
                # follower rejected due to prev_log mismatch -> back up next_index and retry
                # decrement next_index but don't go below 0
                print(f"[{self_id}] DEBUG replicate_entry_and_commit: peer {p} rejected, backing up next_index")
                if ni > 0:
                    next_index[p] = max(0, ni - 1)
                    # loop and retry immediately (bounded by next_index lowering)
                    continue
                else:
                    # can't back up further
                    break

    cluster_size = len(peers) + 1
    if acks > cluster_size // 2:
        with lock:
            commit_index = entry_idx
        log(f"Entry {entry_idx} committed (acks={acks}/{cluster_size})")
        apply_committed_entries()
        save_state()
    else:
        log(f"Entry {entry_idx} NOT committed (acks={acks}/{cluster_size})")



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
    global log_entries, commit_index, last_applied   # <-- ADD THIS LINE
    mtype = msg.get('type')

    # --- server RPC: append_entries (heartbeat) ---
    # if mtype == 'append_entries':
    #     term = msg.get('term', 0)
    #     leader_id = msg.get('leader')
    #     entries = msg.get('entries', [])
    #     leader_commit = msg.get('leader_commit', None)   # <-- default None
    #     with lock:
    #         if term >= current_term:
    #             current_term = term
    #             role = 'follower'
    #             leader = leader_id
    #             last_heartbeat = time.time()
    #             if entries:
    #                 log(f"Received {len(entries)} new entries from {leader_id}")
    #                 log_entries.extend(entries)
    #                 print(f"[{self_id}] DEBUG log entries after extend: {log_entries}")
    #             # update commit index to min(leader_commit, last log index)
    #             if leader_commit is not None and leader_commit > commit_index:
    #                 new_commit = min(leader_commit, len(log_entries)-1)
    #                 commit_index = new_commit
    #                 log(f"Updated commit_index to {commit_index} from leader {leader_id}")
    #                 apply_committed_entries()
    #             return {'status':'ok'}
    #         else:
    #             log(f"Ignored outdated heartbeat from {leader_id} (term {term}), current term {current_term}")
    #             return {'status':'fail', 'term': current_term}


    if mtype == 'append_entries':
        term = msg.get('term', 0)
        leader_id = msg.get('leader')
        entries = msg.get('entries', [])
        leader_commit = msg.get('leader_commit', None)
        prev_log_index = msg.get('prev_log_index', None)
        prev_log_term = msg.get('prev_log_term', None)

        with lock:
            if term >= current_term:
                current_term = term
                role = 'follower'
                leader = leader_id
                last_heartbeat = time.time()

                # consistency check if leader supplied prev_log info
                if prev_log_index is not None and prev_log_index >= 0:
                    if prev_log_index >= len(log_entries) or log_entries[prev_log_index]['term'] != prev_log_term:
                        # follower's log doesn't match leader at prev_log_index -> reject
                        log(f"Rejecting append_entries from {leader_id}: prev_log mismatch (prev_idx={prev_log_index})")
                        return {'status': 'fail'}

                # truncate any conflicting entries and append new ones
                if entries:
                    # if prev_log_index is specified, ensure we append after it
                    if prev_log_index is None:
                        # default: append after current end
                        base = len(log_entries) - 1
                    else:
                        base = prev_log_index
                    # drop entries after base
                    log_entries[:] = log_entries[:base+1]
                    log_entries.extend(entries)
                    log(f"Received {len(entries)} new entries from {leader_id}")
                    print(f"[{self_id}] DEBUG log entries after extend: {log_entries}")

                # update commit index to min(leader_commit, last log index)
                if leader_commit is not None and leader_commit > commit_index:
                    new_commit = min(leader_commit, len(log_entries)-1)
                    commit_index = new_commit
                    log(f"Updated commit_index to {commit_index} from leader {leader_id}")
                    apply_committed_entries()
                return {'type':'catchup' ,'status':'ok', 'last_index': len(log_entries)-1}
            else:
                log(f"Ignored outdated heartbeat from {leader_id} (term {term}), current term {current_term}")
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
        print(f"[{self_id}] DEBUG handling client {mtype} request")
        with lock:
            if role != 'leader':
                if leader:
                    print(f"[{self_id}] Redirecting to leader {leader}")
                    return {'status':'redirect', 'leader': leader, 'leader_port': PORT}
                else:
                    return {'status':'error', 'message':'no-leader-known'}

            # leader handles operations locally (minimal)
            if mtype == 'signup':
                u = msg.get('username'); p = msg.get('password')
                if not u or not p: return {'status':'error','message':'missing fields'}
                if u in accounts: return {'status':'error','message':'exists'}
                accounts[u] = p
                # TODO: refactor into function
                entry = {'term': current_term, 'op': 'signup', 'data': {**msg, '_client_ip': addr[0]}}
                log_entries.append(entry)
                save_state()
                print(f"[{self_id}] DEBUG log entries after signup append: {log_entries}")
                threading.Thread(target=replicate_entry_and_commit, args=(entry,), daemon=True).start()
                # for p in peers:
                #     threading.Thread(target=send_rpc, args=(p, PORT, {
                #         'type': 'append_entries',
                #         'term': current_term,
                #         'leader': self_id,
                #         'entries': [entry]
                #     }), daemon=True).start()
                log(f"Replicated {mtype} entry to followers")
                return {'status':'ok','message':'account created'}
            if mtype == 'register':
                u = msg.get('username'); p = msg.get('password'); port = msg.get('port')
                if not u or not p or not port: return {'status':'error','message':'missing fields'}
                if accounts.get(u) != p: return {'status':'error','message':'auth failed'}
                online_clients[u] = (addr[0], port)
                # TODO: refactor into function
                entry = {'term': current_term, 'op': 'register', 'data': {**msg, '_client_ip': addr[0]}}
                log_entries.append(entry)
                save_state()
                print(f"[{self_id}] DEBUG log entries after register append: {log_entries}")
                threading.Thread(target=replicate_entry_and_commit, args=(entry,), daemon=True).start()
                # for p in peers:
                #     threading.Thread(target=send_rpc, args=(p, PORT, {
                #         'type': 'append_entries',
                #         'term': current_term,
                #         'leader': self_id,
                #         'entries': [entry]
                #     }), daemon=True).start()
                log(f"Replicated {mtype} entry to followers")
                return {'status':'ok','message':'registered'}
            if mtype == 'deregister':
                u = msg.get('username'); p = msg.get('password')
                if accounts.get(u) != p: return {'status':'error','message':'auth failed'}
                online_clients.pop(u, None)
                # TODO: refactor into function
                entry = {'term': current_term, 'op': 'deregister', 'data': {**msg, '_client_ip': addr[0]}}
                log_entries.append(entry)
                save_state()
                print(f"[{self_id}] DEBUG log entries after deregister append: {log_entries}")
                threading.Thread(target=replicate_entry_and_commit, args=(entry,), daemon=True).start()
                # for p in peers:
                #     threading.Thread(target=send_rpc, args=(p, PORT, {
                #         'type': 'append_entries',
                #         'term': current_term,
                #         'leader': self_id,
                #         'entries': [entry]
                #     }), daemon=True).start()
                log(f"Replicated {mtype} entry to followers")
                return {'status':'ok','message':'deregistered'}

    # --- client read ---
    if mtype == 'peers':
        print(f"[{self_id}] DEBUG handling client peers request")
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
            print(f"[{self_id}] DEBUG received message from {addr}: {msg}")
        except Exception:
            print(f"[{self_id}] DEBUG : failed to parse message from {addr}")
            return
        resp = handle_message(msg, addr)
        print(f"[{self_id}] DEBUG generated response to {addr}: {resp}")
        try:
            conn.sendall(json.dumps(resp).encode())
            print(f"[{self_id}] DEBUG  sent response to {addr}: {resp}")
        except Exception:
            print(f"[{self_id}] DEBUG : failed to send response to {addr}")
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
            conn, addr = s.accept() # blocking
            threading.Thread(target=handle_client, args=(conn, addr), daemon=True).start()

# ---------------------------
# Election / heartbeats
# ---------------------------
def become_leader():
    global role, leader, voted_for, next_index, match_index
    with lock:
        role = 'leader'
        leader = self_id
        voted_for = self_id
        # init indexes
        next_idx = len(log_entries)     # next index to send
        next_index = {p: next_idx for p in peers}
        match_index = {p: -1 for p in peers}
        save_state()
    print(f"[{self_id}] DEBUG next_index on become_leader: {next_index}")
    log(f"Became leader (term {current_term})")
    start_heartbeat()

def send_append(p, prev_idx, prev_term, entries_to_send, term, leader_id, leader_commit):
    msg = {
        'type': 'append_entries',
        'term': term,
        'leader': leader_id,
        'prev_log_index': prev_idx,
        'prev_log_term': prev_term,
        'entries': entries_to_send,
        'leader_commit': leader_commit
    }
    resp = send_rpc(p, PORT, msg)
    if resp and resp.get('status') == 'ok':
        follower_last = resp.get('last_index')
        with lock:
            if follower_last is not None:
                match_index[p] = follower_last
                next_index[p] = follower_last + 1



def start_heartbeat():
    def hb_loop():
        while True:
            time.sleep(heartbeat_interval)
            with lock:
                if role != 'leader':
                    return
                term = current_term
                leader_id = self_id
                leader_commit = commit_index    # <-- added
            for p in peers:
                # prev_idx = len(log_entries) - 1
                ni = next_index.get(p, len(log_entries))
                prev_idx = ni - 1
                prev_term = log_entries[prev_idx]['term'] if prev_idx >= 0 else -1
                entries_to_send = log_entries[ni:] if ni < len(log_entries) else []
                threading.Thread(
                    target=send_append,
                    args=(p, prev_idx, prev_term, entries_to_send, term, leader_id, leader_commit),
                    daemon=True
                ).start()
            log("Sent heartbeats")
            print(f"[{self_id}] DEBUG next_index on become_leader: {next_index}")
    threading.Thread(target=hb_loop, daemon=True).start()


def start_election():
    global current_term, role, voted_for
    with lock:
        current_term += 1
        term = current_term
        role = 'candidate'
        voted_for = self_id
        save_state()
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
    load_state()
    print(f"[{self_id}] DEBUG initial log entries: {log_entries}")
    threading.Thread(target=accept_loop, daemon=True).start()
    threading.Thread(target=election_timer, daemon=True).start()
    log("Server started (raft scaffold)")
    while True:
        time.sleep(1)
