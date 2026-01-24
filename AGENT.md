AGENT.md — Projet "P2P-Nitro" (Codename)Vision : Clone de Discord haute performance. Chat & Auth via Serveur Central (Docker/Proxmox). Audio/Vidéo/Streaming en P2P chiffré (E2EE). Architecture Monorepo en Rust.1. Architecture du MonorepoLe projet est structuré pour séparer le binaire client (Tauri) du binaire serveur (Axum) tout en partageant les bibliothèques de cryptographie et les protocoles.Plaintext.
├── Cargo.toml                # Workspace definition
├── apps/
│   ├── desktop/              # Client Tauri (Interface + Media Engine)
│   │   ├── src-tauri/        # Backend Rust du client
│   │   └── src/              # Frontend (React/Vue/Leptos)
│   └── server/               # Backend Axum (Signaling + API + DB)
├── libs/
│   ├── shared-proto/         # Types partagés (Serde, Protobuf, Signaling)
│   ├── e2ee/                 # Logique de chiffrement (XChaCha20, Argon2)
│   └── p2p-core/             # Abstraction WebRTC pour le streaming
├── docker/
│   ├── Dockerfile.server     # Pour le déploiement Proxmox
│   └── docker-compose.yml    # Orchestration API + Postgres
└── AGENT.md
2. La Stack Technique "Nitro"ComposantTechnologieEnvironnementClient UITauri v2MacBook (Local)Media EngineWebRTC-rs + CPALMacBook (Local)Backend APIAxum (Rust)Container (Proxmox)Base de DonnéesPostgreSQL + SQLxContainer (Proxmox)SignalisationWebsocketsContainer (Proxmox)InfrastructureDocker / ProxmoxServeur Distant3. Flux de Données & SécuritéA. Signalisation (Signaling)Pour établir une connexion P2P, le client A et le client B s'échangent des offres SDP via le serveur Axum.Client A -> Serveur (WS) : "Je veux appeler B".Serveur -> Client B (WS) : "A veut t'appeler, voici son ID".Échange des candidats ICE (IPs) via le serveur.Connexion P2P établie : Le serveur ne touche plus aux données média.B. Chiffrement de Bout en Bout (E2EE)Identité : Clés Ed25519 générées sur le MacBook.Media : SRTP (Secure Real-time Transport Protocol) intégré à WebRTC.Messages : Chiffrés avec une clé de session dérivée par Diffie-Hellman avant d'atteindre le serveur Axum.4. Logique Serveur (Apps/Server)Le serveur gère la persistance et la mise en relation.Auth : JWT (JSON Web Tokens) avec stockage des hashs Argon2id.Database : SQLx pour des requêtes vérifiées à la compilation.Scalabilité : Prévu pour tourner dans un container Docker léger sur Proxmox.5. Méthodologie "Mac-as-Editor"Puisque tu ne lances que l'app sur le Mac :Développement : Tu codes dans le workspace. Rust Analyzer tourne sur ton Mac.Test Serveur : Le Dockerfile.server compile le backend. Tu le pousses sur ton Proxmox (via docker compose ou un registry local).Variable d'environnement : L'application Tauri pointe vers l'IP locale de ton Proxmox (ex: http://192.168.1.50:3000).6. Roadmap d'ExécutionPhase 1 : Le Squelette Monorepo[ ] Configurer Cargo.toml (workspace).[ ] Créer le Dockerfile.server pour compiler le backend sur Proxmox.[ ] Setup d'une base de données Postgres sur Proxmox via Docker.Phase 2 : Signaling & Chat de base[ ] Coder la logique Websocket dans apps/server (Axum).[ ] Connecter Tauri au serveur distant pour l'envoi de texte simple.Phase 3 : Le "Gros Morceau" (Audio P2P)[ ] Capture audio locale (Mac).[ ] Encodage Opus.[ ] Échange de signaux P2P via le serveur Axum sur Proxmox.[ ] Flux audio direct Mac <-> Autre Client.Phase 4 : Vidéo & Streaming[ ] Capture d'écran optimisée.[ ] Intégration du Simulcast (gestion des débits).7. Configuration Docker (Pour Proxmox)Exemple du docker-compose.yml que tu placeras sur ton serveur :YAMLservices:
  api:
    build: 
      context: ..
      dockerfile: docker/Dockerfile.server
    ports:
      - "3000:3000"
    environment:
      - DATABASE_URL=postgres://user:pass@db:5432/nitro
  db:
    image: postgres:16-alpine
    environment:
      - POSTGRES_PASSWORD=pass
