# Running your own Kahie sync relay

Kahie syncs notebooks between devices through a **relay** you host yourself (decision D9). The relay only stores and forwards CRDT updates (encrypted in transit with `wss://`); it never interprets notebook content. One small server is enough for a school.

## What you need

- A Linux server or VM reachable by the tablets (school network or internet). 1 vCPU and 1 GB RAM are plenty.
- A domain name pointing to it, for HTTPS (`wss://`). On a closed school network, plain `ws://` on the local network also works.
- About 15 MB of disk per shared page per hour of heavy class use; the relay compacts each log automatically every 1 000 updates (`kahie-relay compact <doc-id>` does it on demand).

## 1. Build

```bash
git clone https://github.com/Heritiana-Ankatso/Kahie.git
cd Kahie/core
cargo build --release -p nb-relay
sudo install -m 755 target/release/kahie-relay /usr/local/bin/
```

## 2. Configure and run as a service

Choose a long random token (`openssl rand -hex 24`). Every device needs it.

`/etc/systemd/system/kahie-relay.service`:

```ini
[Unit]
Description=Kahie sync relay
After=network.target

[Service]
Environment=KAHIE_RELAY_ADDR=127.0.0.1:8787
Environment=KAHIE_RELAY_DATA=/var/lib/kahie-relay
Environment=KAHIE_RELAY_TOKEN=change-me-to-a-long-random-token
ExecStart=/usr/local/bin/kahie-relay
DynamicUser=yes
StateDirectory=kahie-relay
Restart=on-failure

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now kahie-relay
curl http://127.0.0.1:8787/health   # → ok
```

The relay refuses to listen on a non-local address without `KAHIE_RELAY_TOKEN`.

## 3. HTTPS with a reverse proxy

Example with [Caddy](https://caddyserver.com/) (obtains certificates automatically), `/etc/caddy/Caddyfile`:

```
relay.example.org {
    reverse_proxy 127.0.0.1:8787
}
```

WebSocket upgrades are proxied automatically. With nginx, forward `Upgrade` and `Connection` headers.

## 4. Connect the tablets

In Kahie: **Notebooks → More → Sync settings**, enter `wss://relay.example.org` and the token.

- To sync a notebook: **More → Share with my devices**. The first time, Kahie asks whether to encrypt the notebook end to end (recommended; see below). It then shows two codes:
  - the **editor code** (`n….e…` encrypted, `n….w…` not encrypted) for your other devices and co-teachers: full access;
  - the **read-only code** (`n….v…` encrypted, `n….r…` not encrypted) for students: they see the notebook and its updates, but cannot change it.
- On another device: **More → Join a shared notebook**, enter one of the codes.

### Access control

The first device that syncs a notebook with its editor code *claims* it on the relay: the relay stores hashes of the editor key and of the read-only key in `<notebook>.acl` (never the keys themselves). From then on, a connection needs one of the two keys (in addition to the server token); a read-only connection that tries to upload is disconnected. Notebooks shared with older versions of Kahie (codes without a `.`) stay open to anyone with the token and the code.

### End-to-end encryption

With an encrypted code (`e…`/`v…`), the relay stores and forwards only ciphertext:

- **What is encrypted:** every update of every page and of the notebook outline (ink, text, maths, images, titles), with ChaCha20-Poly1305 (`ring`). Each update is `KHE1 ‖ nonce ‖ ciphertext ‖ tag`, bound to its document id, so the relay cannot move an update to another page or alter it unnoticed.
- **Keys:** editors and viewers derive the same content key from the read-only code (editors derive the read-only code from theirs). The secret keys never leave the tablets: the relay receives only credentials derived from them (`w…`/`r…`), and at claim time the hash of the viewers' credential (header `X-Kahie-Read-Hash`). Knowing those credentials gives access to the ciphertext, not to the content.
- **Compaction:** the relay cannot read encrypted updates, so it never compacts those logs; they grow with every change (a few hundred bytes per stroke). Re-share a notebook under a new code if a log becomes large.
- **Not hidden from the relay:** document ids (notebook and page identifiers, not titles), update sizes and times, IP addresses.
- **Codes cannot be recovered.** A lost code means the copy on the relay is unreadable; the notebook on the tablets is unaffected.
- Codes without a key, and plain `w…`/`r…` codes from earlier versions, stay unencrypted. Encryption cannot be turned on for an existing shared notebook: share it again under a new code.

To revoke access, share the notebook again under a new code (export and re-import it, then Share) and delete the old `<notebook>*.db` and `.acl` files on the relay.

## Backups and privacy

- Data lives in `KAHIE_RELAY_DATA`: one SQLite file per document. Back up that directory.
- Anyone with the token and an editor code can read and write that notebook; with a read-only code, read it. Treat codes like passwords.
- With an encrypted code the relay sees only ciphertext (see *End-to-end encryption*); with a plain code it sees update contents. In both cases it sees IP addresses and keeps no other logs.
