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
| `DEFAULT_CATEGORY` | ❌ | `sonarr` | Nom de catégorie exposée à Sonarr |
| `PORT` | ❌ | `8080` | Port d'écoute du proxy |
| `RUST_LOG` | ❌ | `info` | Niveau de log (`info`, `debug`, `trace`) |

## Démarrage rapide

### Docker

```bash
docker build -t pyload-proxy-for-sonarr .

docker run --rm -p 8080:8080 \
  -e SABNZBD_API_KEY=changeme \
  -e PYLOAD_URL=http://pyload:8000 \
  -e PYLOAD_API_KEY=your-pyload-api-key \
  -e DOWNLOAD_DIR=/downloads \
  pyload-proxy-for-sonarr
```

### Cargo (dev)

```bash
SABNZBD_API_KEY=changeme \
PYLOAD_URL=http://localhost:8000 \
PYLOAD_API_KEY=your-pyload-api-key \
cargo run --release
```

## Configuration de Sonarr

1. **Settings** → **Download Clients** → **Add** → **SABnzbd**
2. Renseigner :
   - **Host** : l'hôte où tourne le proxy (ex. `pyload-proxy`)
   - **Port** : `8080`
   - **URL Base** : laisser vide (le proxy écoute `/api` à la racine)
   - **API Key** : la valeur de `SABNZBD_API_KEY`
   - **Category** : `sonarr` (ou la valeur de `DEFAULT_CATEGORY`)
3. **Test** → doit passer ✅

Le proxy expose aussi `/sabnzbd/api` si tu préfères mettre `URL Base = sabnzbd`.

## Comment ça marche

### Mapping SABnzbd → pyLoad

| Mode SAB (appelé par Sonarr) | Action côté pyLoad |
|---|---|
| `version`, `get_config`, `fullstatus`, `warnings`, `options` | Réponses synthétiques |
| `addurl` | `POST /api/add_package` avec le lien |
| `addfile` (multipart) | Le fichier est parsé ligne à ligne, les URLs `http(s)://`/`ftp://` deviennent un package |
| `queue` | `GET /api/get_queue_data` → mappé en slots SAB (paquets non finis) |
| `history` | `GET /api/get_queue_data` + `get_collector_data` → slots SAB (paquets finis) |
| `queue&name=delete`, `history&name=delete` | `POST /api/delete_packages` |
| `pause`, `resume`, `shutdown` | No-op (retourne `{"status": true}`) |

Les `nzo_id` retournés à Sonarr sont au format `pyld_<pid>` où `<pid>` est
l'identifiant de paquet pyLoad. Ça permet à Sonarr de tracker l'avancement et
de demander la suppression après import.

### Authentification

- **Sonarr → proxy** : clé d'API en query string (`?apikey=…`), comme un vrai SAB.
- **Proxy → pyLoad** : header `X-API-Key` sur chaque requête. La clé se génère
  côté pyLoad (interface web → *Settings* → *General* → *API key*, ou via le
  fichier de config).

### Chemins de fichiers

Pour que Sonarr puisse importer les fichiers après téléchargement, il faut que
le dossier `DOWNLOAD_DIR` du proxy corresponde **au chemin tel que Sonarr le
voit**, et que pyLoad écrive bien dans ce volume.

Exemple `docker-compose.yml` :

```yaml
services:
  pyload:
    image: pyload/pyload
    volumes:
      - ./downloads:/opt/pyload/data/Downloads

  proxy:
    build: .
    environment:
      SABNZBD_API_KEY: changeme
      PYLOAD_URL: http://pyload:8000
      PYLOAD_USERNAME: admin
      PYLOAD_PASSWORD: secret
      DOWNLOAD_DIR: /downloads

  sonarr:
    image: linuxserver/sonarr
    volumes:
      - ./downloads:/downloads
```

Ici Sonarr voit `/downloads/<package>` et y trouve les fichiers que pyLoad a
écrits dans `/opt/pyload/data/Downloads/<package>` sur son côté — même volume
hôte, chemins logiques différents. C'est `DOWNLOAD_DIR` (côté Sonarr) qu'on
renvoie dans le champ `storage` de l'historique.

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
2. **Final** : `scratch` + le binaire seul. Tourne en UID `1000`.

TLS sortant (vers pyLoad en HTTPS) marche sans CA mounté : on utilise `rustls`
avec `webpki-roots` (certifs Mozilla embarqués dans le binaire).

## Limitations connues

- **pas de TLS côté serveur** : le proxy écoute en HTTP. À mettre derrière un
  reverse proxy (Caddy, Traefik) si exposé.
- **`addfile`** : seuls les fichiers contenant des URLs en clair (un par ligne)
  sont supportés. Les containers chiffrés (DLC, CCF) ne sont pas décodés.
- **Compatibilité pyLoad** : ciblé sur la branche moderne (pyLoad-ng / `0.5+`).
  L'ancienne API XML-RPC de pyLoad 0.4.x n'est pas supportée.
- **Pas de support multi-utilisateurs pyLoad** : un seul couple
  user/password est utilisé.

## Licence

À définir.
