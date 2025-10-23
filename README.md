# Discovery Service Prototype

Minimal prototype of a **discovery service** for the cloud project.
Clients can register with a central discovery server and fetch a list of online peers.

---

## Project Structure

```
project-root/
├── docker-compose.yml
├── server/
│   └── discovery_server.py
└── client/
    └── discovery_client.py
```

---

## Prerequisites

* Docker & Docker Compose installed
* Python 3 inside containers

---

## Build & Start

It’s recommended to **build images first** if you have local Dockerfiles:

```bash
docker-compose build
docker-compose up -d
```

> Using `docker-compose up -d` alone works if you are using standard images (e.g., `ubuntu:22.04`), but `build` ensures any customizations in Dockerfiles are applied.

This starts:

* `server1` (discovery server)
* `client1`, `client2`, `client3` (clients)

---

## Run the Servers


```bash
docker exec -it server1 python3 /home/raft_discovery_server.py --id server1 --peers server2,server3
docker exec -it server2 python3 /home/raft_discovery_server.py --id server2 --peers server1,server3
docker exec -it server3 python3 /home/raft_discovery_server.py --id server3 --peers server1,server2
```

---

## Crash failure tolerance

**Stop Server**

```bash
docker stop <server#>
docker start <server#>
docker exec -it server1 python3 /home/raft_discovery_server.py --id server# --peers server$,server*

```

---

## Register Clients

Inside `client1`:

```bash
python3 discovery_client.py
```

Example input:

```
Enter your username: client1
Enter your container name: client1
Enter your port: 6000
```

Inside `client2`:

```bash
python3 discovery_client.py
```

Example input:

```
Enter your username: client2
Enter your container name: client2
Enter your port: 6000
```

---

## Remove Containers

```bash
docker rm -f $(docker ps -q)
```

## Split terminal into Panes
```bash
tmux
Ctrl-b %
Ctrl-b arrow
```

## Expected Result

* `client1` sees only itself after first registration.
* After `client2` registers, both clients appear in the peer list.
