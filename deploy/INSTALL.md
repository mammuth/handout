# Homelab install (Proxmox LXC)

A Debian 13 container `handout`, unprivileged, on `local-zfs-encrypted`. Binary comes from the GitHub
release built by `.github/workflows/release.yml` (tag `v*`), installed by `handout-update`.

## 1. Create CT (on pve)

```
pct create <ctid> local:vztmpl/debian-13-standard_13.1-2_amd64.tar.zst \
  --hostname handout --cores 1 --memory 256 --swap 256 \
  --rootfs local-zfs-encrypted:8 \
  --net0 name=eth0,bridge=vmbr0,firewall=1,ip=dhcp \
  --unprivileged 1 --features nesting=1 --onboot 1 \
  --tags encrypted --timezone Europe/Berlin
```

## 2. Firewall (on pve, before first start)

`/etc/pve/firewall/<ctid>.fw` (or the same rules via UI / pvesh):

```
[OPTIONS]

enable: 1
policy_in: DROP
policy_out: ACCEPT

[RULES]

IN ACCEPT -i net0 -source <proxy-ip> -p tcp -dport 8080 -log nolog # caddy
OUT ACCEPT -i net0 -dest <lan-cidr> -p udp -dport 53 -log nolog # dns
OUT ACCEPT -i net0 -dest <lan-cidr> -p tcp -dport 53 -log nolog # dns
OUT ACCEPT -i net0 -p udp -dport 67 -log nolog # dhcp
OUT DROP -i net0 -dest <lan-cidr> -log nolog # no lateral movement into the LAN
OUT DROP -i net0 -dest <lan-ula-prefix> -log nolog # same for the LAN IPv6 ULA prefix
```

Inbound: Caddy only (no other LAN hosts; admin via `pct exec`). Outbound: internet only
(GitHub releases, apt); the LAN is blocked except DNS/DHCP.

```
pct start <ctid>
```

## 3. Install inside the CT

```
ssh root@<proxmox-host> mkdir -p /root/handout-deploy
scp deploy/{handout.service,handout-update,handout.env.example} root@<proxmox-host>:/root/handout-deploy/
# on pve, in /root/handout-deploy:
pct exec <ctid> -- bash -c 'apt-get update && apt-get -y upgrade && apt-get -y install curl jq ca-certificates'
pct push <ctid> handout.service /etc/systemd/system/handout.service
pct push <ctid> handout-update /usr/local/sbin/handout-update --perms 0755
pct push <ctid> handout.env.example /etc/handout.env --perms 0600
pct exec <ctid> -- install -d -m 0700 /etc/handout
# private repo only: fine-grained PAT, this repo, Contents: read
pct exec <ctid> -- bash -c 'umask 077; cat > /etc/handout/github-token'   # paste token, Ctrl-D
pct exec <ctid> -- systemctl enable handout
pct exec <ctid> -- handout-update           # downloads latest release, starts, health-checks
```

Create the owner as the service's DynamicUser (so DB files get the right owner):

```
pct exec <ctid> -- systemd-run --pty --wait --collect \
  -p DynamicUser=yes -p StateDirectory=handout -p EnvironmentFile=/etc/handout.env \
  -E HANDOUT_DATA_DIR=/var/lib/handout \
  /usr/local/bin/handout owner add max --name "Max"
```

## 4. Caddy

Append `Caddyfile.snippet` to `/etc/caddy/Caddyfile`, then:

```
pct exec <caddy-ctid> -- caddy validate --config /etc/caddy/Caddyfile
pct exec <caddy-ctid> -- systemctl reload caddy
```

## 5. Smoke-test matrix (if you keep a firewall probe script)

```
expect_allowed 'Caddy -> handout' <caddy-ctid> handout.lan 8080
expect_blocked 'Grafana -/-> handout' <other-ctid> handout.lan 8080
expect_blocked 'handout -/-> ntfy' <ctid> <other-service> 80
```

then rerun it.

## Update / rollback

```
pct exec <ctid> -- handout-update           # latest
pct exec <ctid> -- handout-update v0.2.0    # pin / roll back to a tag
```

`handout-update` verifies sha256, keeps the previous binary as `handout.prev`, and restores it if
`GET /` doesn't answer within 20 s.
