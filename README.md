# pyload-proxy-for-sonarr

Proxy HTTP qui fait passer [Sonarr](https://github.com/Sonarr/Sonarr) pour
parler à [pyLoad](https://github.com/pyload/pyload) en émulant l'API SABnzbd.

Sonarr n'a pas de support natif pour pyLoad (qui est un *download manager*
HTTP/DDL, pas un client NZB/torrent). Ce proxy expose les endpoints SABnzbd
que Sonarr appelle et les traduit en appels REST pyLoad — Sonarr croit parler
à un SABnzbd, le téléchargement réel passe par pyLoad.

```
┌────────┐  SAB API   ┌──────────────────────────┐  REST API  ┌────────┐
│ Sonarr ├───────────►│ pyload-proxy-for-sonarr  ├───────────►│ pyLoad │
└────────┘            └──────────────────────────┘            └────────┘
```

Écrit en Rust (axum + reqwest/rustls), packagé dans une image `FROM scratch`
(~10 Mo, sans OpenSSL, sans certifs à monter au runtime).

## Configuration

Tout passe par variables d'environnement.

| Variable | Requis | Défaut | Description |
|---|---|---|---|
| `SABNZBD_API_KEY` | ✅ | — | Clé d'API que Sonarr doit fournir au proxy |
| `PYLOAD_URL` | ✅ | — | URL de pyLoad, ex. `http://pyload:8000` |
| `PYLOAD_API_KEY` | ✅ | — | Clé d'API pyLoad (header `X-API-Key`) |
| `DOWNLOAD_DIR` | ❌ | `/downloads` | Chemin de téléchargement **tel que Sonarr le voit** (utilisé pour `storage` dans l'historique) |
| `PYLOAD_DEST` | ❌ | `1` | Destination pyLoad : `0` = Collector, `1` = Queue |
| `DEFAULT_CATEGORY` | ❌ | `sonarr` | Nom de catégorie exposée à Sonarr (doit matcher la *Category* configurée dans Sonarr) |
| `PORT` | ❌ | `8080` | Port d'écoute du proxy |
| `RUST_LOG` | ❌ | `info` | Niveau de log (`info`, `debug`, `trace`) |

La clé d'API pyLoad se génère côté pyLoad (interface web → *Settings* →
*General* → *API key*, ou dans le fichier `config/api_key.json` après
premier démarrage avec l'image `linuxserver/pyload-ng`).

## Démarrage rapide

### Image pré-buildée (GHCR)

Une image est publiée à chaque release sur `ghcr.io` :

```bash
docker pull ghcr.io/dim145/pyloadproxyforsonarr:latest

docker run --rm -p 8080:8080 \
  -e SABNZBD_API_KEY=changeme \
  -e PYLOAD_URL=http://pyload:8000 \
  -e PYLOAD_API_KEY=your-pyload-api-key \
  -e DOWNLOAD_DIR=/downloads \
  ghcr.io/dim145/pyloadproxyforsonarr:latest
```

L'image est multi-arch (`linux/amd64` + `linux/arm64`).

### Build local

```bash
docker build -t pyload-proxy-for-sonarr .

docker run --rm -p 8080:8080 \
  -e SABNZBD_API_KEY=changeme \
  -e PYLOAD_URL=http://pyload:8000 \
  -e PYLOAD_API_KEY=your-pyload-api-key \
  pyload-proxy-for-sonarr
```

### Cargo (dev)

```bash
SABNZBD_API_KEY=changeme \
PYLOAD_URL=http://localhost:8000 \
PYLOAD_API_KEY=your-pyload-api-key \
cargo run --release
```

## docker-compose.yml

Stack complète Sonarr + proxy + pyLoad avec volume partagé et `depends_on`
qui attend la santé du proxy :

```yaml
services:
  pyload:
    image: lscr.io/linuxserver/pyload-ng:latest
    container_name: pyload
    environment:
      PUID: "1000"
      PGID: "1000"
      TZ: Europe/Paris
    volumes:
      - ./data/pyload/config:/config
      - ./data/downloads:/downloads
    ports:
      - "8000:8000"
    restart: unless-stopped

  pyload-proxy:
    image: ghcr.io/dim145/pyloadproxyforsonarr:latest    # ou build: .
    container_name: pyload-proxy
    depends_on:
      - pyload
    environment:
      SABNZBD_API_KEY: changeme-please-rotate-this
      PYLOAD_URL: http://pyload:8000
      PYLOAD_API_KEY: paste-your-pyload-api-key-here
      DOWNLOAD_DIR: /downloads
      DEFAULT_CATEGORY: sonarr
      PYLOAD_DEST: "1"
      RUST_LOG: info
    ports:
      - "8080:8080"
    restart: unless-stopped

  sonarr:
    image: lscr.io/linuxserver/sonarr:latest
    container_name: sonarr
    environment:
      PUID: "1000"
      PGID: "1000"
      TZ: Europe/Paris
    volumes:
      - ./data/sonarr/config:/config
      - ./data/downloads:/downloads
      - ./data/tv:/tv
    ports:
      - "8989:8989"
    depends_on:
      pyload-proxy:
        condition: service_healthy
    restart: unless-stopped
```

**Premier démarrage** :
1. `docker compose up pyload` → ouvrir `http://localhost:8000`, créer le compte
   admin, copier la clé API depuis *Settings → General → API key*.
2. Mettre cette valeur dans `PYLOAD_API_KEY` du compose.
3. `docker compose up -d` pour tout lancer.

Le volume `./data/downloads` est partagé entre pyLoad et Sonarr → quand le
proxy renvoie `storage: /downloads/<package>` dans l'historique, Sonarr
trouve les fichiers via le même point de montage. C'est `DOWNLOAD_DIR` (côté
Sonarr) qu'on renvoie, donc il doit matcher le chemin tel que Sonarr le voit.

## Configuration de Sonarr

1. **Settings** → **Download Clients** → **Add** → **SABnzbd**
2. Renseigner :
   - **Host** : l'hôte où tourne le proxy (ex. `pyload-proxy`)
   - **Port** : `8080`
   - **URL Base** : laisser vide (le proxy écoute `/api` à la racine, mais
     `/sabnzbd/api` est aussi disponible si tu préfères `URL Base = sabnzbd`)
   - **API Key** : la valeur de `SABNZBD_API_KEY`
   - **Category** : **doit** matcher `DEFAULT_CATEGORY` (par défaut `sonarr`).
     L'auto-suggestion `tv-sonarr` de Sonarr ne marchera pas sans changer
     `DEFAULT_CATEGORY` côté proxy.
3. **Test** → doit passer ✅

## Healthcheck

Endpoint `/health` (sans auth) qui appelle `get_server_version` côté pyLoad :

- `200 {"status": "ok", "pyload_version": "..."}` si pyLoad répond
- `503 {"status": "unhealthy"}` sinon

Le binaire supporte aussi un mode `--healthcheck` qui ping son propre
`/health` et retourne `0`/`1` — utilisé par la directive `HEALTHCHECK` du
Dockerfile (puisque l'image `FROM scratch` n'a ni curl ni wget).

```bash
curl http://localhost:8080/health
docker ps  # affiche (healthy) ou (unhealthy)
```

## Comment ça marche

### Mapping SABnzbd → pyLoad

| Mode SAB (appelé par Sonarr) | Action côté pyLoad |
|---|---|
| `version`, `get_config`, `fullstatus`, `warnings`, `options` | Réponses synthétiques |
| `addurl` | `POST /api/add_package` avec le lien |
| `addfile` (multipart) | Le fichier est parsé ligne à ligne, les URLs `http(s)://`/`ftp://` deviennent un package |
| `queue` | `GET /api/get_queue_data` + `get_collector_data` + `status_downloads` → slots SAB (paquets non finis) |
| `history` | `GET /api/get_queue_data` + `get_collector_data` → slots SAB (paquets finis) |
| `queue&name=delete`, `history&name=delete` | `POST /api/delete_packages` |
| `pause`, `resume`, `shutdown` | No-op (retourne `{"status": true}`) |

Les `nzo_id` retournés à Sonarr sont au format `pyld_<pid>` où `<pid>` est
l'identifiant de paquet pyLoad — permet à Sonarr de tracker et supprimer.

### États mappés

Le statut de package SAB est déduit de `links[].status` (`DownloadStatus`
pyLoad) :

| Statut pyLoad | État SAB |
|---|---|
| `FINISHED(0)` | `Completed` (history) |
| `FAILED(8)`, `ABORTED(9)`, `OFFLINE(1)`, `TEMPOFFLINE(6)` | `Failed` (history) |
| `DOWNLOADING(12)`, `STARTING(7)`, `DECRYPTING(10)`, `PROCESSING(13)` | `Downloading` (queue) |
| `WAITING(5)`, `QUEUED(3)`, `ONLINE(2)` | `Queued` (queue) |
| serveur pyLoad en pause | `Paused` (queue) |

### Progression live

`Package.sizedone` dans `get_queue_data` n'est mis à jour qu'à la fin de
chaque fichier — utiliser uniquement cet agrégat laisse la barre de
progression à 0% pendant tout le download. On fusionne donc avec
`/api/status_downloads` qui renvoie un `DownloadInfo` par fichier en cours :

- `bleft` / `size` → progression bytes live
- `speed` → débit
- `eta` → temps restant (calculé par pyLoad côté serveur)
- `wait_until` → quand un fichier est en `WAITING` (ex: cooldown free-tier
  OneFichier), `max(eta, wait_until - now)` est remonté comme temps restant

Si `status_downloads` ne contient pas de ligne pour un package (typique des
paquets purement en queue), on retombe sur les agrégés du package.

### Authentification

- **Sonarr → proxy** : clé d'API en query string (`?apikey=…`), comme un vrai SAB.
- **Proxy → pyLoad** : header `X-API-Key` sur chaque requête.

## Release / publication d'image

Le workflow [.github/workflows/release.yml](.github/workflows/release.yml)
build et push l'image multi-arch sur `ghcr.io` à chaque release publiée sur
GitHub. Pour publier une version :

```bash
git tag v0.1.0
git push origin v0.1.0
# créer la release sur GitHub depuis ce tag
```

L'image est privée par défaut — pour autoriser le `docker pull` anonyme,
passe la visibilité du package à **Public** dans les *Package settings* du
repo GitHub.

## Build

### Local

```bash
cargo build --release
```

### Image Docker (`FROM scratch`)

```bash
docker build -t pyload-proxy-for-sonarr .
```

Le `Dockerfile` est multi-stage :
1. **Builder** : `rust:1.85-alpine` (musl par défaut) compile un binaire statique.
2. **Final** : `scratch` + le binaire seul. Tourne en UID `1000` avec
   `HEALTHCHECK` qui appelle le binaire en mode `--healthcheck`.

TLS sortant (vers pyLoad en HTTPS) marche sans CA mounté : on utilise `rustls`
avec `webpki-roots` (certifs Mozilla embarqués dans le binaire).

## Limitations connues

- **Pas de TLS côté serveur** : le proxy écoute en HTTP. À mettre derrière un
  reverse proxy (Caddy, Traefik) si exposé.
- **`addfile`** : seuls les fichiers contenant des URLs en clair (un par ligne)
  sont supportés. Les containers chiffrés (DLC, CCF) ne sont pas décodés.
- **Compatibilité pyLoad** : ciblé sur la branche moderne (pyLoad-ng / `0.5+`,
  avec l'API `X-API-Key`). L'ancienne API XML-RPC de pyLoad 0.4.x n'est pas
  supportée.
- **Filtre Sonarr par catégorie** : si la *Category* configurée dans Sonarr
  ne matche pas `DEFAULT_CATEGORY` du proxy, Sonarr ignore silencieusement
  tous les items du queue (mais pas de l'history).

## Licence

[MIT](LICENSE).
