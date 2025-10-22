![[Pasted image 20251022013518.png]]

**Why didn't the signup request followed the first redirect from server2?**

**Tracing**
`"DEBUG raw server response from {host}:{port} -> {data}"`
discovery_client.py→main()→send_message(message)→print()
message = {"type": "signup", "username": username, "password": password}

`"[{self_id}] Redirecting to leader {leader}"`
I doubt redirect doesn't send really to leader!
I doubt send message is called multiple times.

![[Pasted image 20251022015437.png]]
after first signup, reaching follower servers always fail!

main().accept_loop().handle_client()