# Local Development with Docker Compose

This guide explains how to run the Arcadio platform locally using Docker Compose.

## Prerequisites

- Docker and Docker Compose installed
- OpenSSL (for generating registry authentication keys)

## Quick Start

### 1. Generate Registry Authentication Keys

The registry requires RSA key pairs for token-based authentication:

```bash
# Generate private key
openssl genrsa -out private_key.pem 2048

# Generate public key
openssl rsa -in private_key.pem -pubout -out public_key.pem
```

### 2. Set Up Environment Variables

Copy the example environment file:

```bash
cp .env.example .env
```

Edit `.env` and configure:

1. **GitHub OAuth** (required for login):
   - Go to https://github.com/settings/developers
   - Create a new OAuth App
   - Set Authorization callback URL to: `http://localhost:3000/auth/github/callback`
   - Copy Client ID and Secret to `.env`

2. **Registry Keys** (from step 1):
   - Copy the contents of `private_key.pem` to `REGISTRY_PRIVATE_KEY`
   - Copy the contents of `public_key.pem` to `REGISTRY_PUBLIC_KEY`
   - Replace newlines with `\n` in the key values

Example:
```bash
REGISTRY_PRIVATE_KEY="-----BEGIN RSA PRIVATE KEY-----\nMIIEpAIBAAK...\n-----END RSA PRIVATE KEY-----"
```

### 3. Start the Services

```bash
docker-compose up -d
```

This will start:
- **postgres** - PostgreSQL database on port 5432
- **registry** - Docker registry on port 5000
- **overseer** - Tournament manager gRPC service on port 50051
- **website** - Web application on port 3000

### 4. Access the Application

Open your browser to: http://localhost:3000

## Service Details

### PostgreSQL
- Port: 5432
- Database: `arcadio`
- User: `arcadio`
- Password: `arcadio`

### Registry
- Port: 5000
- Storage: Docker volume `registry_data`
- Authentication: Token-based (JWT)

### Overseer (Tournament Manager)
- Port: 50051
- Protocol: gRPC
- Manages agent deployment and tournaments

### Website
- Port: 3000
- Authentication: GitHub OAuth
- Database migrations run automatically on startup

## Useful Commands

### View logs
```bash
# All services
docker-compose logs -f

# Specific service
docker-compose logs -f website
```

### Rebuild services
```bash
# Rebuild all
docker-compose up -d --build

# Rebuild specific service
docker-compose up -d --build website
```

### Stop services
```bash
docker-compose down
```

### Stop and remove volumes (deletes data)
```bash
docker-compose down -v
```

### Access database
```bash
docker-compose exec postgres psql -U arcadio -d arcadio
```

### Check service health
```bash
docker-compose ps
```

## Development Workflow

### Making Code Changes

After making changes to Rust code:
```bash
# Rebuild and restart the affected service
docker-compose up -d --build website
# or
docker-compose up -d --build overseer
```

### Database Migrations

Migrations are automatically applied when the website service starts. Migration files are located in `apps/website/migrations/`.

### Registry Access

To push images to the local registry:
```bash
# Login (you'll need a valid token from the website)
docker login localhost:5000

# Tag your image
docker tag my-agent localhost:5000/username/agent-name:latest

# Push
docker push localhost:5000/username/agent-name:latest
```

## Troubleshooting

### Website fails to connect to database
- Ensure PostgreSQL is healthy: `docker-compose ps postgres`
- Check database logs: `docker-compose logs postgres`
- Verify DATABASE_URL in docker-compose.yml

### Registry authentication fails
- Verify REGISTRY_PRIVATE_KEY and REGISTRY_PUBLIC_KEY are set correctly in .env
- Ensure keys are properly formatted with `\n` for newlines
- Check registry logs: `docker-compose logs registry`

### GitHub OAuth not working
- Verify GITHUB_CLIENT_ID and GITHUB_CLIENT_SECRET are set
- Ensure callback URL in GitHub OAuth app is `http://localhost:3000/auth/github/callback`
- Check website logs: `docker-compose logs website`

### Service won't start
- Check if ports are already in use: `lsof -i :3000` (or other port)
- View service logs: `docker-compose logs <service-name>`
- Rebuild from scratch: `docker-compose down -v && docker-compose up -d --build`
