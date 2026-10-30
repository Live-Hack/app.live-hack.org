# Labs

A lab is a private copy of a challenge's service, started by a player (or for a whole team
during a team event) and stopped automatically.

## How it works

1. The player clicks **Start lab**. Live Hack reserves the player's single lab slot (one
   live lab per player or team, enforced by a unique key) and starts it in the background.
2. The image is pulled if missing, a dedicated bridge network is created, and one container
   is started with the challenge port published on a random port of the Docker host.
3. The page polls until the lab runs, then shows `http://<INSTANCE_HOST>:<port>` (HTTP) or
   `nc <INSTANCE_HOST> <port>` (TCP), a countdown, **Extend** and **Stop**.
4. A background sweeper stops labs that expired, labs of ended events, and labs stuck
   starting for 10 minutes; it also removes containers and networks no live lab owns.

Published ports rather than HTTP sub-domains: raw TCP services (pwn, network challenges)
work the same way as web ones, and no wildcard DNS or certificate is needed.

## Writing a lab image

Any image that listens on one port works. The platform injects the flag as `$FLAG`:

- with a static, case-insensitive or regex flag check, `$FLAG` is the challenge flag;
- with the **per instance** check, `$FLAG` is a fresh secret for this lab only, built from
  the platform flag format (`LH{...}` → `LH{3f9c…}`).

Example (`examples/labs/web-hello`):

```dockerfile
FROM busybox:1.36
RUN adduser -D -H app
USER app
EXPOSE 8080
CMD ["sh", "-c", "mkdir -p /tmp/www && printf '<p>flag: %s</p>' \"$FLAG\" > /tmp/www/index.html && exec httpd -f -p 8080 -h /tmp/www"]
```

Build or push the image where the Docker host can pull it, then set image, port and
protocol in the challenge editor or in [challenge.yml](Challenge-Format).

## Isolation and limits

| Control | Value |
| --- | --- |
| Network | One bridge network per lab, inter-container traffic disabled. |
| Memory | Per challenge (default 256 MB), no extra swap. |
| CPU | Per challenge in millicores (default 500). |
| Processes | 256. |
| Capabilities | `NET_RAW`, `MKNOD`, `AUDIT_WRITE`, `SYS_CHROOT`, `SETFCAP` dropped; `no-new-privileges`. |
| Filesystem | Optionally read-only, with a 64 MB writable `/tmp`. |
| Lifetime | Per challenge (default 60 min); **Extend** adds one lifetime, up to 4 in total. |

Labs keep outbound internet access by default. Use Docker's own egress controls on the
host if your challenges must not reach the internet.

## Docker access

The app never mounts the Docker socket. It talks to `docker-proxy`, which only allows the
container, network and image APIs (no exec, no volumes, no swarm). Remove the proxy and
unset `DOCKER_HOST` to run without labs.

## Operations

- `INSTANCE_HOST` must be a name players can reach; open the Docker host's ephemeral port
  range on its firewall.
- `MAX_INSTANCES` caps concurrent labs; when full, players are asked to retry.
- **Admin → Labs** lists running labs with their owner, port and expiry, shows container
  logs and stops a lab. **Admin → Settings → Labs** pauses new starts.
- `LABS_NAMESPACE` lets several platforms share one Docker host: each only cleans its own
  labs.
