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

## Run the Server

Inside `server1`:

```bash
python3 discovery_server.py
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

## Expected Result

* `client1` sees only itself after first registration.
* After `client2` registers, both clients appear in the peer list.
